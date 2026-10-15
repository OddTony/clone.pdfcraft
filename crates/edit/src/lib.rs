//! pdfcraft-edit — page content editing (L4). Today: page marks, Acrobat's Header & footer,
//! Watermark, Background and Bates numbering (execution plan M7.6).
//!
//! **How marks are stored.** Each mark is one content stream appended to (or, for content behind
//! the page, prepended to; a blended background goes right after the page's own content) the
//! page's `/Contents`. A normal background on a page that starts with a recognised white page
//! fill goes between that fill and the rest of the page: the stream holding the fill is split
//! there into two new streams with the same bytes (see `background.rs`). Marks are drawn in
//! display space (after `/Rotate`) and
//! wrapped in a marked-content artifact (§14.8.2.2) so readers and screen readers skip it:
//! `/Artifact <</Type /Pagination /Subtype /Header /PCMark /HeaderFooter>> BDC … EMC`. The
//! `/PCMark` entry (also in the stream dictionary) names the kind of mark, which is how Update and
//! Remove find them again without decompressing page content. When
//! anything is drawn on top, the page's original content is wrapped in `q … Q` (two tiny streams)
//! so its graphics state can't leak into the mark.
//!
//! Text uses standard Helvetica (WinAnsi) added to the page resources as `/PCHelv`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use pdfcraft_cos::{Dict, Document, Object, Stream};
use pdfcraft_fonts::{helvetica_width, literal, win_ansi};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    /// Holds the 0-based index; the message names the 1-based page a user counts (#1550).
    #[error("page {} does not exist", .0.saturating_add(1))]
    NoSuchPage(usize),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Cos(#[from] pdfcraft_cos::CosError),
}

/// The kinds of page marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    HeaderFooter,
    Watermark,
    Background,
}

impl MarkKind {
    fn tag(self) -> &'static str {
        match self {
            MarkKind::HeaderFooter => "HeaderFooter",
            MarkKind::Watermark => "Watermark",
            MarkKind::Background => "Background",
        }
    }
}

pub type Rgb = [f64; 3];

/// Header and footer settings (Acrobat's Add Header and Footer dialog).
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderFooter {
    /// Left, centre and right header text, then left, centre and right footer text. Tokens:
    /// `<<1>>` page number, `<<n>>` page count, `<<1 of n>>`, `<<1/n>>`, `<<Page 1>>`,
    /// `<<Page 1 of n>>`, dates `<<m/d/yyyy>>`, `<<mm/dd/yyyy>>`, `<<d/m/yyyy>>`, `<<yyyy-mm-dd>>`,
    /// `<<m/d/yy>>`, `<<mmmm d, yyyy>>`, and Bates numbers `<<Bates Number#6#1#PRE#SUF>>`
    /// (digits, start, prefix, suffix).
    pub text: [String; 6],
    pub font_size: f64,
    pub color: Rgb,
    pub underline: bool,
    /// Margins in points: top, bottom, left, right (the text's distance from the page edges).
    pub margins: [f64; 4],
    /// The number `<<1>>` shows on the first page of the range.
    pub start_number: u32,
}

impl Default for HeaderFooter {
    /// Acrobat's defaults: 8 pt black, margins 0.5 in top and bottom, 1 in left and right.
    fn default() -> Self {
        Self { text: Default::default(), font_size: 8.0, color: [0.0; 3], underline: false, margins: [36.0, 36.0, 72.0, 72.0], start_number: 1 }
    }
}

impl HeaderFooter {
    /// The first character [`add_header_footer`] would refuse, because the standard fonts can't
    /// draw it (#125). Tokens only add ASCII (Bates prefixes and suffixes are part of the
    /// template), so checking the templates, split as `expand`'s output is, covers every page
    /// (and a `\r` that would be drawn is refused too). The dialog asks this too, so its preview
    /// never offers text that applying refuses (#2013).
    pub fn first_undrawable(&self) -> Option<char> {
        self.text.iter().flat_map(|t| t.lines()).find_map(pdfcraft_fonts::first_non_win_ansi)
    }
}

/// A picture for a background or watermark (Acrobat: Source ▸ File): an XObject already in
/// the document, an image (drawn into a unit square) or a form (a page of a PDF).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkSource {
    pub xobject: pdfcraft_cos::ObjRef,
    /// Its natural size in points.
    pub size: (f64, f64),
    /// An image XObject (unit square); otherwise a form whose `/Matrix` maps it to `size`.
    pub image: bool,
}

/// A watermark: text, or a picture from a file (`source`; the text is then ignored).
#[derive(Clone, Debug, PartialEq)]
pub struct Watermark {
    pub text: String,
    pub source: Option<MarkSource>,
    /// Pictures: size relative to the page (1.0 fits the page).
    pub scale: f64,
    /// 0 = fit: about half the page diagonal.
    pub font_size: f64,
    pub color: Rgb,
    /// 0–1.
    pub opacity: f64,
    /// Degrees counter-clockwise (Acrobat's 45° runs from bottom-left to top-right).
    pub rotation: f64,
    /// Draw behind the page content instead of on top.
    pub behind: bool,
    /// Offset of the centre from the page centre, in points (right, up).
    pub offset: [f64; 2],
}

