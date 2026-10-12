//! OFD → PDF, **early alpha** (#1036). Each OFD page becomes a PDF page of the same size, drawn
//! from the [`pdfcraft_ofd`] model:
//!
//! - paths (fill, stroke, even-odd, colours, line width, opacity; arcs become Béziers);
//! - images through Create's own decoders ([`crate::image_xobject`]: PNG, JPEG, TIFF, GIF, BMP,
//!   JPEG 2000);
//! - text at OFD's glyph positions, as real text (selectable, searchable): with the document's
//!   embedded TrueType/OpenType font when it has the glyphs (by `CGTransform` glyph index or the
//!   font's cmap), otherwise with a standard font (Helvetica, Times, Courier) for WinAnsi text,
//!   or a non-embedded Adobe-GB1 font (`STSong-Light`) for Chinese, which readers substitute.
//!   No font file is added: only fonts the OFD file itself carries are embedded.
//!
//! Template pages and composite graphic units are written once each, as form XObjects that
//! every page or object using them draws. A unit is a transparency group, so its object's
//! `Alpha` fades it as a whole; a unit that contains itself, or units nested more than
//! [`MAX_UNIT_DEPTH`] deep, are not drawn again (reported).
//!
//! Paths, text and images are clipped to their `Clips`: each clip is a PDF clipping path (`W n`,
//! or `W* n` for an even-odd path) in page space, so several clips intersect. A clip made of one
//! path area is drawn exactly; one made of text or of several areas (a union, which PDF clipping
//! can't express) and an empty one are skipped: the object is drawn unclipped and the clip is
//! reported.
//!
//! What isn't drawn (composite units, shadings, JBIG2 and other undecodable images, the clips
//! above) is counted in [`OfdReport::skipped`], by kind in [`OfdReport::skipped_by_kind`].
//! Signatures and seals are not carried over or checked.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use pdfcraft_cos::{Dict, Document, ObjRef, Object, Stream};
use pdfcraft_fonts::{helvetica_width, win_ansi, win_ansi_encodable};
use pdfcraft_ofd::{
    ClipShape, Color, ColorSpaceKind, FillRule, Graphic, LayerKind, Matrix, Object as Item, PageContent, PathCmd, Resources, TemplatePage,
    TextObject, ZOrder, zip::Package,
};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider, instance::LocationRef, instance::Size};

use crate::{CreateError, MAX_SIDE, add_page, set_title};

/// How deep composite graphic units may nest in one another.
const MAX_UNIT_DEPTH: usize = 16;

/// What the conversion could not do, for the user.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OfdReport {
    pub pages: usize,
    /// Objects (or parts of objects) that were not drawn.
    pub skipped: usize,
    /// [`OfdReport::skipped`] by kind ("pattern and shading colours", "text used as a clipping
    /// region", …).
    pub skipped_by_kind: BTreeMap<String, usize>,
    /// The OFD file is signed: its signatures and seals are not in the PDF and were not checked.
    pub signed: bool,
    /// Details: the reader's warnings and what was skipped, once each.
    pub notes: Vec<String>,
}

/// Convert an OFD file to a new PDF document. Errors when the file can't be read at all.
pub fn from_ofd(bytes: &[u8]) -> Result<(Document, OfdReport), CreateError> {
    let bad = |e: pdfcraft_ofd::OfdError| CreateError::Invalid(e.to_string());
    // One package for the model, the images and the fonts: one unpacking budget.
    let limits = pdfcraft_ofd::Limits::default();
    let pkg = Package::open(bytes, &limits).map_err(bad)?;
    let model = pdfcraft_ofd::parse_package(&pkg, &limits).map_err(bad)?;
    let mut w = Writer {
        doc: Document::new_empty(),
        pkg: &pkg,
        res: &model.resources,
        fonts: HashMap::new(),
        cjk: None,
        standard: HashMap::new(),
        images: HashMap::new(),
        states: HashMap::new(),
        forms: HashMap::new(),
        units: HashMap::new(),
        unit_stack: Vec::new(),
        report: OfdReport { signed: model.signatures.is_some(), notes: model.warnings.clone(), ..OfdReport::default() },
    };
    let mut templates: HashMap<u32, &TemplatePage> = HashMap::new();
    for tp in &model.templates {
        templates.entry(tp.id).or_insert(tp);
    }
    for page in &model.pages {
        let (pw, ph) = page.area.physical.size_pt();
        if !(1.0..=MAX_SIDE).contains(&pw) || !(1.0..=MAX_SIDE).contains(&ph) {
            return Err(CreateError::Invalid(format!("{}: the page size {pw:.0} × {ph:.0} pt is outside what PDF allows", page.location)));
        }
        let mut out = PageOut::default();
        out.ops.push_str(&cm(&page.area.physical.page_to_pdf()));
        let template = |z: ZOrder| -> Vec<&TemplatePage> {
            page.templates
                .iter()
                .filter_map(|t| {
                    let tp = *templates.get(&t.template_id)?;
                    (t.z_order.unwrap_or(tp.z_order) == z).then_some(tp)
                })
                .collect()
        };
        for tp in template(ZOrder::Background) {
            w.template(tp, &mut out);
        }
        w.content(&page.content, &mut out);
        for tp in template(ZOrder::Foreground) {
            w.template(tp, &mut out);
        }
        let resources = out.resources();
        add_page(&mut w.doc, pw, ph, resources, Some(out.ops.into_bytes()))?;
        w.report.pages += 1;
    }
    w.finish_fonts();
    set_title(&mut w.doc, model.info.title.as_deref().unwrap_or("OFD document"));
    let mut report = w.report;
    if report.signed {
        report.notes.push("the OFD signatures and seals are not carried over or checked".into());
    }
    report.notes.dedup();
    Ok((w.doc, report))
}

