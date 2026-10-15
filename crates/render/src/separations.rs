//! Output Preview: a page's separations (one plate per ink), ink coverage, and a composite
//! that shows what the inks print, overprinting included.
//!
//! **How a plate is made.** The page is rendered once per ink with hayro-interpret's plate mode
//! (`InterpreterSettings::plate`, vendored patch 34): every paint draws that ink's tint as a
//! gray film instead of its colour, and a paint that overprints without specifying the ink
//! leaves the film as it is (ISO 32000-2 §8.6.7). Clipping, anti-aliasing, soft masks,
//! transparency groups and blending stay the renderer's own, so a plate is the page as drawn,
//! one ink at a time. Which inks a colour specifies, and with what tint, follows the colour
//! space: DeviceCMYK and four-component ICC colours are separated as they are; gray paints
//! black only; RGB and CIE-based colours go through sRGB and ISO 32000-2 §10.4.2.4 (black
//! generation and undercolour removal the identity); `Separation` and `DeviceN` colorants paint
//! their own plates (the process names the process plates, `All` every plate, `None` nothing).
//! See the crate README for what is approximated.

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::color::{Ink, InkSinkFn, Plate, cmyk_to_srgb8};
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings, render_into, render_size};

use crate::raster::{MAX_SIDE, device_pixels, effective_scale, panic_message};
use crate::{PageRenderer, Pixels};

/// Spot inks separated per page, after the four process inks. More than any press runs: a
/// `DeviceN` space has at most 32 colorants, and every plate costs a render and a byte per
/// pixel. Further spot inks are listed in [`Separations::unseparated`].
pub const MAX_SPOT_INKS: usize = 32;

/// Device pixels a separation may hold per plate (4 MP: 2000 × 2000), so a page with many
/// inks stays within a few hundred megabytes. The scale is reduced to fit.
pub const MAX_PLATE_PIXELS: f32 = 4.0e6;

/// The scale [`PageRenderer::page_inks`] renders at to find the inks a page uses.
const DISCOVERY_SCALE: f32 = 0.1;

/// An ink as Output Preview lists it: a process ink or a spot colour, and how it looks.
#[derive(Clone, Debug, PartialEq)]
pub struct InkInfo {
    ink: Ink,
    /// The ink at 100% in sRGB, for a swatch.
    pub swatch: [u8; 3],
    /// The ink alone at each tint (index: tint 0–255) in sRGB, from the colour space's
    /// alternate space and tint transform. Process inks compose through the CMYK profile
    /// instead and leave this empty.
    appearance: Arc<[[u8; 3]]>,
}

impl InkInfo {
    fn process(ink: Ink) -> Self {
        let swatch = match ink {
            Ink::Cyan => cmyk_rgb([255, 0, 0, 0]),
            Ink::Magenta => cmyk_rgb([0, 255, 0, 0]),
            Ink::Yellow => cmyk_rgb([0, 0, 255, 0]),
            _ => cmyk_rgb([0, 0, 0, 255]),
        };
        Self { ink, swatch, appearance: Arc::from(Vec::new()) }
    }

    /// The ink's name: `Cyan` … `Black`, or the spot colour's own name.
    pub fn name(&self) -> &str {
        self.ink.name()
    }

    /// A spot colour rather than a process ink.
    pub fn is_spot(&self) -> bool {
        matches!(self.ink, Ink::Spot(_))
    }
}

/// One CMYK colour in sRGB through the profile pages are drawn with; black if it fails.
fn cmyk_rgb(cmyk: [u8; 4]) -> [u8; 3] {
    let mut rgb = [0; 3];
    match cmyk_to_srgb8(&cmyk, &mut rgb) {
        Some(()) => rgb,
        None => [0; 3],
    }
}

/// Coverage of each ink and in total at one point, in percent.
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    /// Each ink's tint, in [`Separations::inks`] order, 0–100.
    pub inks: Vec<f32>,
    /// The sum of the enabled inks' tints (total area coverage), 0–100 × inks.
    pub total: f32,
}

/// Per-ink figures for a whole page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InkStats {
    /// The highest tint anywhere on the page, in percent.
    pub max: f32,
    /// The share of the page the ink prints on (any tint above zero), in percent.
    pub area: f32,
}

/// A page separated into plates, one per ink.
#[derive(Clone, Debug)]
pub struct Separations {
    pub page: usize,
    pub width: u32,
    pub height: u32,
    /// Device pixels per point the plates were rendered at.
    pub scale: f32,
    /// Whether the plates honour overprinting (`OP`, `op`, `OPM`), as a separating device
    /// does. Without it every paint knocks out what is under it.
    pub overprint: bool,
    /// The process inks, then the spot inks the page uses, in the order first painted.
    pub inks: Vec<InkInfo>,
    /// Spot inks the page uses past [`MAX_SPOT_INKS`]: listed, not separated.
    pub unseparated: Vec<String>,
    /// One plate per ink, in `inks` order: the ink's tint per pixel, 0 (none) to 255 (solid),
    /// row-major.
    plates: Vec<Vec<u8>>,
}