impl Watermark {
    /// The first character of a text watermark [`add_watermark`] would refuse, because the
    /// standard fonts can't draw it (#125). A picture watermark draws no text.
    pub fn first_undrawable(&self) -> Option<char> {
        match self.source {
            Some(_) => None,
            None => self.text.trim().lines().find_map(pdfcraft_fonts::first_non_win_ansi),
        }
    }
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: String::new(),
            source: None,
            scale: 0.5,
            font_size: 0.0,
            color: [0.6, 0.6, 0.6],
            opacity: 0.5,
            rotation: 45.0,
            behind: false,
            offset: [0.0; 2],
        }
    }
}

/// A background below the page foreground: a solid colour, or a picture from a file.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    pub color: Rgb,
    pub opacity: f64,
    pub source: Option<MarkSource>,
    /// Pictures: size relative to the page (1.0 fits the page).
    pub scale: f64,
    /// Blend the background with the page content (the Multiply blend mode, ISO 32000-2
    /// §11.3.5) instead of placing it below the foreground. This can change foreground colours.
    /// The default normal placement already handles recognised leading white page fills;
    /// blending remains an explicit option for other pages.
    pub multiply: bool,
}

impl Default for Background {
    fn default() -> Self {
        Self { color: [1.0, 1.0, 0.85], opacity: 1.0, source: None, scale: 1.0, multiply: false }
    }
}

/// Drawing a picture mark: fitted to the page at `scale`, centred (plus `offset`), turned by
/// `rotation` degrees. Returns the content and the resource name it uses.
fn picture(src: &MarkSource, page: (f64, f64), scale: f64, rotation: f64, offset: [f64; 2]) -> String {
    let (w, h) = page;
    let (sw, sh) = (src.size.0.max(0.01), src.size.1.max(0.01));
    let k = (w / sw).min(h / sh) * if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let (dw, dh) = (sw * k, sh * k);
    let (s, c) = rotation.to_radians().sin_cos();
    let (cx, cy) = (w / 2.0 + offset[0], h / 2.0 + offset[1]);
    // Centre, rotate, then place the picture's box (unit square for images).
    let place = if src.image {
        format!("{} 0 0 {} {} {} cm", n(dw), n(dh), n(-dw / 2.0), n(-dh / 2.0))
    } else {
        format!("{} 0 0 {} {} {} cm", n(k), n(k), n(-dw / 2.0), n(-dh / 2.0))
    };
    format!("{} {} {} {} {} {} cm\n{place}\n/PCPic{} Do\n", n(c), n(s), n(-s), n(c), n(cx), n(cy), src.xobject.num)
}

/// Register a mark picture in the page's own resources (`/PCPic<num>`).
fn add_picture_resource(doc: &mut Document, page: &pdfcraft_model::Page, src: &MarkSource) -> Result<(), EditError> {
    let p = doc.get(page.obj).as_dict().cloned().unwrap_or_default();
    let mut res = p.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let mut xo = res.get(b"XObject").map(|x| doc.resolve(x)).and_then(|x| x.as_dict().cloned()).unwrap_or_default();
    xo.set(format!("PCPic{}", src.xobject.num).into_bytes(), Object::Ref(src.xobject));
    res.set(b"XObject".to_vec(), Object::Dict(xo));
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    Ok(())
}

/// A PDF page picture as one isolated transparency group (ISO 32000-2 §11.4.7, §11.6.6): a
/// form whose content draws `src`. A blend mode set where it is drawn then applies to the
/// picture as a whole, composited once, and not to each of its elements, whose own `gs` (a
/// `/BM /Normal`, say) would otherwise replace it. Images are one element already.
fn as_group(doc: &mut Document, src: MarkSource) -> MarkSource {
    if src.image {
        return src;
    }
    let mut group = Dict::new();
    group.set(b"Type".to_vec(), Object::name("Group"));
    group.set(b"S".to_vec(), Object::name("Transparency"));
    group.set(b"I".to_vec(), Object::Bool(true));
    let mut xo = Dict::new();
    xo.set(b"PCSrc".to_vec(), Object::Ref(src.xobject));
    let mut res = Dict::new();
    res.set(b"XObject".to_vec(), Object::Dict(xo));
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), Object::name("XObject"));
    d.set(b"Subtype".to_vec(), Object::name("Form"));
    // The source form's /Matrix maps it to (0, 0)–size, so that is all the group covers.
    d.set(b"BBox".to_vec(), Object::Array(vec![Object::Real(0.0), Object::Real(0.0), Object::Real(src.size.0), Object::Real(src.size.1)]));
    d.set(b"Group".to_vec(), Object::Dict(group));
    d.set(b"Resources".to_vec(), Object::Dict(res));
    let xobject = doc.add(Object::Stream(Stream::flate(d, b"/PCSrc Do\n")));
    MarkSource { xobject, ..src }
}