/// An embedded font from the OFD package, written as a Type 0 font with Identity-H glyph codes.
struct Embedded {
    type0: ObjRef,
    cid: ObjRef,
    units_per_em: f64,
    /// Glyph → (advance in 1/1000 em, the text it stands for).
    used: BTreeMap<u16, (f64, String)>,
    bytes: Vec<u8>,
}

/// The resources and operators of one page being written.
#[derive(Default)]
struct PageOut {
    ops: String,
    fonts: BTreeMap<String, ObjRef>,
    xobjects: BTreeMap<String, ObjRef>,
    states: BTreeMap<String, ObjRef>,
}

impl PageOut {
    fn resources(&self) -> Dict {
        let dict = |m: &BTreeMap<String, ObjRef>| {
            let mut d = Dict::new();
            for (k, r) in m {
                d.set(k.as_bytes().to_vec(), Object::Ref(*r));
            }
            d
        };
        let mut res = Dict::new();
        res.set(b"Font".to_vec(), Object::Dict(dict(&self.fonts)));
        res.set(b"XObject".to_vec(), Object::Dict(dict(&self.xobjects)));
        res.set(b"ExtGState".to_vec(), Object::Dict(dict(&self.states)));
        res
    }
}

struct Writer<'a, 'b> {
    doc: Document,
    pkg: &'a Package<'b>,
    res: &'a Resources,
    /// By OFD font ID: the embedded font, or `None` when it couldn't be used.
    fonts: HashMap<u32, Option<Embedded>>,
    cjk: Option<ObjRef>,
    standard: HashMap<&'static str, ObjRef>,
    images: HashMap<u32, Option<ObjRef>>,
    states: HashMap<(u8, u8), ObjRef>,
    /// Template pages, each written once as a form XObject that every page using it draws.
    forms: HashMap<u32, ObjRef>,
    /// Composite graphic units as form XObjects, or why one can't be drawn.
    units: HashMap<u32, Result<ObjRef, &'static str>>,
    /// The units being written, innermost last (a unit may not contain itself).
    unit_stack: Vec<u32>,
    report: OfdReport,
}

/// A number for a content stream: finite, at most four decimals, no trailing zeros.
fn n(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.to_string() }
}

fn cm(m: &Matrix) -> String {
    format!("{} {} {} {} {} {} cm\n", n(m.a), n(m.b), n(m.c), n(m.d), n(m.e), n(m.f))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2 + 2);
    s.push('<');
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s.push('>');
    s
}

/// A PDF name made from a font name: ASCII letters and digits only.
fn pdf_name(name: &str, fallback: &str) -> String {
    let s: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(60).collect();
    if s.is_empty() { fallback.to_string() } else { s }
}

impl Writer<'_, '_> {
    fn skip(&mut self, what: &str) {
        self.report.skipped += 1;
        *self.report.skipped_by_kind.entry(what.to_string()).or_default() += 1;
        let note = format!("not drawn yet: {what}");
        if !self.report.notes.contains(&note) {
            self.report.notes.push(note);
        }
    }

    /// Draw a template page: a form XObject written the first time it is used, so a template on
    /// many pages costs its content once (and a small file can't multiply it by its page count).
    fn template(&mut self, tp: &TemplatePage, out: &mut PageOut) {
        let r = match self.forms.get(&tp.id) {
            Some(r) => *r,
            None => {
                let mut form = PageOut::default();
                self.content(&tp.content, &mut form);
                let mut d = Dict::new();
                d.set(b"Type".to_vec(), Object::name("XObject"));
                d.set(b"Subtype".to_vec(), Object::name("Form"));
                // Template content is in the page's OFD space (mm); the page clips it.
                d.set(
                    b"BBox".to_vec(),
                    Object::Array(vec![Object::Real(-MAX_SIDE), Object::Real(-MAX_SIDE), Object::Real(MAX_SIDE), Object::Real(MAX_SIDE)]),
                );
                d.set(b"Resources".to_vec(), Object::Dict(form.resources()));
                let r = self.doc.add(Object::Stream(Stream::flate(d, form.ops.as_bytes())));
                self.forms.insert(tp.id, r);
                r
            }
        };
        let name = format!("Tpl{}", tp.id);
        out.xobjects.insert(name.clone(), r);
        let _ = writeln!(out.ops, "/{name} Do");
    }