/// Whether ink `index` is enabled; inks past the end of `enabled` are.
fn on(enabled: &[bool], index: usize) -> bool {
    enabled.get(index).copied().unwrap_or(true)
}

fn percent(tint: u32) -> f32 {
    tint as f32 * 100.0 / 255.0
}

impl Separations {
    /// Ink `index`'s plate: its tint per pixel, row-major.
    pub fn plate(&self, index: usize) -> Option<&[u8]> {
        self.plates.get(index).map(Vec::as_slice)
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width && y < self.height).then(|| y as usize * self.width as usize + x as usize)
    }

    /// The pixel under a point given in `0.0..1.0` fractions of the page's width and height.
    pub fn pixel_at(&self, u: f32, v: f32) -> Option<(u32, u32)> {
        if !(u.is_finite() && v.is_finite()) || !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let x = ((u * self.width as f32) as u32).min(self.width.saturating_sub(1));
        let y = ((v * self.height as f32) as u32).min(self.height.saturating_sub(1));
        Some((x, y))
    }

    /// Each ink's tint and the total of the enabled inks at pixel (`x`, `y`).
    pub fn coverage_at(&self, x: u32, y: u32, enabled: &[bool]) -> Option<Coverage> {
        let at = self.pixel_index(x, y)?;
        let tints: Vec<u32> = self.plates.iter().map(|p| p.get(at).copied().map_or(0, u32::from)).collect();
        let total = tints.iter().enumerate().filter(|(i, _)| on(enabled, *i)).map(|(_, t)| *t).sum();
        Some(Coverage { inks: tints.into_iter().map(percent).collect(), total: percent(total) })
    }

    /// The total coverage of the enabled inks at every pixel, as a sum of tint bytes.
    fn totals(&self, enabled: &[bool]) -> Vec<u32> {
        let mut totals = vec![0_u32; self.plates.first().map_or(0, Vec::len)];
        for (_, plate) in self.plates.iter().enumerate().filter(|(i, _)| on(enabled, *i)) {
            for (sum, tint) in totals.iter_mut().zip(plate) {
                *sum = sum.saturating_add(u32::from(*tint));
            }
        }
        totals
    }

    /// The page's highest total area coverage of the enabled inks, in percent, and a pixel
    /// where it occurs; `None` for an empty raster.
    pub fn max_total(&self, enabled: &[bool]) -> Option<(f32, u32, u32)> {
        let totals = self.totals(enabled);
        let (at, max) = totals.iter().enumerate().max_by_key(|(_, t)| **t)?;
        let width = usize::try_from(self.width).ok().filter(|w| *w > 0)?;
        let (x, y) = (u32::try_from(at % width).ok()?, u32::try_from(at / width).ok()?);
        Some((percent(*max), x, y))
    }

    /// Which pixels' total coverage of the enabled inks is above `limit` percent.
    pub fn above(&self, limit: f32, enabled: &[bool]) -> Vec<bool> {
        let limit = if limit.is_finite() { limit.max(0.0) } else { f32::MAX };
        self.totals(enabled).into_iter().map(|t| percent(t) > limit).collect()
    }

    /// Each ink's highest tint and printed area over the page.
    pub fn stats(&self) -> Vec<InkStats> {
        let pixels = (self.width as usize).saturating_mul(self.height as usize).max(1);
        self.plates
            .iter()
            .map(|plate| {
                let max = plate.iter().copied().max().unwrap_or(0);
                let inked = plate.iter().filter(|t| **t > 0).count();
                InkStats { max: percent(u32::from(max)), area: inked as f32 * 100.0 / pixels as f32 }
            })
            .collect()
    }

    /// What the enabled inks print, as opaque premultiplied RGBA8: the process plates through
    /// the CMYK profile pages are drawn with, each spot ink multiplied over them by its own
    /// appearance (how a spot ink printed over others is usually simulated). Pixels whose total
    /// coverage is above `warn`'s limit are painted in `warn`'s colour instead.
    pub fn composite(&self, enabled: &[bool], warn: Option<(f32, [u8; 3])>) -> Pixels {
        let n = (self.width as usize).saturating_mul(self.height as usize);
        let process = |k: usize| self.plates.get(k).filter(|_| on(enabled, k));
        let mut cmyk = vec![0_u8; n.saturating_mul(4)];
        for k in 0..4 {
            if let Some(plate) = process(k) {
                for (px, tint) in cmyk.as_chunks_mut::<4>().0.iter_mut().zip(plate) {
                    if let Some(c) = px.get_mut(k) {
                        *c = *tint;
                    }
                }
            }
        }
        let mut rgb = vec![255_u8; n.saturating_mul(3)];
        if cmyk_to_srgb8(&cmyk, &mut rgb).is_none() {
            rgb.fill(255);
        }
        drop(cmyk);
        for (i, info) in self.inks.iter().enumerate().skip(4).filter(|(i, _)| on(enabled, *i)) {
            let Some(plate) = self.plates.get(i) else { continue };
            for (px, tint) in rgb.as_chunks_mut::<3>().0.iter_mut().zip(plate) {
                if *tint == 0 {
                    continue;
                }
                if let Some(ink) = info.appearance.get(usize::from(*tint)) {
                    for (c, s) in px.iter_mut().zip(ink) {
                        *c = ((u16::from(*c) * u16::from(*s) + 127) / 255) as u8;
                    }
                }
            }
        }
        let over = warn.map(|(limit, colour)| (self.above(limit, enabled), colour));
        let words: Vec<u32> = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .map(|(i, px)| {
                let [r, g, b] = match &over {
                    Some((mask, colour)) if mask.get(i).copied().unwrap_or(false) => *colour,
                    _ => *px,
                };
                u32::from_ne_bytes([r, g, b, 255])
            })
            .collect();
        Pixels::from(words)
    }

    /// Ink `index`'s plate as film for display: opaque gray, black where the ink is solid.
    pub fn film(&self, index: usize) -> Option<Pixels> {
        let plate = self.plates.get(index)?;
        Some(Pixels::from(plate.iter().map(|t| u32::from_ne_bytes([255 - t, 255 - t, 255 - t, 255])).collect::<Vec<_>>()))
    }
}