/// Values tokens need that come from outside the document.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Context {
    /// Today's date: (year, month 1–12, day 1–31).
    pub date: (i64, u32, u32),
}

// Model-derived transforms can contain tiny scale factors; retain their PDF decimal precision.
fn matrix_number(value: f64) -> String {
    let mut bytes = Vec::new();
    pdfcraft_cos::serialize(&Object::Real(value), &mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn n(v: f64) -> String {
    let s = format!("{:.3}", if v.abs() < 5e-4 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn rgb(c: Rgb) -> String {
    let c = c.map(|x| x.clamp(0.0, 1.0));
    format!("{} {} {} rg", n(c[0]), n(c[1]), n(c[2]))
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// Expand `<<…>>` tokens for page `index` (0-based within the range) of `count`.
pub fn expand(template: &str, page_number: u32, count: usize, bates: u64, cx: &Context) -> String {
    let (y, m, d) = cx.date;
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("<<") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find(">>") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        let p = page_number;
        let value = match token {
            "1" => p.to_string(),
            "n" => count.to_string(),
            "1 of n" => format!("{p} of {count}"),
            "1/n" => format!("{p}/{count}"),
            "Page 1" => format!("Page {p}"),
            "Page 1 of n" => format!("Page {p} of {count}"),
            "m/d" => format!("{m}/{d}"),
            "m/d/yy" => format!("{m}/{d}/{:02}", y.rem_euclid(100)),
            "m/d/yyyy" => format!("{m}/{d}/{y}"),
            "mm/dd/yy" => format!("{m:02}/{d:02}/{:02}", y.rem_euclid(100)),
            "mm/dd/yyyy" => format!("{m:02}/{d:02}/{y}"),
            "d/m/yy" => format!("{d}/{m}/{:02}", y.rem_euclid(100)),
            "d/m/yyyy" => format!("{d}/{m}/{y}"),
            "dd/mm/yyyy" => format!("{d:02}/{m:02}/{y}"),
            "yyyy-mm-dd" => format!("{y}-{m:02}-{d:02}"),
            "mmmm d, yyyy" => format!("{} {d}, {y}", MONTHS[(m.clamp(1, 12) - 1) as usize]),
            t if t.starts_with("Bates Number") => {
                // Bates Number#digits#start#prefix#suffix (start is applied by the caller).
                let parts: Vec<&str> = t.split('#').collect();
                let digits = parts.get(1).and_then(|d| d.parse::<usize>().ok()).unwrap_or(6).clamp(1, 15);
                format!("{}{:0digits$}{}", parts.get(3).unwrap_or(&""), bates, parts.get(4).unwrap_or(&""))
            }
            _ => format!("<<{token}>>"),
        };
        out.push_str(&value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The Bates start number in a template (`<<Bates Number#digits#start#…>>`), if any.
fn bates_start(template: &str) -> Option<u64> {
    let i = template.find("<<Bates Number")?;
    let t = &template[i + 2..];
    let t = &t[..t.find(">>")?];
    t.split('#').nth(2).and_then(|s| s.parse().ok()).or(Some(1))
}

fn page_list(doc: &Document) -> Vec<pdfcraft_model::Page> {
    pdfcraft_model::pages(doc)
}

fn check(pages: &[usize], count: usize) -> Result<(), EditError> {
    match pages.iter().find(|p| **p >= count) {
        Some(p) => Err(EditError::NoSuchPage(*p)),
        None => Ok(()),
    }
}

/// The page's `/Contents` as a list of stream references (inline content is promoted).
fn contents(doc: &mut Document, page: &pdfcraft_model::Page) -> Result<Vec<Object>, EditError> {
    let obj = doc.get(page.obj);
    let d = obj.as_dict().cloned().unwrap_or_default();
    Ok(match d.get(b"Contents").cloned() {
        None => Vec::new(),
        Some(Object::Ref(r)) => match doc.get(r).as_array() {
            Some(a) => a.clone(),
            None => vec![Object::Ref(r)],
        },
        Some(Object::Array(a)) => a,
        Some(other) => vec![Object::Ref(doc.add(other))],
    })
}

/// The extended graphics state a mark is drawn with: its opacity, and whether it multiplies
/// with what is already on the page. Each combination has its own resource name.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gs {
    opacity: f64,
    multiply: bool,
}

impl Gs {
    fn normal(opacity: f64) -> Self {
        Gs { opacity, multiply: false }
    }

    /// The resource name (`PCGS50`, `PCGSM100`).
    fn name(self) -> String {
        format!("PCGS{}{}", if self.multiply { "M" } else { "" }, (self.opacity * 100.0).round() as i64)
    }

    /// The operator selecting this state.
    fn op(self) -> String {
        format!("/{} gs\n", self.name())
    }
}

/// Add the font (and a graphics state) to the page's own resources.
fn add_resources(doc: &mut Document, page: &pdfcraft_model::Page, gs: Option<Gs>, content: Option<&[u8]>) -> Result<(), EditError> {
    let mut res = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let mut fonts = res.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()).unwrap_or_default();
    for (name, base) in [(&b"PCHelv"[..], "Helvetica"), (b"PCTimes", "Times-Roman"), (b"PCCour", "Courier")] {
        // Helvetica always (marks use it); the others only when the content names them.
        if name != b"PCHelv" && !content.is_some_and(|c| c.windows(name.len() + 1).any(|w| w[0] == b'/' && &w[1..] == name)) {
            continue;
        }
        let mut font = Dict::new();
        font.set(b"Type".to_vec(), Object::name("Font"));
        font.set(b"Subtype".to_vec(), Object::name("Type1"));
        font.set(b"BaseFont".to_vec(), Object::name(base));
        font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        fonts.set(name.to_vec(), Object::Dict(font));
    }
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    if let Some(state) = gs {
        let mut all = res.get(b"ExtGState").map(|g| doc.resolve(g)).and_then(|g| g.as_dict().cloned()).unwrap_or_default();
        let mut s = Dict::new();
        s.set(b"Type".to_vec(), Object::name("ExtGState"));
        s.set(b"CA".to_vec(), Object::Real(state.opacity));
        s.set(b"ca".to_vec(), Object::Real(state.opacity));
        if state.multiply {
            s.set(b"BM".to_vec(), Object::name("Multiply"));
        }
        all.set(state.name().into_bytes(), Object::Dict(s));
        res.set(b"ExtGState".to_vec(), Object::Dict(all));
    }
    // The page gets its own (possibly copied) resources; shared dictionaries stay untouched.
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(res)))?;
    Ok(())
}

const WRAP_OPEN: &[u8] = b"q %PdfCraft\n";
const WRAP_CLOSE: &[u8] = b"Q %PdfCraft\n";

#[cfg(test)]
fn stream_bytes(doc: &Document, o: &Object) -> Option<Vec<u8>> {
    match &*doc.resolve(o) {
        Object::Stream(s) => s.decoded().ok(),
        _ => None,
    }
}

/// The `/PCMark` tag in a content stream's dictionary: a mark kind, or `Wrap` for the q/Q
/// wrapper. Read from the dictionary, so finding marks never decompresses page content.
fn stream_tag(doc: &Document, o: &Object) -> Option<String> {
    let obj = doc.resolve(o);
    obj.as_dict()?.name(b"PCMark").map(|n| String::from_utf8_lossy(n).into_owned())
}

fn tagged(tag: &str) -> Dict {
    let mut d = Dict::new();
    d.set(b"PCMark".to_vec(), Object::name(tag));
    d
}

/// Where content goes in a page's `/Contents`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    /// Before everything (prepended).
    Behind,
    /// Right after the page's own (wrapped) content, under whatever was put on top of it
    /// before: a blended background stays under headers, watermarks and added content. On a
    /// page with no content of its own it goes after the backgrounds that lead the list.
    OverContent,
    /// After everything (appended).
    OnTop,
}

/// Put a mark's content on a page at `layer`. Anything above the page's own content first wraps
/// that content in q/Q.
fn place(doc: &mut Document, page: &pdfcraft_model::Page, kind: MarkKind, content: Vec<u8>, layer: Layer) -> Result<(), EditError> {
    place_tagged(doc, page, kind.tag(), content, layer)
}

/// Put content on a page, tagged `tag` (see [`place`]).
fn place_tagged(doc: &mut Document, page: &pdfcraft_model::Page, tag: &str, content: Vec<u8>, layer: Layer) -> Result<(), EditError> {
    let mut list = contents(doc, page)?;
    let mark = Object::Ref(doc.add(Object::Stream(Stream::flate(tagged(tag), &content))));
    if layer == Layer::Behind {
        list.insert(0, mark);
    } else {
        let wrapped = list.iter().any(|o| stream_tag(doc, o).as_deref() == Some("Wrap"));
        let original = list.iter().any(|o| stream_tag(doc, o).is_none());
        if !wrapped && original {
            let open = Object::Ref(doc.add(Object::Stream(Stream::from_raw(tagged("Wrap"), WRAP_OPEN.to_vec()))));
            let close = Object::Ref(doc.add(Object::Stream(Stream::from_raw(tagged("Wrap"), WRAP_CLOSE.to_vec()))));
            // Behind-marks stay outside the wrapper.
            let split = list.iter().position(|o| stream_tag(doc, o).is_none()).unwrap_or(list.len());
            list.insert(split, open);
            list.push(close);
        }
        // Over the content: right after the wrapper's closing piece. Without a wrapper the page
        // has no content of its own to sit over: it goes over the backgrounds that lead the
        // list and under everything else added.
        let close = list.iter().rposition(|o| stream_tag(doc, o).as_deref() == Some("Wrap"));
        match (layer, close) {
            (Layer::OverContent, Some(c)) => list.insert(c.saturating_add(1), mark),
            (Layer::OverContent, None) => {
                let backgrounds = list.iter().take_while(|o| stream_tag(doc, o).as_deref() == Some(MarkKind::Background.tag())).count();
                list.insert(backgrounds, mark)
            }
            _ => list.push(mark),
        }
    }
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(list)))?;
    Ok(())
}