    /// A composite graphic unit as a form XObject (a transparency group), written the first
    /// time it is used; or why it can't be drawn.
    fn unit_form(&mut self, id: u32) -> Result<ObjRef, &'static str> {
        if let Some(r) = self.units.get(&id) {
            return *r;
        }
        if self.unit_stack.contains(&id) {
            return Err("composite graphic units that contain themselves");
        }
        if self.unit_stack.len() >= MAX_UNIT_DEPTH {
            return Err("composite graphic units nested too deep");
        }
        let res = self.res;
        let Some(unit) = res.composites.get(&id) else {
            self.units.insert(id, Err("composite graphic units that aren't defined"));
            return Err("composite graphic units that aren't defined");
        };
        self.unit_stack.push(id);
        let mut form = PageOut::default();
        self.objects(&unit.objects, None, &mut form);
        self.unit_stack.pop();
        let mut group = Dict::new();
        group.set(b"S".to_vec(), Object::name("Transparency"));
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("XObject"));
        d.set(b"Subtype".to_vec(), Object::name("Form"));
        // The unit's own space; what the placing object's boundary leaves out isn't cut off.
        d.set(
            b"BBox".to_vec(),
            Object::Array(vec![Object::Real(-MAX_SIDE), Object::Real(-MAX_SIDE), Object::Real(MAX_SIDE), Object::Real(MAX_SIDE)]),
        );
        d.set(b"Group".to_vec(), Object::Dict(group));
        d.set(b"Resources".to_vec(), Object::Dict(form.resources()));
        let r = self.doc.add(Object::Stream(Stream::flate(d, form.ops.as_bytes())));
        self.units.insert(id, Ok(r));
        Ok(r)
    }

    fn content(&mut self, content: &PageContent, out: &mut PageOut) {
        let rank = |k: LayerKind| match k {
            LayerKind::Background => 0,
            LayerKind::Body | LayerKind::Custom => 1,
            LayerKind::Foreground => 2,
        };
        let mut layers: Vec<_> = content.layers.iter().collect();
        layers.sort_by_key(|l| rank(l.kind));
        for layer in layers {
            self.objects(&layer.objects, layer.draw_param, out);
        }
    }

    fn objects(&mut self, objects: &[Item], layer_param: Option<u32>, out: &mut PageOut) {
        for o in objects {
            match o {
                Item::Path(p) if p.graphic.visible => {
                    let param = p.graphic.draw_param.or(layer_param);
                    let fill = p.fill.then(|| p.fill_color.clone().or_else(|| self.param_color(param, true))).flatten();
                    let stroke = p.stroke.then(|| p.stroke_color.clone().or_else(|| self.param_color(param, false)).unwrap_or_else(black));
                    let fill_ops = fill.as_ref().and_then(|c| self.color(c, true));
                    let stroke_ops = stroke.as_ref().and_then(|c| self.color(c, false));
                    if (p.fill && fill.as_ref().is_some_and(|c| c.shading)) || (p.stroke && stroke.as_ref().is_some_and(|c| c.shading)) {
                        self.skip("pattern and shading colours");
                    }
                    let paint = match (fill_ops.is_some(), stroke_ops.is_some(), p.rule) {
                        (true, true, FillRule::NonZero) => "B",
                        (true, true, FillRule::EvenOdd) => "B*",
                        (true, false, FillRule::NonZero) => "f",
                        (true, false, FillRule::EvenOdd) => "f*",
                        (false, true, _) => "S",
                        (false, false, _) => continue,
                    };
                    let line_width = p.graphic.line_width.or_else(|| self.param_line_width(param)).unwrap_or(0.353);
                    let mut s = String::from("q\n");
                    self.clip(&p.graphic, &mut s);
                    s.push_str(&cm(&p.graphic.to_page()));
                    self.alpha(&p.graphic, fill.as_ref(), stroke.as_ref(), out, &mut s);
                    let _ = writeln!(s, "{} w", n(line_width));
                    for c in [fill_ops, stroke_ops].into_iter().flatten() {
                        s.push_str(&c);
                    }
                    path_ops(&p.path, &Matrix::IDENTITY, &mut s);
                    let _ = writeln!(s, "{paint}\nQ");
                    out.ops.push_str(&s);
                }
                Item::Text(t) if t.graphic.visible => self.text(t, layer_param, out),
                Item::Image(i) if i.graphic.visible => {
                    let Some(r) = self.image(i.resource_id) else { continue };
                    let name = format!("Im{}", i.resource_id);
                    out.xobjects.insert(name.clone(), r);
                    let mut s = String::from("q\n");
                    self.clip(&i.graphic, &mut s);
                    s.push_str(&cm(&i.graphic.to_page()));
                    self.alpha(&i.graphic, None, None, out, &mut s);
                    // OFD's image space runs down the page; PDF's unit square runs up.
                    let _ = writeln!(s, "1 0 0 -1 0 1 cm\n/{name} Do\nQ");
                    out.ops.push_str(&s);
                }
                Item::Composite(c) if c.graphic.visible => {
                    let r = match self.unit_form(c.resource_id) {
                        Ok(r) => r,
                        Err(why) => {
                            self.skip(why);
                            continue;
                        }
                    };
                    let name = format!("Cu{}", c.resource_id);
                    out.xobjects.insert(name.clone(), r);
                    let mut s = String::from("q\n");
                    self.clip(&c.graphic, &mut s);
                    s.push_str(&cm(&c.graphic.to_page()));
                    self.alpha(&c.graphic, None, None, out, &mut s);
                    let _ = writeln!(s, "/{name} Do\nQ");
                    out.ops.push_str(&s);
                }
                Item::Block(b) => self.objects(&b.objects, layer_param, out),
                _ => {}
            }
        }
    }

    /// Clip to `g`'s clips, in page space: call it right after `q`, before the object's `cm`.
    /// A clip this can't draw is skipped and reported, and the object is drawn without it.
    fn clip(&mut self, g: &Graphic, s: &mut String) {
        for clip in &g.clips {
            let area = match clip.areas.as_slice() {
                [area] => area,
                [] => {
                    self.skip("empty clipping regions");
                    continue;
                }
                _ => {
                    self.skip("clipping regions made of several areas");
                    continue;
                }
            };
            match &area.shape {
                ClipShape::Path(p) if !p.path.is_empty() => {
                    path_ops(&p.path, &p.graphic.to_page().then(&g.clip_to_page(area)), s);
                    s.push_str(if p.rule == FillRule::EvenOdd { "W* n\n" } else { "W n\n" });
                }
                ClipShape::Text => self.skip("text used as a clipping region"),
                ClipShape::Path(_) | ClipShape::Empty => self.skip("empty clipping regions"),
            }
        }
    }

    fn param(&self, mut id: Option<u32>, mut pick: impl FnMut(&pdfcraft_ofd::DrawParam) -> bool) {
        // Follow `Relative` a bounded number of times (cycles are possible).
        for _ in 0..8 {
            let Some(p) = id.and_then(|i| self.res.draw_params.get(&i)) else { return };
            if pick(p) {
                return;
            }
            id = p.relative;
        }
    }

    fn param_color(&self, id: Option<u32>, fill: bool) -> Option<Color> {
        let mut found = None;
        self.param(id, |p| {
            found = if fill { p.fill_color.clone() } else { p.stroke_color.clone() };
            found.is_some()
        });
        found
    }

    fn param_line_width(&self, id: Option<u32>) -> Option<f64> {
        let mut found = None;
        self.param(id, |p| {
            found = p.line_width;
            found.is_some()
        });
        found
    }

    /// The colour operator for `c`, or `None` for a pattern/shading or an unusable value.
    fn color(&self, c: &Color, fill: bool) -> Option<String> {
        if c.shading || c.value.is_empty() {
            return None;
        }
        let space = c.color_space.and_then(|id| self.res.color_spaces.get(&id));
        let bits = space.map_or(8, |s| s.bits_per_component).clamp(1, 16);
        let max = f64::from((1u32 << bits) - 1);
        let v: Vec<String> = c.value.iter().map(|x| n((x / max).clamp(0.0, 1.0))).collect();
        let kind = space.map(|s| &s.kind);
        let op = match (kind, v.len()) {
            (Some(ColorSpaceKind::Gray), 1) | (None, 1) => "g",
            (Some(ColorSpaceKind::Cmyk), 4) | (None, 4) => "k",
            (_, 3) => "rg",
            _ => return None,
        };
        let op = if fill { op.to_string() } else { op.to_uppercase() };
        Some(format!("{} {op}\n", v.join(" ")))
    }

    /// Opacity from the object's `Alpha` and its colours' `Alpha`, as an ExtGState.
    fn alpha(&mut self, g: &Graphic, fill: Option<&Color>, stroke: Option<&Color>, out: &mut PageOut, s: &mut String) {
        let mix = |c: Option<&Color>| (u16::from(g.alpha) * u16::from(c.map_or(255, |c| c.alpha)) / 255) as u8;
        let key = (mix(fill), mix(stroke));
        if key == (255, 255) {
            return;
        }
        let r = *self.states.entry(key).or_insert_with(|| {
            let mut d = Dict::new();
            d.set(b"Type".to_vec(), Object::name("ExtGState"));
            d.set(b"ca".to_vec(), Object::Real(f64::from(key.0) / 255.0));
            d.set(b"CA".to_vec(), Object::Real(f64::from(key.1) / 255.0));
            self.doc.add(d)
        });
        let name = format!("GS{}_{}", key.0, key.1);
        out.states.insert(name.clone(), r);
        let _ = writeln!(s, "/{name} gs");
    }

    fn image(&mut self, id: u32) -> Option<ObjRef> {
        if let Some(r) = self.images.get(&id) {
            return *r;
        }
        let r = match self.res.media.get(&id) {
            None => None,
            Some(m) => match self.pkg.read(&m.file) {
                Ok(bytes) => crate::image_xobject(&mut self.doc, &m.file, &bytes).ok().map(|(r, _)| r),
                Err(_) => None,
            },
        };
        if r.is_none() {
            self.skip("images in formats PdfCraft can't decode (e.g. JBIG2) or missing image files");
        }
        self.images.insert(id, r);
        r
    }

    fn embedded(&mut self, id: u32) -> Option<&mut Embedded> {
        if !self.fonts.contains_key(&id) {
            let font = self.load_font(id);
            self.fonts.insert(id, font);
        }
        self.fonts.get_mut(&id).and_then(Option::as_mut)
    }

    fn load_font(&mut self, id: u32) -> Option<Embedded> {
        let f = self.res.fonts.get(&id)?;
        let bytes = self.pkg.read(f.file.as_deref()?).ok()?;
        let font = FontRef::new(&bytes).ok()?;
        let truetype = font.glyf().is_ok();
        if !truetype && font.cff().is_err() {
            return None;
        }
        let units_per_em = f64::from(font.head().ok()?.units_per_em());
        if units_per_em <= 0.0 {
            return None;
        }
        let name = pdf_name(&f.name, &format!("OFDFont{id}"));
        let mut file = Dict::new();
        if truetype {
            file.set(b"Length1".to_vec(), Object::Int(bytes.len() as i64));
        } else {
            file.set(b"Subtype".to_vec(), Object::name("OpenType"));
        }
        let file = self.doc.add(Object::Stream(Stream::flate(file, &bytes)));
        let mut fd = Dict::new();
        fd.set(b"Type".to_vec(), Object::name("FontDescriptor"));
        fd.set(b"FontName".to_vec(), Object::name(&name));
        fd.set(b"Flags".to_vec(), Object::Int(4));
        fd.set(b"FontBBox".to_vec(), Object::Array(vec![0.into(), (-200).into(), 1000.into(), 900.into()]));
        fd.set(b"ItalicAngle".to_vec(), Object::Int(0));
        fd.set(b"Ascent".to_vec(), Object::Int(880));
        fd.set(b"Descent".to_vec(), Object::Int(-120));
        fd.set(b"CapHeight".to_vec(), Object::Int(700));
        fd.set(b"StemV".to_vec(), Object::Int(80));
        fd.set(if truetype { b"FontFile2".to_vec() } else { b"FontFile3".to_vec() }, Object::Ref(file));
        let fd = self.doc.add(fd);
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::name("Font"));
        cid.set(b"Subtype".to_vec(), Object::name(if truetype { "CIDFontType2" } else { "CIDFontType0" }));
        cid.set(b"BaseFont".to_vec(), Object::name(&name));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(system_info("Identity", 0)));
        cid.set(b"FontDescriptor".to_vec(), Object::Ref(fd));
        if truetype {
            cid.set(b"CIDToGIDMap".to_vec(), Object::name("Identity"));
        }
        let cid = self.doc.add(cid);
        let mut t0 = Dict::new();
        t0.set(b"Type".to_vec(), Object::name("Font"));
        t0.set(b"Subtype".to_vec(), Object::name("Type0"));
        t0.set(b"BaseFont".to_vec(), Object::name(&name));
        t0.set(b"Encoding".to_vec(), Object::name("Identity-H"));
        t0.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Ref(cid)]));
        let type0 = self.doc.add(t0);
        Some(Embedded { type0, cid, units_per_em, used: BTreeMap::new(), bytes })
    }

    /// Widths and ToUnicode for every embedded font, now that the glyphs used are known.
    fn finish_fonts(&mut self) {
        for font in self.fonts.values().flatten() {
            let mut w = Vec::new();
            for (gid, (adv, _)) in &font.used {
                w.push(Object::Int(i64::from(*gid)));
                w.push(Object::Array(vec![Object::Real(*adv)]));
            }
            let _ = self.doc.update_dict(font.cid, |d| {
                d.set(b"DW".to_vec(), Object::Int(1000));
                d.set(b"W".to_vec(), Object::Array(w));
            });
            let mut cmap = String::from(
                "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
            );
            let entries: Vec<_> = font.used.iter().collect();
            for chunk in entries.chunks(100) {
                let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
                for (gid, (_, text)) in chunk {
                    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
                    let _ = writeln!(cmap, "{} {}", hex(&gid.to_be_bytes()), hex(&utf16));
                }
                cmap.push_str("endbfchar\n");
            }
            cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
            let r = self.doc.add(Object::Stream(Stream::flate(Dict::new(), cmap.as_bytes())));
            let _ = self.doc.update_dict(font.type0, |d| d.set(b"ToUnicode".to_vec(), Object::Ref(r)));
        }
    }

    /// The shared non-embedded Chinese font (Adobe-GB1, Unicode encoding).
    fn cjk(&mut self) -> ObjRef {
        if let Some(r) = self.cjk {
            return r;
        }
        let mut fd = Dict::new();
        fd.set(b"Type".to_vec(), Object::name("FontDescriptor"));
        fd.set(b"FontName".to_vec(), Object::name("STSong-Light"));
        fd.set(b"Flags".to_vec(), Object::Int(6));
        fd.set(b"FontBBox".to_vec(), Object::Array(vec![(-25).into(), (-254).into(), 1000.into(), 880.into()]));
        fd.set(b"ItalicAngle".to_vec(), Object::Int(0));
        fd.set(b"Ascent".to_vec(), Object::Int(880));
        fd.set(b"Descent".to_vec(), Object::Int(-120));
        fd.set(b"CapHeight".to_vec(), Object::Int(880));
        fd.set(b"StemV".to_vec(), Object::Int(93));
        let fd = self.doc.add(fd);
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::name("Font"));
        cid.set(b"Subtype".to_vec(), Object::name("CIDFontType0"));
        cid.set(b"BaseFont".to_vec(), Object::name("STSong-Light"));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(system_info("GB1", 5)));
        cid.set(b"FontDescriptor".to_vec(), Object::Ref(fd));
        cid.set(b"DW".to_vec(), Object::Int(1000));
        let cid = self.doc.add(cid);
        let mut t0 = Dict::new();
        t0.set(b"Type".to_vec(), Object::name("Font"));
        t0.set(b"Subtype".to_vec(), Object::name("Type0"));
        t0.set(b"BaseFont".to_vec(), Object::name("STSong-Light"));
        t0.set(b"Encoding".to_vec(), Object::name("UniGB-UTF16-H"));
        t0.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Ref(cid)]));
        let r = self.doc.add(t0);
        self.cjk = Some(r);
        r
    }

    fn standard(&mut self, base: &'static str) -> ObjRef {
        *self.standard.entry(base).or_insert_with(|| {
            let mut f = Dict::new();
            f.set(b"Type".to_vec(), Object::name("Font"));
            f.set(b"Subtype".to_vec(), Object::name("Type1"));
            f.set(b"BaseFont".to_vec(), Object::name(base));
            f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
            self.doc.add(f)
        })
    }

    fn text(&mut self, t: &TextObject, layer_param: Option<u32>, out: &mut PageOut) {
        let param = t.graphic.draw_param.or(layer_param);
        let fill = t.fill.then(|| t.fill_color.clone().or_else(|| self.param_color(param, true)).unwrap_or_else(black));
        let stroke = t.stroke.then(|| t.stroke_color.clone().or_else(|| self.param_color(param, false)).unwrap_or_else(black));
        let mode = match (fill.is_some(), stroke.is_some()) {
            (true, false) => 0,
            (false, true) => 1,
            (true, true) => 2,
            (false, false) => 3,
        };
        let size = t.size;
        let hs = if t.h_scale.is_finite() && t.h_scale > 0.0 { t.h_scale } else { 1.0 };
        let font = self.res.fonts.get(&t.font).cloned();
        let base: &'static str = match &font {
            Some(f) if f.fixed_width => "Courier",
            Some(f) if f.serif && (f.bold || t.weight >= 700) => "Times-Bold",
            Some(f) if f.serif => "Times-Roman",
            Some(f) if f.bold || t.weight >= 700 => "Helvetica-Bold",
            _ if t.weight >= 700 => "Helvetica-Bold",
            _ => "Helvetica",
        };
        // `CGTransform` glyphs, one per character, by character index across the object's codes.
        let mut glyph_of: HashMap<usize, u16> = HashMap::new();
        for cg in &t.cg_transforms {
            if cg.code_count == cg.glyph_count && cg.glyphs.len() == cg.glyph_count as usize {
                for (k, g) in cg.glyphs.iter().enumerate() {
                    if let Ok(g) = u16::try_from(*g) {
                        glyph_of.insert(cg.code_position as usize + k, g);
                    }
                }
            }
        }
        let mut s = String::from("q\n");
        self.clip(&t.graphic, &mut s);
        s.push_str(&cm(&t.graphic.to_page()));
        self.alpha(&t.graphic, fill.as_ref(), stroke.as_ref(), out, &mut s);
        for c in [fill.as_ref().and_then(|c| self.color(c, true)), stroke.as_ref().and_then(|c| self.color(c, false))].into_iter().flatten() {
            s.push_str(&c);
        }
        let _ = writeln!(s, "{} w\nBT\n{mode} Tr", n(t.graphic.line_width.unwrap_or(0.353)));
        let (mut x, mut y) = (0.0, 0.0);
        let mut index = 0usize;
        for code in &t.codes {
            let (x0, y0) = (code.x.unwrap_or(x), code.y.unwrap_or(y));
            let mut prev: Option<f64> = None; // the previous glyph's position and advance
            let mut pos = (x0, y0);
            for (k, (ch, dx, dy)) in code.glyph_offsets().into_iter().enumerate() {
                let px = match (dx, prev) {
                    (Some(d), _) => x0 + d,
                    (None, Some(next)) => next,
                    (None, None) => x0,
                };
                let py = dy.map_or(pos.1, |d| y0 + d);
                pos = (px, py);
                let advance = self.glyph(t.font, base, ch, glyph_of.get(&(index + k)).copied(), size, hs, (px, py), out, &mut s);
                prev = Some(px + advance);
            }
            index += code.text.chars().count();
            x = prev.unwrap_or(pos.0);
            y = pos.1;
        }
        s.push_str("ET\nQ\n");
        out.ops.push_str(&s);
    }

    /// Draw one character at `at` (object space, mm) and return its advance in mm.
    #[allow(clippy::too_many_arguments)]
    fn glyph(
        &mut self,
        font_id: u32,
        base: &'static str,
        ch: char,
        cg: Option<u16>,
        size: f64,
        hs: f64,
        at: (f64, f64),
        out: &mut PageOut,
        s: &mut String,
    ) -> f64 {
        let tm = |s: &mut String| {
            let _ = writeln!(s, "{} 0 0 {} {} {} Tm", n(size * hs), n(-size), n(at.0), n(at.1));
        };
        if let Some(font) = self.embedded(font_id) {
            let gid = cg.or_else(|| {
                let f = FontRef::new(&font.bytes).ok()?;
                f.charmap().map(ch).and_then(|g| u16::try_from(g.to_u32()).ok())
            });
            if let Some(gid) = gid.filter(|g| *g != 0) {
                let adv = FontRef::new(&font.bytes)
                    .ok()
                    .and_then(|f| f.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(GlyphId::new(u32::from(gid))))
                    .map_or(1000.0, |a| f64::from(a) * 1000.0 / font.units_per_em);
                font.used.entry(gid).or_insert((adv, ch.to_string()));
                let name = format!("F{font_id}");
                let r = font.type0;
                out.fonts.insert(name.clone(), r);
                let _ = writeln!(s, "/{name} 1 Tf");
                tm(s);
                let _ = writeln!(s, "{} Tj", hex(&gid.to_be_bytes()));
                return adv / 1000.0 * size * hs;
            }
        }
        if win_ansi_encodable(ch) {
            let r = self.standard(base);
            let name = base.replace('-', "");
            out.fonts.insert(name.clone(), r);
            let _ = writeln!(s, "/{name} 1 Tf");
            tm(s);
            let text = ch.to_string();
            // Hex, not a literal: WinAnsi bytes above 0x7F must reach the content stream unchanged.
            let _ = writeln!(s, "{} Tj", hex(&win_ansi(&text)));
            // Courier is fixed width; Helvetica's widths stand in for the other standard faces.
            return if base == "Courier" { 0.6 * size } else { helvetica_width(&text, size) } * hs;
        }
        let r = self.cjk();
        out.fonts.insert("FCJK".into(), r);
        let _ = writeln!(s, "/FCJK 1 Tf");
        tm(s);
        let mut buf = [0u16; 2];
        let utf16: Vec<u8> = ch.encode_utf16(&mut buf).iter().flat_map(|u| u.to_be_bytes()).collect();
        let _ = writeln!(s, "{} Tj", hex(&utf16));
        size * hs
    }
}