/// Spot inks reported while a page renders: name order kept, each ink's appearance sampled
/// the first time it is seen.
#[derive(Default)]
struct SpotLog {
    order: Vec<InkInfo>,
    seen: HashSet<Ink>,
}

fn spot_sink(log: &Arc<Mutex<SpotLog>>) -> InkSinkFn {
    let log = log.clone();
    Arc::new(move |ink: &Ink, appearance: &dyn Fn(f32) -> [u8; 3]| {
        let mut log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if log.seen.contains(ink) || log.order.len() > MAX_SPOT_INKS * 4 {
            return;
        }
        let table: Vec<[u8; 3]> = (0..=255_u8).map(|t| appearance(f32::from(t) / 255.0)).collect();
        let swatch = table.last().copied().unwrap_or([0; 3]);
        log.seen.insert(ink.clone());
        log.order.push(InkInfo { ink: ink.clone(), swatch, appearance: Arc::from(table) });
    })
}

/// Render page `page` of `pdf` at `scale` on white with `settings`: (width, height, pixels).
/// Never panics: a renderer panic is an `Err` whose flag is `true`.
fn render_whole<'a>(
    pdf: &'a Pdf,
    cache: &RenderCache<'a>,
    settings: &InterpreterSettings,
    page: usize,
    scale: f32,
) -> Result<(u32, u32, Pixels), (String, bool)> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let pages = pdf.pages();
        let p = pages.get(page).ok_or_else(|| format!("page {} does not exist", page + 1))?;
        let (w, h) = p.render_dimensions();
        if !(w.is_finite() && h.is_finite()) || w < 0.5 || h < 0.5 {
            return Err(format!("page {} has an empty or invalid page box ({w}×{h} pt)", page + 1));
        }
        let side = |pt: f32| device_pixels(pt, scale).min(MAX_SIDE as u32) as u16;
        let rs = RenderSettings {
            x_scale: scale,
            y_scale: scale,
            width: Some(side(w)),
            height: Some(side(h)),
            bg_color: WHITE,
            color_replacement: None,
            ..Default::default()
        };
        let (pw, ph) = render_size(p, &rs);
        let mut pixels = Pixels::zeroed(pw.into(), ph.into()).ok_or_else(|| format!("page {} is too large to render", page + 1))?;
        render_into(p, cache, settings, &rs, pixels.bytes_mut());
        Ok((u32::from(pw), u32::from(ph), pixels))
    }));
    match result {
        Ok(r) => r.map_err(|e| (e, false)),
        Err(panic) => Err((format!("renderer crashed on page {}: {}", page + 1, panic_message(&panic)), true)),
    }
}

/// The scale a separation of a `w` × `h` pt page renders at: `scale`, within the renderer's
/// caps and [`MAX_PLATE_PIXELS`].
fn plate_scale(w: f32, h: f32, scale: f32) -> f32 {
    let capped = effective_scale(w, h, scale);
    let area = (w.max(1.0) * h.max(1.0)).max(1.0);
    let by_plate = (MAX_PLATE_PIXELS / area).sqrt();
    let s = capped.min(by_plate);
    if s.is_finite() && s > 0.0 { s } else { f32::MIN_POSITIVE }
}