/// Draw `content` on top of a page (0-based) as permanent page content, with standard Helvetica
/// available as `/PCHelv`. The stream is tagged `tag` (not a mark kind, so Remove never takes
/// it away); the original content is wrapped in q/Q first. Used for applied redaction boxes.
pub fn stamp(doc: &mut Document, page: usize, tag: &str, content: Vec<u8>) -> Result<(), EditError> {
    let all = page_list(doc);
    check(&[page], all.len())?;
    add_resources(doc, &all[page], None, Some(&content))?;
    let p = page_list(doc).swap_remove(page);
    place_tagged(doc, &p, tag, content, Layer::OnTop)
}

fn begin(kind: MarkKind, subtype: &str, matrix: [f64; 6]) -> String {
    format!(
        "q\n/Artifact <</Type /Pagination /Subtype /{subtype} /PCMark /{}>> BDC\n{} {} {} {} {} {} cm\n",
        kind.tag(),
        matrix_number(matrix[0]),
        matrix_number(matrix[1]),
        matrix_number(matrix[2]),
        matrix_number(matrix[3]),
        matrix_number(matrix[4]),
        matrix_number(matrix[5])
    )
}

const END: &str = "EMC\nQ\n";

/// The refusal for page text with `c`, which nothing here can draw.
pub(crate) fn undrawable(c: char) -> EditError {
    EditError::Invalid(undrawable_message(c))
}