fn system_info(ordering: &str, supplement: i64) -> Dict {
    let mut d = Dict::new();
    d.set(b"Registry".to_vec(), pdfcraft_cos::PdfString::text("Adobe"));
    d.set(b"Ordering".to_vec(), pdfcraft_cos::PdfString::text(ordering));
    d.set(b"Supplement".to_vec(), Object::Int(supplement));
    d
}

fn black() -> Color {
    Color { value: vec![0.0, 0.0, 0.0], color_space: None, index: None, alpha: 255, shading: false }
}

/// Path operators for `path`, its points mapped by `m`. Quadratic curves and arcs become cubic
/// Béziers first, so an affine `m` maps them exactly.
fn path_ops(path: &[PathCmd], m: &Matrix, s: &mut String) {
    let pt = |s: &mut String, x: f64, y: f64| {
        let (x, y) = m.apply(x, y);
        let _ = write!(s, "{} {} ", n(x), n(y));
    };
    let mut cur: Option<(f64, f64)> = None;
    let mut start = (0.0, 0.0);
    for cmd in path {
        match *cmd {
            PathCmd::Move(x, y) => {
                pt(s, x, y);
                s.push_str("m\n");
                cur = Some((x, y));
                start = (x, y);
            }
            PathCmd::Close => {
                if cur.is_some() {
                    s.push_str("h\n");
                    cur = Some(start);
                }
            }
            _ => {
                let (x0, y0) = match cur {
                    Some(p) => p,
                    None => {
                        // A drawing command without a current point starts one there.
                        let p = end_point(cmd);
                        pt(s, p.0, p.1);
                        s.push_str("m\n");
                        start = p;
                        cur = Some(p);
                        continue;
                    }
                };
                match *cmd {
                    PathCmd::Line(x, y) => {
                        pt(s, x, y);
                        s.push_str("l\n");
                    }
                    PathCmd::Quad(qx, qy, x, y) => {
                        pt(s, x0 + 2.0 / 3.0 * (qx - x0), y0 + 2.0 / 3.0 * (qy - y0));
                        pt(s, x + 2.0 / 3.0 * (qx - x), y + 2.0 / 3.0 * (qy - y));
                        pt(s, x, y);
                        s.push_str("c\n");
                    }
                    PathCmd::Cubic(a, b, c, d, x, y) => {
                        pt(s, a, b);
                        pt(s, c, d);
                        pt(s, x, y);
                        s.push_str("c\n");
                    }
                    PathCmd::Arc { rx, ry, angle, large, sweep, x, y } => {
                        for [a, b, c, d, e, f] in arc_to_cubics((x0, y0), rx, ry, angle, large, sweep, (x, y)) {
                            pt(s, a, b);
                            pt(s, c, d);
                            pt(s, e, f);
                            s.push_str("c\n");
                        }
                    }
                    PathCmd::Move(..) | PathCmd::Close => {}
                }
                cur = Some(end_point(cmd));
            }
        }
    }
}