impl PageRenderer {
    /// Page `page`'s size as rendered, in points.
    fn page_size(&self, page: usize) -> Result<(f32, f32), String> {
        let pdf = self.pdf.as_ref().ok_or("the document could not be parsed")?;
        let pages = pdf.pages();
        let p = pages.get(page).ok_or_else(|| format!("page {} does not exist", page + 1))?;
        Ok(p.render_dimensions())
    }

    /// The inks page `page` prints with: the four process inks (always listed, as a press runs
    /// them), then each spot ink its content paints, in the order first painted. Spot inks that
    /// only a soft mask uses are left out (masks don't print). Never panics.
    pub fn page_inks(&mut self, page: usize) -> Result<Vec<InkInfo>, String> {
        let spots = self.discover_spots(page, DISCOVERY_SCALE)?;
        let mut inks: Vec<InkInfo> = Ink::PROCESS.into_iter().map(InkInfo::process).collect();
        inks.extend(spots);
        Ok(inks)
    }

    /// The page's spot inks, found by rendering it at `scale` (within [`MAX_PLATE_PIXELS`]).
    fn discover_spots(&mut self, page: usize, scale: f32) -> Result<Vec<InkInfo>, String> {
        let (w, h) = self.page_size(page)?;
        let scale = plate_scale(w, h, scale);
        let log = Arc::new(Mutex::new(SpotLog::default()));
        let settings = InterpreterSettings { ink_sink: Some(spot_sink(&log)), plate: None, ..self.settings.clone() };
        let pdf = self.pdf.as_ref().ok_or("the document could not be parsed")?;
        let rendered = render_whole(pdf, &RenderCache::new(), &settings, page, scale);
        if let Err((e, panicked)) = rendered {
            if panicked {
                self.reparse();
            }
            return Err(e);
        }
        let log = std::mem::take(&mut *log.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        Ok(log.order)
    }

    /// Separate page `page` at `scale` device pixels per point (reduced to the renderer's caps
    /// and [`MAX_PLATE_PIXELS`]): one plate per ink of [`page_inks`](Self::page_inks).
    /// `overprint` honours the overprint parameters as a separating device does. Never panics.
    pub fn separate(&mut self, page: usize, scale: f32, overprint: bool) -> Result<Separations, String> {
        let (w, h) = self.page_size(page)?;
        let scale = plate_scale(w, h, scale);
        // The plates render at the separation's scale; spot inks are found at a small one
        // first, so that each plate is rendered once.
        let mut spots = self.discover_spots(page, DISCOVERY_SCALE.min(scale))?;
        let unseparated: Vec<String> = spots.iter().skip(MAX_SPOT_INKS).map(|i| i.name().to_string()).collect();
        spots.truncate(MAX_SPOT_INKS);
        let mut inks: Vec<InkInfo> = Ink::PROCESS.into_iter().map(InkInfo::process).collect();
        inks.extend(spots);

        let pdf = self.pdf.as_ref().ok_or("the document could not be parsed")?;
        let mut plates = Vec::with_capacity(inks.len());
        let mut size = None;
        let mut failure = None;
        // One cache for every plate: fonts and masks are shared; plate images skip the decoded
        // image cache, and shadings and patterns on a plate are keyed by it.
        let cache = RenderCache::new();
        for info in &inks {
            let settings = InterpreterSettings { plate: Some(Plate { ink: info.ink.clone(), overprint }), ink_sink: None, ..self.settings.clone() };
            match render_whole(pdf, &cache, &settings, page, scale) {
                Ok((pw, ph, pixels)) => {
                    if size.is_some_and(|s| s != (pw, ph)) {
                        failure = Some((format!("page {} changed size between plates", page + 1), false));
                        break;
                    }
                    size = Some((pw, ph));
                    // The film is gray on white: its red channel is the paper left uncovered.
                    plates.push(pixels.as_chunks::<4>().0.iter().map(|[red, ..]| 255 - red).collect::<Vec<u8>>());
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        drop(cache);
        if let Some((e, panicked)) = failure {
            if panicked {
                self.reparse();
            }
            return Err(e);
        }
        let (width, height) = size.unwrap_or((0, 0));
        Ok(Separations { page, width, height, scale, overprint, inks, unseparated, plates })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{RenderConfig, RenderRequest, RequestKind};

    /// A 40 × 40 pt page with `content` and `resources`, and `objects` numbered from 5 (each a
    /// whole object body: a dictionary, or a dictionary and its stream).
    fn page_pdf(content: &str, resources: &str, objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n".to_vec();
        out.extend_from_slice(b"1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n");
        out.extend_from_slice(b"2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n");
        out.extend_from_slice(
            format!("3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Contents 4 0 R /Resources << {resources} >> >> endobj\n").as_bytes(),
        );
        out.extend_from_slice(format!("4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n", content.len()).as_bytes());
        for (i, body) in objects.iter().enumerate() {
            out.extend_from_slice(format!("{} 0 obj ", i + 5).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b" endobj\n");
        }
        out.extend_from_slice(b"trailer << /Root 1 0 R >>\n%%EOF");
        out
    }

    /// A stream object body with `dict` entries and `data`.
    fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut out = format!("<< {dict} /Length {} >> stream\n", data.len()).into_bytes();
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream");
        out
    }

    fn renderer(pdf: Vec<u8>) -> PageRenderer {
        PageRenderer::new(Arc::new(pdf), RenderConfig::default())
    }

    fn separate(pdf: Vec<u8>, overprint: bool) -> Separations {
        renderer(pdf).separate(0, 1.0, overprint).expect("separates")
    }

    fn names(s: &Separations) -> Vec<&str> {
        s.inks.iter().map(InkInfo::name).collect()
    }

    /// Each ink's coverage at device pixel (x, y), in whole percent.
    fn at(s: &Separations, x: u32, y: u32) -> Vec<u32> {
        s.coverage_at(x, y, &[]).expect("on the page").inks.iter().map(|c| c.round() as u32).collect()
    }

    /// A spot colour whose alternate is a yellowish CMYK.
    const GOLD: &str = "[/Separation /Gold /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0.3 1 0] /N 1 >>]";

    fn gold_resource() -> String {
        format!("/ColorSpace << /CS0 {GOLD} >>")
    }

    #[test]
    fn device_cmyk_puts_each_component_on_its_own_plate() {
        let s = separate(page_pdf("0.2 0.4 0.6 0.8 k 0 0 40 40 re f", "", &[]), true);
        assert_eq!(names(&s), ["Cyan", "Magenta", "Yellow", "Black"]);
        assert_eq!((s.width, s.height), (40, 40));
        assert_eq!(at(&s, 20, 20), [20, 40, 60, 80]);
        assert!((s.coverage_at(20, 20, &[]).unwrap().total - 200.0).abs() < 1.0);
    }

    #[test]
    fn a_spot_colour_paints_its_own_plate_and_knocks_out_the_process_plates() {
        let content = "0 0 0 1 k 0 0 40 40 re f /CS0 cs 0.5 scn 10 10 20 20 re f";
        let s = separate(page_pdf(content, &gold_resource(), &[]), true);
        assert_eq!(names(&s), ["Cyan", "Magenta", "Yellow", "Black", "Gold"]);
        assert_eq!(at(&s, 20, 20), [0, 0, 0, 0, 50], "knocked out under the spot colour");
        assert_eq!(at(&s, 2, 2), [0, 0, 0, 100, 0]);
    }

    #[test]
    fn overprinting_keeps_the_plates_a_colour_does_not_specify() {
        let resources = format!("{} /ExtGState << /GS1 << /op true >> >>", gold_resource());
        let content = "0 0 0 1 k 0 0 40 40 re f /GS1 gs /CS0 cs 0.5 scn 10 10 20 20 re f";
        let pdf = page_pdf(content, &resources, &[]);
        assert_eq!(at(&separate(pdf.clone(), true), 20, 20), [0, 0, 0, 100, 50], "overprinted");
        assert_eq!(at(&separate(pdf, false), 20, 20), [0, 0, 0, 0, 50], "overprint simulation off: knocked out");
    }

    #[test]
    fn op_sets_stroke_overprint_and_fill_overprint_too_without_op() {
        let resources = format!("{} /ExtGState << /GS1 << /OP true >> /GS2 << /OP true /op false >> >>", gold_resource());
        let fill = |gs: &str| page_pdf(&format!("0 0 0 1 k 0 0 40 40 re f /{gs} gs /CS0 cs 1 scn 10 10 20 20 re f"), &resources, &[]);
        assert_eq!(at(&separate(fill("GS1"), true), 20, 20)[3], 100, "OP alone also sets op");
        assert_eq!(at(&separate(fill("GS2"), true), 20, 20)[3], 0, "op false keeps fills knocking out");
        let stroke = page_pdf("0 0 0 1 k 0 0 40 40 re f /GS2 gs /CS0 CS 1 SCN 8 w 0 20 m 40 20 l S", &resources, &[]);
        assert_eq!(at(&separate(stroke, true), 20, 20), [0, 0, 0, 100, 100], "strokes overprint with OP");
    }

    #[test]
    fn nonzero_overprint_mode_leaves_zero_device_cmyk_components() {
        let resources = "/ExtGState << /M1 << /OP true /OPM 1 >> /M0 << /OP true /OPM 0 >> >>";
        let pdf = |gs: &str| page_pdf(&format!("1 0 0 0 k 0 0 40 40 re f /{gs} gs 0 1 0 0 k 10 10 20 20 re f"), resources, &[]);
        assert_eq!(at(&separate(pdf("M1"), true), 20, 20), [100, 100, 0, 0], "OPM 1: the zero cyan leaves the plate");
        assert_eq!(at(&separate(pdf("M0"), true), 20, 20), [0, 100, 0, 0], "OPM 0: zero components paint");
    }

    #[test]
    fn gray_and_rgb_are_process_colour_and_never_take_the_nonzero_overprint_mode() {
        let resources = "/ExtGState << /M1 << /OP true /OPM 1 >> >>";
        let gray = page_pdf("1 0 0 0 k 0 0 40 40 re f /M1 gs 0.25 g 10 10 20 20 re f", resources, &[]);
        assert_eq!(at(&separate(gray, true), 20, 20), [0, 0, 0, 75], "gray is black only, and knocks out cyan");
        let s = separate(page_pdf("1 0 0 rg 0 0 20 40 re f 0.2 0.2 0.2 rg 20 0 20 40 re f", "", &[]), true);
        assert_eq!(at(&s, 10, 20), [0, 100, 100, 0], "red");
        assert_eq!(at(&s, 30, 20), [0, 0, 0, 80], "a neutral gray goes to black (10.4.2.4)");
    }

    #[test]
    fn devicen_components_go_to_the_plates_they_name() {
        let resources = "/ColorSpace << /CS0 [/DeviceN [/Cyan /Silver /None] /DeviceCMYK 5 0 R] >>";
        let function = stream("/FunctionType 4 /Domain [0 1 0 1 0 1] /Range [0 1 0 1 0 1 0 1]", b"{ pop 0.5 mul 0 0 }");
        let s = separate(page_pdf("/CS0 cs 0.3 0.7 1 scn 0 0 40 40 re f", resources, &[function]), true);
        assert_eq!(names(&s), ["Cyan", "Magenta", "Yellow", "Black", "Silver"]);
        assert_eq!(at(&s, 20, 20), [30, 0, 0, 0, 70]);
    }

    #[test]
    fn the_all_colorant_paints_every_plate_and_none_paints_nothing() {
        let gray = "/DeviceGray << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>";
        let resources = format!("/ColorSpace << /CS0 {GOLD} /Reg [/Separation /All {gray}] /No [/Separation /None {gray}] >>");
        let content = "/CS0 cs 0.2 scn 0 0 40 40 re f /Reg cs 1 scn 0 0 20 40 re f /No cs 1 scn 20 0 20 40 re f";
        let s = separate(page_pdf(content, &resources, &[]), true);
        assert_eq!(at(&s, 10, 20), [100, 100, 100, 100, 100], "registration colour");
        assert_eq!(at(&s, 30, 20), [0, 0, 0, 0, 20], "None leaves every plate as it is");
    }

    #[test]
    fn icc_based_cmyk_is_separated_as_its_components() {
        let profile = include_bytes!("../../../vendor/hayro-interpret/assets/CGATS001Compat-v2-micro.icc");
        let pdf = page_pdf("/CS0 cs 0.1 0.3 0.5 0.7 scn 0 0 40 40 re f", "/ColorSpace << /CS0 [/ICCBased 5 0 R] >>", &[stream("/N 4", profile)]);
        assert_eq!(at(&separate(pdf, true), 20, 20), [10, 30, 50, 70]);
    }

    #[test]
    fn images_are_separated_sample_by_sample() {
        let cmyk =
            stream("/Type /XObject /Subtype /Image /Width 2 /Height 1 /BitsPerComponent 8 /ColorSpace /DeviceCMYK", &[255, 0, 0, 0, 0, 0, 0, 128]);
        let s = separate(page_pdf("q 40 0 0 40 0 0 cm /Im1 Do Q", "/XObject << /Im1 5 0 R >>", &[cmyk]), true);
        assert_eq!(at(&s, 10, 20), [100, 0, 0, 0]);
        assert_eq!(at(&s, 30, 20), [0, 0, 0, 50]);
    }

    #[test]
    fn a_spot_colour_image_overprints_like_any_paint() {
        let image = stream(&format!("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace {GOLD}"), &[128]);
        let resources = "/XObject << /Im1 5 0 R >> /ExtGState << /GS1 << /op true >> >>";
        let pdf =
            |gs: &str| page_pdf(&format!("0 0 0 1 k 0 0 40 40 re f {gs} q 20 0 0 20 10 10 cm /Im1 Do Q"), resources, std::slice::from_ref(&image));
        assert_eq!(at(&separate(pdf("/GS1 gs"), true), 20, 20), [0, 0, 0, 100, 50]);
        assert_eq!(at(&separate(pdf(""), true), 20, 20), [0, 0, 0, 0, 50]);
    }

    #[test]
    fn shadings_are_separated_through_their_colour_space() {
        let shading = format!(
            "<< /ShadingType 2 /ColorSpace {GOLD} /Coords [0 0 40 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> /Extend [true true] >>"
        );
        let s = separate(page_pdf("0 0 0 1 k 0 0 40 40 re f /Sh1 sh", "/Shading << /Sh1 5 0 R >>", &[shading.into_bytes()]), true);
        let (left, right) = (at(&s, 4, 20), at(&s, 36, 20));
        assert!(left[4] < 20 && right[4] > 80, "a gradient on the spot plate: {left:?} {right:?}");
        assert_eq!((left[3], right[3]), (0, 0), "the shading knocks out the black under it");
    }

    #[test]
    fn an_uncoloured_pattern_paints_the_film_of_the_colour_it_is_used_with() {
        let cell = stream("/PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << >>", b"0 0 10 10 re f");
        let resources = format!("/ColorSpace << /PCS [/Pattern {GOLD}] >> /Pattern << /P1 5 0 R >>");
        let s = separate(page_pdf("0 0 0 1 k 0 0 40 40 re f /PCS cs 0.5 /P1 scn 0 0 40 40 re f", &resources, &[cell]), true);
        assert_eq!(at(&s, 20, 20), [0, 0, 0, 0, 50], "the pattern's spot colour, knocking out black");
    }

    #[test]
    fn soft_masks_keep_the_luminosity_of_their_own_colours() {
        // A mask drawn in blue lets about a tenth of the black through. Drawn as the black
        // plate's film, blue (no black ink) would read as white and let all of it through.
        let group =
            stream("/Type /XObject /Subtype /Form /BBox [0 0 40 40] /Group << /S /Transparency /CS /DeviceRGB >>", b"0 0 1 rg 0 0 40 40 re f");
        let resources = "/ExtGState << /SM << /SMask << /S /Luminosity /G 5 0 R >> >> >>";
        let s = separate(page_pdf("/SM gs 0 0 0 1 k 0 0 40 40 re f", resources, &[group]), true);
        let black = at(&s, 20, 20)[3];
        assert!((1..30).contains(&black), "black through a blue luminosity mask: {black}%");
    }

    #[test]
    fn page_inks_lists_spot_colours_in_the_order_first_painted_including_patterns_and_images() {
        let image = stream(&format!("/Type /XObject /Subtype /Image /Width 1 /Height 1 /BitsPerComponent 8 /ColorSpace {GOLD}"), &[255]);
        let cell = stream(
            "/PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << /ColorSpace << /S [/Separation /Silver /DeviceGray << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0.5] /N 1 >>] >> >>",
            b"/S cs 1 scn 0 0 5 5 re f",
        );
        let mask_group = stream(
            "/Type /XObject /Subtype /Form /BBox [0 0 40 40] /Group << /S /Transparency /CS /DeviceGray >> /Resources << /ColorSpace << /M [/Separation /MaskOnly /DeviceGray << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>] >> >>",
            b"/M cs 0.5 scn 0 0 40 40 re f",
        );
        let resources = "/XObject << /Im1 5 0 R >> /Pattern << /P1 6 0 R >> /ExtGState << /SM << /SMask << /S /Luminosity /G 7 0 R >> >> >>";
        let content = "/Pattern cs /P1 scn 0 0 20 20 re f q 10 0 0 10 25 25 cm /Im1 Do Q /SM gs 0 g 0 0 40 40 re f";
        let mut r = renderer(page_pdf(content, resources, &[image, cell, mask_group]));
        let inks = r.page_inks(0).unwrap();
        let names: Vec<&str> = inks.iter().map(InkInfo::name).collect();
        assert_eq!(names, ["Cyan", "Magenta", "Yellow", "Black", "Silver", "Gold"], "a mask's ink doesn't print");
        assert!(inks[5].is_spot() && !inks[3].is_spot());
        assert_eq!(inks[4].swatch, [128, 128, 128], "Silver at 100% is its alternate colour");
    }

    #[test]
    fn total_area_coverage_sums_the_enabled_inks() {
        let s = separate(page_pdf("0.6 0.5 0.5 1 k 0 0 20 40 re f 0.2 0 0 0 k 20 0 20 40 re f", "", &[]), true);
        let (max, x, _) = s.max_total(&[]).unwrap();
        assert!((max - 260.0).abs() < 1.5 && x < 20, "{max} at {x}");
        assert!((s.coverage_at(10, 20, &[false, true, true, true]).unwrap().total - 200.0).abs() < 1.5, "cyan left out");
        let over = s.above(250.0, &[]);
        assert!(over[20 * 40 + 10] && !over[20 * 40 + 30]);
        assert!(!s.above(270.0, &[]).iter().any(|o| *o));
        let stats = s.stats();
        assert!((stats[0].max - 60.0).abs() < 1.0 && (stats[0].area - 100.0).abs() < 0.1, "{:?}", stats[0]);
        assert!((stats[3].area - 50.0).abs() < 1.0, "black covers the left half: {:?}", stats[3]);
    }

    /// The page drawn as usual, at device pixel (x, y).
    fn drawn(pdf: Vec<u8>, x: usize, y: usize) -> [u8; 3] {
        let page = renderer(pdf).render(RenderRequest { page: 0, kind: RequestKind::Pixels, tile: None, scale: 1.0, tag: 0, preview: false });
        let px = &page.rgba[(y * 40 + x) * 4..][..3];
        [px[0], px[1], px[2]]
    }

    fn rgb(p: &Pixels, x: usize, y: usize) -> [u8; 3] {
        let px = &p[(y * 40 + x) * 4..][..4];
        assert_eq!(px[3], 255);
        [px[0], px[1], px[2]]
    }

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 3)
    }

    #[test]
    fn the_composite_of_process_and_spot_plates_looks_like_the_page() {
        let pdf = page_pdf("0.1 0.5 0.9 0.2 k 0 0 20 40 re f /CS0 cs 1 scn 20 0 20 40 re f", &gold_resource(), &[]);
        let s = separate(pdf.clone(), true);
        let composite = s.composite(&[], None);
        for x in [10, 30] {
            let (ours, page) = (rgb(&composite, x, 20), drawn(pdf.clone(), x, 20));
            assert!(close(ours, page), "x = {x}: composite {ours:?}, page {page:?}");
        }
        assert_eq!(rgb(&s.composite(&[true, true, true, true, false], None), 30, 20), [255, 255, 255], "Gold switched off");
    }

    #[test]
    fn the_composite_shows_overprinted_inks_mixed() {
        let resources = "/ExtGState << /M1 << /OP true /OPM 1 >> >>";
        let pdf = page_pdf("1 0 0 0 k 0 0 40 40 re f /M1 gs 0 1 0 0 k 10 10 20 20 re f", resources, &[]);
        let mixed = rgb(&separate(pdf.clone(), true).composite(&[], None), 20, 20);
        let knocked = rgb(&separate(pdf, false).composite(&[], None), 20, 20);
        assert!(close(mixed, cmyk_rgb([255, 255, 0, 0])), "blue where magenta overprints cyan: {mixed:?}");
        assert!(close(knocked, cmyk_rgb([0, 255, 0, 0])), "magenta without overprint simulation: {knocked:?}");
    }

    #[test]
    fn the_composite_marks_pixels_over_the_coverage_limit() {
        let s = separate(page_pdf("0.6 0.5 0.5 1 k 0 0 20 40 re f", "", &[]), true);
        let marked = s.composite(&[], Some((250.0, [0, 255, 0])));
        assert_eq!(rgb(&marked, 10, 20), [0, 255, 0]);
        assert_eq!(rgb(&marked, 30, 20), [255, 255, 255]);
        assert_eq!(s.film(3).map(|f| rgb(&f, 10, 20)), Some([0, 0, 0]), "solid black film");
    }

    #[test]
    fn hostile_input_is_an_error_not_a_panic() {
        assert!(renderer(b"not a pdf".to_vec()).separate(0, 1.0, true).is_err());
        assert!(renderer(page_pdf("0 g", "", &[])).separate(3, 1.0, true).is_err());
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 1e9] {
            let s = renderer(page_pdf("0 0 0 1 k 0 0 40 40 re f", "", &[])).separate(0, scale, true).expect("any scale separates");
            assert!(s.width >= 1 && (s.width as f32) * (s.height as f32) <= MAX_PLATE_PIXELS * 1.01, "{scale}: {}×{}", s.width, s.height);
        }
        // A page the size of a continent has its inks found at a bounded size.
        let huge = String::from_utf8_lossy(&page_pdf("/CS0 cs 1 scn 0 0 40 40 re f", &gold_resource(), &[]))
            .replace("/MediaBox [0 0 40 40]", "/MediaBox [0 0 14000000 14000000]")
            .into_bytes();
        assert_eq!(renderer(huge).page_inks(0).expect("inks").len(), 5);
        // Malformed colour spaces and overprint parameters draw what they can.
        let resources = "/ColorSpace << /A [/DeviceN [] /DeviceGray 0] /B [/Separation] /C [/Indexed /DeviceCMYK 300 <00>] >> /ExtGState << /G << /OP 7 /OPM /X /op (y) >> >>";
        let s = renderer(page_pdf("/G gs /A cs 1 scn 0 0 9 9 re f /B cs 1 scn /C cs 255 scn 0 0 40 40 re f", resources, &[])).separate(0, 1.0, true);
        let s = s.expect("separates");
        assert!(s.coverage_at(40, 0, &[]).is_none(), "past the right edge");
        assert!(s.pixel_at(1.0, 0.5).is_none() && s.pixel_at(f32::NAN, 0.0).is_none());
        assert_eq!(s.pixel_at(0.999, 0.0), Some((39, 0)));
    }
}