/// The wording of the refusal for text with `c`, which the standard fonts can't draw.
pub fn undrawable_message(c: char) -> String {
    format!("the standard fonts can't draw \"{c}\" (U+{:04X}); only Western European characters can be added as text for now", u32::from(c))
}

fn text_op(x: f64, y: f64, text: &str) -> Vec<u8> {
    let mut v = format!("1 0 0 1 {} {} Tm ", n(x), n(y)).into_bytes();
    v.extend(literal(&win_ansi(text)));
    v.extend_from_slice(b" Tj\n");
    v
}

/// Add a header and footer to `pages` (0-based). With `replace`, existing headers and footers on
/// those pages are removed first (Acrobat's Replace Existing).
pub fn add_header_footer(doc: &mut Document, pages: &[usize], hf: &HeaderFooter, replace: bool, cx: &Context) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    if hf.text.iter().all(|t| t.trim().is_empty()) {
        return Err(EditError::Invalid("type the header or footer text first".into()));
    }
    if let Some(c) = hf.first_undrawable() {
        return Err(undrawable(c));
    }
    if !(hf.font_size.is_finite() && hf.font_size > 0.0 && hf.font_size <= 200.0 && hf.margins.iter().all(|m| m.is_finite() && *m >= 0.0)) {
        return Err(EditError::Invalid("invalid font size or margins".into()));
    }
    // Page numbers count up from start_number, so the last selected page needs the most room.
    // Refuse the whole run before drawing anything: headers applied to some pages and then a
    // failed page number would leave the document half changed.
    let last_number = u64::from(hf.start_number)
        .checked_add(pages.len().saturating_sub(1) as u64)
        .ok_or_else(|| EditError::Invalid("the header page numbers are too large".into()))?;
    if last_number > u64::from(u32::MAX) {
        return Err(EditError::Invalid(format!(
            "the header start number {} leaves no room for {} pages (page numbers stop at {})",
            hf.start_number,
            pages.len(),
            u32::MAX
        )));
    }
    // The same goes for Bates numbers: the last selected page must still fit in a u64.
    let bates0 = hf.text.iter().find_map(|t| bates_start(t)).unwrap_or(1);
    if bates0.checked_add(pages.len().saturating_sub(1) as u64).is_none() {
        return Err(EditError::Invalid(format!(
            "the Bates start number {bates0} leaves no room for {} pages (Bates numbers stop at {})",
            pages.len(),
            u64::MAX
        )));
    }
    if replace {
        remove_marks(doc, pages, MarkKind::HeaderFooter)?;
    }
    let all = page_list(doc);
    let count = all.len();
    for (k, &i) in pages.iter().enumerate() {
        let page = &all[i];
        let (w, h) = page.display_size(doc);
        let number =
            u32::try_from(u64::from(hf.start_number) + k as u64).map_err(|_| EditError::Invalid("the header page numbers are too large".into()))?;
        let bates = bates0.checked_add(k as u64).ok_or_else(|| EditError::Invalid("the Bates numbers are too large".into()))?;
        let size = hf.font_size;
        let [top, bottom, left, right] = hf.margins;
        let mut body: Vec<u8> = format!("BT\n/PCHelv {} Tf\n{}\n", n(size), rgb(hf.color)).into_bytes();
        let mut underlines = String::new();
        for (slot, template) in hf.text.iter().enumerate() {
            if template.trim().is_empty() {
                continue;
            }
            let lines: Vec<String> = expand(template, number, count, bates, cx).lines().map(str::to_owned).collect();
            let header = slot < 3;
            let line_h = size * 1.2;
            for (li, line) in lines.iter().enumerate() {
                let tw = helvetica_width(line, size);
                let x = match slot % 3 {
                    0 => left,
                    1 => (w - tw) / 2.0,
                    _ => w - right - tw,
                };
                // Headers hang below the top margin; footers sit on the bottom margin.
                let y = if header { h - top - size * 0.8 - li as f64 * line_h } else { bottom + (lines.len() - 1 - li) as f64 * line_h };
                body.extend(text_op(x, y, line));
                if hf.underline {
                    underlines.push_str(&format!("{} {} {} {} re f\n", n(x), n(y - size * 0.15), n(tw), n((size * 0.06).max(0.4))));
                }
            }
        }
        body.extend_from_slice(b"ET\n");
        if !underlines.is_empty() {
            body.extend(format!("{}\n{underlines}", rgb(hf.color)).bytes());
        }
        let mut content = begin(MarkKind::HeaderFooter, "Header", page.view_matrix(doc)).into_bytes();
        content.extend(body);
        content.extend_from_slice(END.as_bytes());
        add_resources(doc, page, None, None)?;
        let page = &page_list(doc)[i];
        place(doc, page, MarkKind::HeaderFooter, content, Layer::OnTop)?;
    }
    Ok(())
}