fn end_point(cmd: &PathCmd) -> (f64, f64) {
    match *cmd {
        PathCmd::Move(x, y) | PathCmd::Line(x, y) | PathCmd::Quad(_, _, x, y) | PathCmd::Cubic(_, _, _, _, x, y) | PathCmd::Arc { x, y, .. } => {
            (x, y)
        }
        PathCmd::Close => (0.0, 0.0),
    }
}

/// An SVG-style elliptical arc as cubic Béziers (endpoint to centre parameterisation, at most
/// four quarter-turn segments). A zero radius is a straight line.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(p0: (f64, f64), rx: f64, ry: f64, angle_deg: f64, large: bool, sweep: bool, p1: (f64, f64)) -> Vec<[f64; 6]> {
    let line = vec![[p0.0, p0.1, p1.0, p1.1, p1.0, p1.1]];
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-9 || ry < 1e-9 || (p0.0 - p1.0).abs() + (p0.1 - p1.1).abs() < 1e-12 {
        return line;
    }
    let phi = angle_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let (dx, dy) = ((p0.0 - p1.0) / 2.0, (p0.1 - p1.1) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
    if large == sweep {
        coef = -coef;
    }
    let (cxp, cyp) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let cx = cos * cxp - sin * cyp + (p0.0 + p1.0) / 2.0;
    let cy = sin * cxp + cos * cyp + (p0.1 + p1.1) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let theta = angle(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut delta = angle((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    if !delta.is_finite() || !cx.is_finite() || !cy.is_finite() {
        return line;
    }
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().clamp(1.0, 4.0) as usize;
    let step = delta / segments as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| (cx + rx * t.cos() * cos - ry * t.sin() * sin, cy + rx * t.cos() * sin + ry * t.sin() * cos);
    let deriv = |t: f64| (-rx * t.sin() * cos - ry * t.cos() * sin, -rx * t.sin() * sin + ry * t.cos() * cos);
    let mut out = Vec::with_capacity(segments);
    let mut t = theta;
    for i in 0..segments {
        let t2 = t + step;
        let (a, da) = (point(t), deriv(t));
        let (mut b, db) = (point(t2), deriv(t2));
        if i + 1 == segments {
            b = p1; // end exactly where the path says
        }
        out.push([a.0 + k * da.0, a.1 + k * da.1, b.0 - k * db.0, b.1 - k * db.1, b.0, b.1]);
        t = t2;
    }
    out
}

#[cfg(test)]
#[path = "ofd_tests.rs"]
mod tests;