/// Add a text watermark to `pages`.
pub fn add_watermark(doc: &mut Document, pages: &[usize], wm: &Watermark, replace: bool) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    let text = wm.text.trim();
    if text.is_empty() && wm.source.is_none() {
        return Err(EditError::Invalid("type the watermark text first".into()));
    }
    if let Some(c) = wm.first_undrawable() {
        return Err(undrawable(c));
    }
    if !(wm.opacity.is_finite()
        && wm.rotation.is_finite()
        && wm.font_size.is_finite()
        && wm.font_size >= 0.0
        && wm.offset.iter().all(|o| o.is_finite()))
    {
        return Err(EditError::Invalid("invalid watermark settings".into()));
    }
    if replace {
        remove_marks(doc, pages, MarkKind::Watermark)?;
    }
    let gs = Gs::normal(wm.opacity.clamp(0.0, 1.0));
    let layer = if wm.behind { Layer::Behind } else { Layer::OnTop };
    let lines: Vec<&str> = text.lines().collect();
    for &i in pages {
        let page = page_list(doc)[i].clone();
        let (w, h) = page.display_size(doc);
        if let Some(src) = &wm.source {
            let mut content = begin(MarkKind::Watermark, "Watermark", page.view_matrix(doc));
            content.push_str(&gs.op());
            content.push_str(&picture(src, (w, h), wm.scale, wm.rotation, wm.offset));
            content.push_str(END);
            add_resources(doc, &page, Some(gs), None)?;
            add_picture_resource(doc, &page_list(doc)[i].clone(), src)?;
            let page = &page_list(doc)[i];
            place(doc, page, MarkKind::Watermark, content.into_bytes(), layer)?;
            continue;
        }
        let widest = lines.iter().map(|l| helvetica_width(l, 1.0)).fold(0.0, f64::max).max(0.01);
        let size = if wm.font_size > 0.0 { wm.font_size } else { ((w * w + h * h).sqrt() * 0.5 / widest).clamp(6.0, 300.0) };
        let (s, c) = wm.rotation.to_radians().sin_cos();
        let (cx, cy) = (w / 2.0 + wm.offset[0], h / 2.0 + wm.offset[1]);
        let mut content = begin(MarkKind::Watermark, "Watermark", page.view_matrix(doc)).into_bytes();
        content.extend(
            format!("{}{} {} {} {} {} {} cm\nBT\n/PCHelv {} Tf\n{}\n", gs.op(), n(c), n(s), n(-s), n(c), n(cx), n(cy), n(size), rgb(wm.color))
                .bytes(),
        );
        let line_h = size * 1.15;
        let total = line_h * (lines.len() as f64 - 1.0);
        for (li, line) in lines.iter().enumerate() {
            let tw = helvetica_width(line, size);
            content.extend(text_op(-tw / 2.0, total / 2.0 - li as f64 * line_h - size * 0.35, line));
        }
        content.extend_from_slice(b"ET\n");
        content.extend_from_slice(END.as_bytes());
        add_resources(doc, &page, Some(gs), None)?;
        let page = &page_list(doc)[i];
        place(doc, page, MarkKind::Watermark, content, layer)?;
    }
    Ok(())
}

/// Add a background to `pages`: below the foreground, above a recognised leading white fill
/// (otherwise behind the page), or explicitly blended over it with [`Background::multiply`].
pub fn add_background(doc: &mut Document, pages: &[usize], bg: &Background, replace: bool) -> Result<(), EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    if replace {
        remove_marks(doc, pages, MarkKind::Background)?;
    }
    let gs = Gs { opacity: if bg.opacity.is_finite() { bg.opacity.clamp(0.0, 1.0) } else { 1.0 }, multiply: bg.multiply };
    let layer = if bg.multiply { Layer::OverContent } else { Layer::Behind };
    // Blended, a PDF page is drawn as one group, so its own graphics states can't undo the blend.
    let source = match bg.source {
        Some(src) if bg.multiply => Some(as_group(doc, src)),
        other => other,
    };
    for &i in pages {
        let page = page_list(doc)[i].clone();
        let (w, h) = page.display_size(doc);
        let mut content = begin(MarkKind::Background, "Background", page.view_matrix(doc));
        content.push_str(&gs.op());
        match &source {
            Some(src) => content.push_str(&picture(src, (w, h), bg.scale, 0.0, [0.0; 2])),
            None => content.push_str(&format!("{}\n0 0 {} {} re f\n", rgb(bg.color), n(w), n(h))),
        }
        content.push_str(END);
        add_resources(doc, &page, Some(gs), None)?;
        if let Some(src) = &source {
            add_picture_resource(doc, &page_list(doc)[i].clone(), src)?;
        }
        let page = &page_list(doc)[i];
        if bg.multiply || !background::place(doc, page, content.as_bytes())? {
            place(doc, page, MarkKind::Background, content.into_bytes(), layer)?;
        }
    }
    Ok(())
}

/// How much of a page's decoded content [`page_hides_background`] reads (decoding stops there):
/// the fill it looks for comes first, so a page that has not painted within this much doesn't
/// have one.
const COVER_SCAN_BYTES: usize = 1024 * 1024;

mod background;

/// Whether a normal background can be inserted above a balanced leading white fill.
pub fn page_has_white_background(doc: &Document, page: usize) -> bool {
    background::supported(doc, page)
}

/// Does page `page` (0-based) start by painting an opaque fill over its whole visible area, so
/// that a background behind its content stays hidden? Files from accounting and office software
/// often do: an opaque white rectangle the size of the page is the first thing drawn. Normal
/// placement handles the subset recognised by [`page_has_white_background`].
///
/// Conservative: only a fill of a path that is one rectangle, unclipped and with no `gs` before it
/// (which could make it translucent or blend it), counts; anything else painted first answers
/// `false` (several rectangles can cancel out under either fill rule), and so does optional
/// content (`/OC … BDC`), which may be hidden. The
/// streams PdfCraft added (marks, added content) are skipped, a malformed page answers `false`, and at most
/// [`COVER_SCAN_BYTES`] of content are decoded: a stream that would decode to more ends the
/// reading there.
pub fn page_hides_background(doc: &Document, page: usize) -> bool {
    use pdfcraft_content::{Matrix, Pieces, contains};
    let Some(page) = page_list(doc).into_iter().nth(page) else { return false };
    let list = match page.dict.get(b"Contents").map(|c| doc.resolve(c)).as_deref() {
        Some(Object::Array(a)) => a.clone(),
        Some(Object::Stream(_)) => page.dict.get(b"Contents").cloned().into_iter().collect(),
        _ => return false,
    };
    let mut pieces = Vec::new();
    let mut read = 0usize;
    for o in &list {
        if read >= COVER_SCAN_BYTES {
            break;
        }
        // Only the page's own content: marks and added items are PdfCraft's (tagged).
        if stream_tag(doc, o).is_some() {
            continue;
        }
        let Object::Stream(stream) = &*doc.resolve(o) else { return false };
        // Bounded while decoding: a stream that would decode to more than what is left of the
        // budget ends the reading (its start is not separately decodable).
        let Ok(data) = stream.decoded_within(COVER_SCAN_BYTES.saturating_sub(read)) else { break };
        read = read.saturating_add(data.len());
        pieces.push(data);
    }
    let crop = page.crop(doc);
    // A Pattern colour space (named directly or through the page's /ColorSpace resources) can
    // tile with transparent gaps, so a fill in one doesn't prove anything is covered.
    let spaces = page.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict()?.get(b"ColorSpace").map(|c| doc.resolve(c)));
    let is_pattern = |name: &[u8]| -> bool {
        if name == b"Pattern" {
            return true;
        }
        let space = spaces.as_deref().and_then(Object::as_dict).and_then(|d| d.get(name)).map(|s| doc.resolve(s));
        match space.as_deref() {
            Some(Object::Name(n)) => n.as_slice() == b"Pattern",
            Some(Object::Array(a)) => a.first().map(|f| doc.resolve(f)).is_some_and(|f| matches!(&*f, Object::Name(n) if n.as_slice() == b"Pattern")),
            _ => false,
        }
    };
    let (mut ctm, mut saved) = (Matrix::IDENTITY, Vec::new());
    let mut rects: Vec<[f64; 4]> = Vec::new();
    let mut other_path = false;
    for op in Pieces::join(&pieces).parse().ops {
        match op.op.as_slice() {
            b"q" => saved.push(ctm),
            b"Q" => ctm = saved.pop().unwrap_or(Matrix::IDENTITY),
            b"cm" => match Matrix::from_operands(&op.operands) {
                Some(m) => ctm = m.then(&ctm),
                None => return false,
            },
            b"re" => {
                let Some([x, y, w, h]) = op.nums::<4>() else { return false };
                // Only a turn by a multiple of 90° keeps a rectangle's bounding box exact.
                let [a, b, c, d, _, _] = ctm.0;
                if !(b.abs() < 1e-9 && c.abs() < 1e-9 || a.abs() < 1e-9 && d.abs() < 1e-9) {
                    return false;
                }
                rects.push(ctm.bbox([x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h)]));
            }
            b"m" | b"l" | b"c" | b"v" | b"y" | b"h" => other_path = true,
            b"n" => {
                rects.clear();
                other_path = false;
            }
            b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                return match rects.as_slice() {
                    [r] => !other_path && r.iter().all(|v| v.is_finite()) && contains(*r, crop, 0.5),
                    _ => false,
                };
            }
            // The fill colour space: a pattern (or `scn` naming one) may not cover.
            b"cs" => match op.operands.last() {
                Some(Object::Name(n)) if !is_pattern(n) => {}
                _ => return false,
            },
            b"scn" if matches!(op.operands.last(), Some(Object::Name(_))) => return false,
            // Optional content (a layer) may be hidden, and the fill with it.
            b"BDC" if matches!(op.operands.first(), Some(Object::Name(n)) if n.as_slice() == b"OC") => return false,
            // Colour, line style and marked content change nothing that is painted.
            b"g" | b"G" | b"rg" | b"RG" | b"k" | b"K" | b"CS" | b"sc" | b"SC" | b"scn" | b"SCN" | b"w" | b"J" | b"j" | b"M" | b"d" | b"ri" | b"i"
            | b"BDC" | b"BMC" | b"EMC" | b"MP" | b"DP" | b"BX" | b"EX" => {}
            // Clipping, a `gs`, or anything painted before a fill: not a plain covering fill.
            _ => return false,
        }
    }
    false
}

/// Remove marks of `kind` from `pages`; returns how many were removed. When no mark is left on
/// top of a page, the q/Q wrapper goes too, restoring the original content list. A background
/// placed above a white page fill is the exception: the two streams its insertion split the
/// original into stay (same bytes, same rendering), so the list keeps one more piece.
pub fn remove_marks(doc: &mut Document, pages: &[usize], kind: MarkKind) -> Result<usize, EditError> {
    let all = page_list(doc);
    check(pages, all.len())?;
    let mut removed = 0;
    for &i in pages {
        let page = &all[i];
        let list = contents(doc, page)?;
        let before = list.len();
        let mut kept: Vec<Object> = list.into_iter().filter(|o| stream_tag(doc, o).as_deref() != Some(kind.tag())).collect();
        removed += before - kept.len();
        // Marks on top come after the closing wrapper; without any, the wrapper has no purpose.
        let is_wrap = |o: &Object| stream_tag(doc, o).as_deref() == Some("Wrap");
        let close = kept.iter().rposition(&is_wrap);
        let marks_on_top = close.is_some_and(|c| kept[c + 1..].iter().any(|o| stream_tag(doc, o).is_some_and(|t| t != "Wrap")));
        if !marks_on_top {
            kept.retain(|o| !is_wrap(o));
        }
        if before != kept.len() {
            doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(kept)))?;
        }
    }
    Ok(removed)
}

/// Which kinds of marks the document has (for enabling Update/Remove).
pub fn marks_present(doc: &Document) -> Vec<MarkKind> {
    added::scan_markers(doc).marks
}

pub mod objects;
pub use objects::{EditableObject, MAX_MOVE_OBJECTS, ObjectKind, ObjectTarget, editable_objects, move_objects};

mod flatten;
pub use flatten::{flatten, flatten_fill_sign};
pub mod images;
pub use images::{ImageChange, PageImage, change_image, page_images, reading_images, rect_to_rect, turn_about_centre};
pub mod bidi;
pub mod indic;
mod linebreak;
pub mod text;
pub use text::{
    BlockStyle, LineEdit, RangeStyle, TextBlock, TextLine, clear_block, reading_blocks, replace_block, replace_line, rewrite_block, text_blocks,
    text_lines,
};
pub mod added;
pub use added::{
    Added, AddedImage, AddedText, Align, Content, Family, MarkerScan, add_content, delete_content, first_undrawable, list_added, scan_markers,
    update_content,
};

#[cfg(test)]
mod tests;
