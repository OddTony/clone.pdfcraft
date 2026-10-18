//! Geometry-based regions and mixed-style rewriting of existing text.

use super::*;

fn same_plane(a: &TextLine, b: &TextLine) -> bool {
    let at = Matrix(a.origin.tm).then(&Matrix(a.origin.ctm));
    let bt = Matrix(b.origin.tm).then(&Matrix(b.origin.ctm));
    a.scope == b.scope
        && [0, 2].iter().all(|axis| {
            let ax = at.0.get(*axis).copied().unwrap_or(0.0);
            let ay = at.0.get(axis + 1).copied().unwrap_or(0.0);
            let bx = bt.0.get(*axis).copied().unwrap_or(0.0);
            let by = bt.0.get(axis + 1).copied().unwrap_or(0.0);
            let al = ax.hypot(ay);
            let bl = bx.hypot(by);
            al > 0.0 && bl > 0.0 && (ax / al - bx / bl).abs() < 0.001 && (ay / al - by / bl).abs() < 0.001
        })
}

fn row_rect(row: &[usize], lines: &[TextLine]) -> Option<[f64; 4]> {
    row.iter().filter_map(|i| lines.get(*i)).map(|l| l.rect).reduce(union)
}

/// [`row_rect`] in its text's own frame ([`TextLine::frame_rect`]), so rows of turned text
/// compare along and across their baselines.
fn frame_row_rect(row: &[usize], lines: &[TextLine]) -> Option<[f64; 4]> {
    row.iter().filter_map(|i| lines.get(*i)).map(TextLine::frame_rect).reduce(union)
}

pub(super) fn group_blocks(lines: &[TextLine]) -> Vec<TextBlock> {
    group_layout(lines, true)
}

/// Exporters currently represent a block with one style. Keep their style boundaries until
/// they support rich runs, while edit selection may encompass the complete visual region.
pub(super) fn group_reading_blocks(lines: &[TextLine]) -> Vec<TextBlock> {
    group_layout(lines, false)
}

/// Text-space units → user space along the baseline. It is the vertical scale ([`Origin::k`])
/// unless the text matrix stretches the glyphs, as an OCR layer fitting each word to the scan does.
fn across(line: &TextLine) -> f64 {
    let m = Matrix(line.origin.tm).then(&Matrix(line.origin.ctm)).0;
    let h = m[0].hypot(m[1]);
    if h.is_finite() && h > 1e-9 { h } else { line.origin.k }
}

/// Invisible text (rendering mode 3), as in an OCR layer over a scanned image.
fn invisible(line: &TextLine) -> bool {
    line.origin.state.render_mode == 3
}

fn same_style(a: &TextLine, b: &TextLine) -> bool {
    same_font(a, b.font.as_bytes(), b.face.as_deref())
        && a.base_font == b.base_font
        && a.bold == b.bold
        && a.italic == b.italic
        && a.color == b.color
        && (a.rotation() - b.rotation()).abs() < 0.01
        && (a.size - b.size).abs() < 0.01
}

/// The gap from `prev` to `line` along their baseline, in reading order: to the right of it, or,
/// when either reads right to left and `line` does not continue to the right, to its left. A
/// right-to-left row drawn as several runs (Arabic beside a Latin word or number) is one row.
fn reading_gap(prev: &TextLine, line: &TextLine) -> f64 {
    let forward = line.frame_rect()[0] - prev.frame_rect()[2];
    if forward < -prev.size.min(line.size) * 0.2 && (crate::bidi::rtl_base(&prev.text) || crate::bidi::rtl_base(&line.text)) {
        prev.frame_rect()[0] - line.frame_rect()[2]
    } else {
        forward
    }
}

fn group_layout(lines: &[TextLine], mixed: bool) -> Vec<TextBlock> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let joins = rows.last().and_then(|r| r.last()).and_then(|i| lines.get(*i)).is_some_and(|prev| {
            let gap = reading_gap(prev, line);
            // OCR words (invisible text over a scan) join only across a word space: a wider gap
            // is a table cell or a tab stop, which a rewrite would close up.
            let scanned = !mixed || (invisible(prev) == invisible(line) && (!invisible(line) || gap < prev.size.max(line.size) * 0.6));
            scanned
                && (mixed || same_style(prev, line))
                && same_plane(prev, line)
                && (prev.frame_origin().1 - line.frame_origin().1).abs() < prev.size.min(line.size) * 0.2
                && gap >= -prev.size.min(line.size) * 0.2
                && gap < prev.size.max(line.size) * 1.5
                // Separate same-style text objects that merely touch or overlap, with no space
                // between them, are separate labels (for example a paragraph dragged against
                // another, #594). Style runs and words separated by a space or gap still join.
                && (prev.origin.bt_op == line.origin.bt_op
                    || gap > prev.size.min(line.size) * 0.01
                    || !same_style(prev, line)
                    || prev.text.ends_with(char::is_whitespace)
                    || line.text.starts_with(char::is_whitespace))
        });
        if joins && let Some(row) = rows.last_mut() {
            row.push(i);
        } else {
            rows.push(vec![i]);
        }
    }
    let mut blocks: Vec<TextBlock> = Vec::new();
    for row in rows {
        let Some(first) = row.first().and_then(|i| lines.get(*i)) else { continue };
        let (Some(rect), Some(frame)) = (row_rect(&row, lines), frame_row_rect(&row, lines)) else { continue };
        let joins = blocks.last().is_some_and(|block| {
            let Some(prev_row) = block.rows.last() else { return false };
            let Some(prev) = prev_row.first().and_then(|i| lines.get(*i)) else { return false };
            let Some(pr) = frame_row_rect(prev_row, lines) else { return false };
            let gap = prev.frame_origin().1 - first.frame_origin().1;
            let tol = prev.size.max(first.size).max(1.0);
            let aligned =
                (pr[0] - frame[0]).abs() < tol || ((pr[0] + pr[2] - frame[0] - frame[2]) / 2.0).abs() < tol || (pr[2] - frame[2]).abs() < tol;
            // OCR lines stay apart: the scan fixes where each one is, so they never rewrap together.
            (!mixed || !(invisible(prev) || invisible(first)))
                && (mixed || same_style(prev, first))
                && same_plane(prev, first)
                && aligned
                && first.size.min(prev.size) >= first.size.max(prev.size) * 0.4
                && gap > first.size.min(prev.size) * 0.8
                && gap < first.size.max(prev.size) * 2.5
        });
        if !joins {
            blocks.push(TextBlock {
                text: String::new(),
                editor_text: String::new(),
                spans: Vec::new(),
                rect,
                base_font: first.base_font.clone(),
                size: first.size,
                bold: first.bold,
                italic: first.italic,
                color: first.color,
                rotation: first.rotation(),
                lines: Vec::new(),
                rows: Vec::new(),
                invisible: true,
            });
        }
        let Some(block) = blocks.last_mut() else { continue };
        let mut row_text = String::new();
        if !block.rows.is_empty() {
            block.editor_text.push('\n');
        }
        for &index in &row {
            let Some(line) = lines.get(index) else { continue };
            // Some producers position separate words without encoding a space.
            if let Some(prev) = block.lines.last().filter(|i| row.contains(i)).and_then(|i| lines.get(*i))
                && !row_text.ends_with(char::is_whitespace)
                && !line.text.starts_with(char::is_whitespace)
                && reading_gap(prev, line) > line.size * 0.12
            {
                row_text.push(' ');
                block.editor_text.push(' ');
            }
            let start = block.editor_text.len();
            block.editor_text.push_str(&line.text);
            block.spans.push((start..block.editor_text.len(), index));
            row_text.push_str(&line.text);
            block.invisible &= line.origin.state.render_mode == 3;
            block.lines.push(index);
        }
        if !block.text.is_empty() {
            if block.text.ends_with('-') {
                block.text.pop();
            } else {
                block.text.push(' ');
            }
        }
        block.text.push_str(row_text.trim_matches(paragraph_separator));
        block.rect = union(block.rect, rect);
        block.rows.push(row);
    }
    blocks
}

/// Find the complete source program. Never use an interface fallback as a document font.
fn source_face(doc: &Document, fonts: &Dict, name: &str, text: &str) -> Result<(std::sync::Arc<Vec<u8>>, String), EditError> {
    let source =
        fonts.get(name.as_bytes()).and_then(|f| doc.dict(f)).ok_or_else(|| EditError::Invalid("the shaped text has no font resource".into()))?;
    let family = Metrics::from_dict(doc, &source).base_font;
    let mut candidates = vec![source];
    candidates.extend(
        fonts
            .iter()
            .filter_map(|(_, f)| doc.dict(f))
            .filter(|f| pdfcraft_fonts::embedding::face_key(&Metrics::from_dict(doc, f).base_font) == pdfcraft_fonts::embedding::face_key(&family)),
    );
    for font in candidates {
        if let Some(bytes) = pdfcraft_fonts::pdf::embedded_sfnt(doc, &font)
            && pdfcraft_fonts::embedding::covers(&bytes, text)
            && pdfcraft_fonts::embedding::validate(&bytes).is_ok()
        {
            return Ok((std::sync::Arc::new(bytes), family));
        }
    }
    Err(EditError::Invalid("complex text needs the complete source TrueType/OpenType face; load that font before editing".into()))
}

struct ContextualParagraph<'a> {
    chars: std::ops::Range<usize>,
    bytes: Vec<usize>,
    layout: pdfcraft_fonts::source_shaping::Paragraph<'a>,
    face: std::sync::Arc<Vec<u8>>,
    family: String,
}

impl ContextualParagraph<'_> {
    fn byte_range(&self, range: std::ops::Range<usize>) -> Result<std::ops::Range<usize>, EditError> {
        let invalid = || EditError::Invalid("the shaped source range is outside its paragraph".into());
        let begin = range.start.checked_sub(self.chars.start).ok_or_else(invalid)?;
        let end = range.end.checked_sub(self.chars.start).ok_or_else(invalid)?;
        if begin > end {
            return Err(invalid());
        }
        let begin = self.bytes.get(begin).copied().ok_or_else(invalid)?;
        let end = self.bytes.get(end).copied().ok_or_else(invalid)?;
        Ok(begin..end)
    }
}

/// For each output row (a range of the draft's characters), whether it is exactly the source row
/// at the same position: the same characters, none of them inserted or changed by the edit.
fn untouched_rows(block: &TextBlock, draft: &str, rows: &[std::ops::Range<usize>]) -> Vec<bool> {
    let old: Vec<char> = block.editor_text.chars().collect();
    let new: Vec<char> = draft.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old.iter().rev().zip(new.iter().rev()).take(old.len().min(new.len()).saturating_sub(prefix)).take_while(|(a, b)| a == b).count();
    // The source character a draft character is, if the edit left it alone.
    let origin = |i: usize| {
        if i < prefix {
            Some(i)
        } else if i >= new.len().saturating_sub(suffix) {
            (old.len() + i).checked_sub(new.len())
        } else {
            None
        }
    };
    let mut source_rows = Vec::new();
    let mut start = 0;
    for (i, ch) in old.iter().enumerate() {
        if *ch == '\n' {
            source_rows.push(start..i);
            start = i + 1;
        }
    }
    source_rows.push(start..old.len());
    // A line break inside a source run would pair editor rows with the wrong visual rows.
    if source_rows.len() != block.rows.len() {
        return vec![false; rows.len()];
    }
    rows.iter()
        .enumerate()
        .map(|(r, range)| {
            source_rows.get(r).is_some_and(|source| {
                !range.is_empty() && range.len() == source.len() && range.clone().zip(source.clone()).all(|(i, j)| origin(i) == Some(j))
            })
        })
        .collect()
}

/// Rewrite visual rows while keeping the source style on each surviving character. Resource
/// names may differ between Unicode subsets of the same face; encoding searches those subsets.
pub(super) fn rewrite_styled_block(
    doc: &mut Document,
    page: usize,
    block: &TextBlock,
    lines: &[TextLine],
    text: Option<&str>,
    style: &BlockStyle,
) -> Result<LineEdit, EditError> {
    // Font completion and shaped CID maps may allocate before a later paragraph is validated.
    // Commit once: rejected font/style/geometry input must leave the source PDF byte-identical.
    let mut edited = doc.clone();
    let result = rewrite_styled_block_inner(&mut edited, page, block, lines, text, style)?;
    *doc = edited;
    Ok(result)
}

fn rewrite_styled_block_inner(
    doc: &mut Document,
    page: usize,
    block: &TextBlock,
    lines: &[TextLine],
    text: Option<&str>,
    style: &BlockStyle,
) -> Result<LineEdit, EditError> {
    let first = block.lines.first().and_then(|i| lines.get(*i)).ok_or_else(|| EditError::Invalid("the paragraph has no source runs".into()))?;
    let draft = match text {
        Some(t) if t != block.text => t,
        _ => &block.editor_text,
    };
    if draft.len() > 4_000_000 {
        return Err(EditError::Invalid("the replacement text is too large".into()));
    }
    if draft == block.editor_text && *style == BlockStyle::default() {
        return Ok(LineEdit { substituted: None });
    }
    let mut ranges: Vec<_> = style.ranges.iter().collect();
    ranges.sort_by_key(|r| r.range.start);
    if ranges.len() > 1024
        || ranges.iter().any(|r| {
            r.range.start >= r.range.end
                || draft.get(r.range.clone()).is_none()
                || [r.size, r.char_spacing, r.scale].iter().flatten().any(|n| !n.is_finite())
                || r.size.is_some_and(|n| n <= 0.0 || n > 14400.0)
                || r.color.is_some_and(|c| c.iter().any(|n| !n.is_finite() || !(0.0..=1.0).contains(n)))
        })
        || ranges.windows(2).any(|r| r[0].range.end > r[1].range.start)
    {
        return Err(EditError::Invalid("text style ranges must be disjoint UTF-8 boundaries with finite formatting (at most 1024 ranges)".into()));
    }
    let source_count = lines.len();
    let mut styled_lines = lines.to_vec();
    let mut styled_spans = Vec::new();
    let mut underlines = HashMap::new();
    let mut overrides = HashMap::new();
    for (span, source) in block.edited_spans(draft) {
        let mut edges = vec![span.start, span.end];
        for r in &ranges {
            for edge in [r.range.start, r.range.end] {
                if span.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        for bounds in edges.windows(2) {
            let (Some(start), Some(end)) = (bounds.first().copied(), bounds.get(1).copied()) else { continue };
            let mut index = source;
            let selected = ranges.iter().find(|r| r.range.contains(&start)).copied();
            if selected.is_some() || style.family.is_some() || style.bold.is_some() {
                let default_range = RangeStyle {
                    range: start..end,
                    family: None,
                    resource: None,
                    size: None,
                    color: None,
                    underline: None,
                    char_spacing: None,
                    scale: None,
                };
                let r = selected.unwrap_or(&default_range);
                let mut line = lines.get(source).unwrap_or(first).clone();
                if let Some((family, bold, italic)) =
                    r.family.or(style.family).or_else(|| style.bold.map(|b| (source_family(&line.base_font), b, line.italic)))
                {
                    let base = family.base_font(bold, italic);
                    line.font = format!("PCRange{}", base.replace('-', ""));
                    line.base_font = base.to_owned();
                    line.bold = bold;
                    line.italic = italic;
                }
                if let Some(resource) = &r.resource {
                    line.font = resource.clone();
                }
                if let Some(size) = r.size {
                    line.size = size;
                }
                if let Some(color) = r.color {
                    line.color = color;
                    line.origin.state.fill = vec![Op::new("rg", color.map(Object::Real).to_vec())];
                }
                if let Some(spacing) = r.char_spacing {
                    line.origin.state.char_spacing = spacing / line.origin.k.max(1e-6);
                }
                if let Some(scale) = r.scale {
                    line.origin.state.scale = (scale / 100.0).clamp(0.1, 10.0);
                }
                index = styled_lines.len();
                styled_lines.push(line);
                if let Some(r) = selected {
                    overrides.insert(index, r);
                }
                if let Some(underline) = r.underline {
                    underlines.insert(index, underline);
                }
            }
            styled_spans.push((start..end, index));
        }
    }
    let lines = styled_lines.as_slice();
    let p = page_dict(doc, page)?;
    let mut res = p.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let mut fonts = res.get(b"Font").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let streams = content_streams(doc, &p.dict);
    let pieces = Pieces::join(&streams.iter().map(|(_, d)| d.as_slice()).collect::<Vec<_>>());
    let ops = pieces.parse().ops;
    let shown = interpret(doc, &ops, &fonts, &mut HashMap::new(), &mut Carry::new(), None, None);
    let source_ops: std::collections::HashSet<_> = block.lines.iter().filter_map(|i| lines.get(*i)).flat_map(|l| l.ops.iter().copied()).collect();
    let affected_bts: std::collections::HashSet<_> = shown.iter().filter(|s| source_ops.contains(&s.op)).map(|s| s.bt).collect();
    if shown.iter().any(|s| affected_bts.contains(&s.bt) && s.state.render_mode >= 4) {
        return Err(EditError::Invalid("text used as a clipping path cannot be rewritten".into()));
    }
    for line in lines.iter().filter(|line| line.font.starts_with("PCRange")) {
        let mut f = Dict::new();
        f.set(b"Type".to_vec(), Object::name("Font"));
        f.set(b"Subtype".to_vec(), Object::name("Type1"));
        f.set(b"BaseFont".to_vec(), Object::name(&line.base_font));
        f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        fonts.set(line.font.as_bytes().to_vec(), Object::Dict(f));
    }
    let mut metrics = std::collections::BTreeMap::new();
    for (name, object) in fonts.iter().take(256) {
        if let Some(font) = doc.resolve(object).as_dict() {
            metrics.insert(String::from_utf8_lossy(name).into_owned(), Metrics::from_dict(doc, font));
        }
    }
    for index in &block.lines {
        if let Some(line) = lines.get(*index)
            && !metrics.contains_key(&line.font)
            && let Some(font) = fonts.get(line.font.as_bytes()).and_then(|f| doc.resolve(f).as_dict().cloned())
        {
            metrics.insert(line.font.clone(), Metrics::from_dict(doc, &font));
        }
    }
    if [style.size, style.line_spacing, style.char_spacing, style.scale, style.width].iter().flatten().any(|v| !v.is_finite())
        || style.color.is_some_and(|c| c.iter().any(|v| !v.is_finite()))
    {
        return Err(EditError::Invalid("the paragraph's formatting must use finite numbers".into()));
    }
    let [dx, dy] = style.offset.unwrap_or([0.0, 0.0]);
    if !dx.is_finite() || !dy.is_finite() || style.width.is_some_and(|w| !w.is_finite()) {
        return Err(EditError::Invalid("the paragraph's position and width must be finite".into()));
    }
    // A visual row grows even when it contains several source/font runs. Multi-row
    // paragraphs retain their container width; an explicit width always wins.
    let rtl = pdfcraft_fonts::appearance_text::paragraph_rtl(draft);
    let natural = if block.invisible || block.rows.len() == 1 {
        let available = if rtl { block.rect[2] + dx - (p.crop(doc)[0] + 36.0) } else { p.crop(doc)[2] - 36.0 - (block.rect[0] + dx) };
        available.max(block.rect[2] - block.rect[0])
    } else {
        block.rect[2] - block.rect[0]
    };
    let width = style.width.unwrap_or(natural).clamp(1.0, 14_400.0);
    // Each tuple is a character, its source style and an encodable resource name.
    let mut chars = Vec::new();
    let mut substituted = None;
    let mut fallback_names: std::collections::BTreeMap<(String, char), String> = std::collections::BTreeMap::new();
    let mut full_faces: HashMap<String, String> = HashMap::new();
    let mut programs: HashMap<String, Option<std::sync::Arc<Vec<u8>>>> = HashMap::new();
    for (range, index) in styled_spans {
        let source = lines.get(index).unwrap_or(first);
        for ch in draft.get(range).unwrap_or_default().chars() {
            if ch == '\n' {
                chars.push((ch, index, source.font.clone()));
                continue;
            }
            if pdfcraft_fonts::embedding::default_ignorable(ch) {
                let resource = overrides.get(&index).and_then(|r| r.resource.as_ref()).or(style.resource.as_ref()).unwrap_or(&source.font);
                if !metrics.contains_key(resource) {
                    return Err(EditError::Invalid("the selected font resource is missing".into()));
                }
                chars.push((ch, index, resource.clone()));
                continue;
            }
            let s = ch.to_string();
            let name = if let Some(resource) = overrides.get(&index).and_then(|r| r.resource.as_ref()).or(style.resource.as_ref()) {
                if !metrics.get(resource).is_some_and(|m| source_encodes(m, &s)) {
                    return Err(EditError::Invalid(format!("the selected font resource {resource} cannot show U+{:04X}", u32::from(ch))));
                }
                resource.clone()
            } else if metrics.get(&source.font).is_some_and(|m| source_encodes(m, &s)) {
                source.font.clone()
            } else if let Some((name, _)) = metrics.iter().find(|(_, m)| {
                pdfcraft_fonts::embedding::face_key(&m.base_font) == pdfcraft_fonts::embedding::face_key(&source.base_font) && source_encodes(m, &s)
            }) {
                name.clone()
            } else if let Some(face) =
                pdfcraft_fonts::embedding::matching_faces(&source.base_font).into_iter().find(|f| pdfcraft_fonts::embedding::covers(f.bytes, &s))
            {
                let key = face.name();
                if let Some(name) = full_faces.get(&key) {
                    name.clone()
                } else {
                    let name = pdfcraft_fonts::embedding::embed(doc, &mut fonts, face.bytes, face.family).map_err(EditError::Invalid)?;
                    let font = fonts
                        .get(name.as_bytes())
                        .and_then(|f| doc.dict(f))
                        .ok_or_else(|| EditError::Invalid("the full source font could not be loaded".into()))?;
                    metrics.insert(name.clone(), Metrics::from_dict(doc, &font));
                    full_faces.insert(key, name.clone());
                    name
                }
            } else if let Some(bytes) = programs
                .entry(source.font.clone())
                .or_insert_with(|| source_program(doc, &fonts, &source.font).map(std::sync::Arc::new))
                .clone()
                .filter(|bytes| pdfcraft_fonts::embedding::covers(bytes, &s))
            {
                // The document's own complete program of this font has the glyph: draw it with
                // that face rather than another family's fallback.
                let key = format!("source {}", source.font);
                if let Some(name) = full_faces.get(&key) {
                    name.clone()
                } else {
                    let family = pdfcraft_fonts::embedding::names(&bytes).map_or_else(|_| source.base_font.clone(), |(family, _)| family);
                    let name = pdfcraft_fonts::embedding::embed(doc, &mut fonts, &bytes, &family).map_err(EditError::Invalid)?;
                    let font = fonts
                        .get(name.as_bytes())
                        .and_then(|f| doc.dict(f))
                        .ok_or_else(|| EditError::Invalid("the source font could not be loaded".into()))?;
                    metrics.insert(name.clone(), Metrics::from_dict(doc, &font));
                    full_faces.insert(key, name.clone());
                    name
                }
            } else {
                let family = source_family(&source.base_font);
                if needs_type3(&s) {
                    let key = (source.base_font.rsplit('+').next().unwrap_or(&source.base_font).to_owned(), ch);
                    if let Some(name) = fallback_names.get(&key) {
                        chars.push((ch, index, name.clone()));
                        continue;
                    }
                    if fallback_names.len() >= MAX_TYPE3_GLYPHS {
                        return Err(EditError::Invalid("the replacement has too many fallback glyphs".into()));
                    }
                    let fallback = type3_font(doc, &mut fonts, &s, family, source.bold, MAX_TYPE3_GLYPHS)?;
                    let name = fallback.names.first().cloned().ok_or_else(no_japanese_font)?;
                    let dict = fonts
                        .get(name.as_bytes())
                        .and_then(|f| doc.resolve(f).as_dict().cloned())
                        .ok_or_else(|| EditError::Invalid("the fallback font could not be loaded".into()))?;
                    metrics.insert(name.clone(), Metrics::from_dict(doc, &dict));
                    substituted = Some(fallback.family.to_string());
                    fallback_names.insert(key, name.clone());
                    name
                } else {
                    let base = family.base_font(source.bold, source.italic);
                    let name = format!("PCEd{}", base.replace('-', ""));
                    let mut f = Dict::new();
                    f.set(b"Type".to_vec(), Object::name("Font"));
                    f.set(b"Subtype".to_vec(), Object::name("Type1"));
                    f.set(b"BaseFont".to_vec(), Object::name(base));
                    f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
                    metrics.entry(name.clone()).or_insert_with(|| Metrics::from_dict(doc, &f));
                    fonts.set(name.clone().into_bytes(), Object::Dict(f));
                    substituted = Some(base.to_string());
                    name
                }
            };
            chars.push((ch, index, name));
        }
    }
    let size_of = |source: &TextLine, index: usize| {
        overrides.get(&index).and_then(|r| r.size).or(style.size).unwrap_or(source.size).max(0.1) / source.origin.k.max(1e-6)
    };
    let mut paragraph_texts = Vec::new();
    let mut char_start = 0;
    for paragraph in draft.split('\n') {
        let end = char_start + paragraph.chars().count();
        paragraph_texts.push((char_start..end, paragraph.to_owned()));
        char_start = end + 1;
    }
    let effective = |index: usize| {
        let source = lines.get(index).unwrap_or(first);
        let r = overrides.get(&index);
        (
            size_of(source, index),
            r.and_then(|r| r.color).or(style.color).unwrap_or(source.color),
            r.and_then(|r| r.char_spacing).or(style.char_spacing).map_or(source.origin.state.char_spacing, |v| v / source.origin.k.max(1e-6)),
            r.and_then(|r| r.scale).or(style.scale).map_or(source.origin.state.scale, |v| (v / 100.0).clamp(0.1, 10.0)),
            source.origin.state.word_spacing,
            source.origin.k,
            source.origin.state.render_mode,
            underlines.get(&index).copied().or(style.underline),
        )
    };
    let mut faces = HashMap::new();
    let mut programs: Vec<std::sync::Arc<Vec<u8>>> = Vec::new();
    let mut program_bytes = 0usize;
    let mut contexts = Vec::new();
    for (range, paragraph) in &paragraph_texts {
        if !pdfcraft_fonts::source_shaping::required(paragraph) || range.is_empty() {
            continue;
        }
        let Some(seed) = chars.get(range.start).cloned() else { continue };
        let look = effective(seed.1);
        let mut face = None;
        let mut family = String::new();
        let mut seen = std::collections::HashSet::new();
        for item in chars.get(range.clone()).unwrap_or_default() {
            if effective(item.1) != look {
                return Err(EditError::Invalid("contextual/bidi text across different styles needs a uniform full-face paragraph".into()));
            }
            if !faces.contains_key(&item.2) {
                if faces.len() >= 256 {
                    return Err(EditError::Invalid("the contextual text uses too many source font resources".into()));
                }
                let (bytes, family) = source_face(doc, &fonts, &item.2, paragraph)?;
                let bytes = if let Some(previous) = programs.iter().find(|previous| previous.as_slice() == bytes.as_slice()) {
                    previous.clone()
                } else {
                    program_bytes = program_bytes
                        .checked_add(bytes.len())
                        .filter(|total| *total <= 64 * 1024 * 1024)
                        .ok_or_else(|| EditError::Invalid("the shaped source fonts exceed their memory budget".into()))?;
                    if programs.len() >= 256 {
                        return Err(EditError::Invalid("the paragraph uses too many distinct source fonts".into()));
                    }
                    programs.push(bytes.clone());
                    bytes
                };
                faces.insert(item.2.clone(), (bytes, family));
            }
            if !seen.insert(&item.2) {
                continue;
            }
            let (bytes, name) = faces.get(&item.2).ok_or_else(|| EditError::Invalid("the source face is missing".into()))?;
            if face.as_ref().is_some_and(|f: &std::sync::Arc<Vec<u8>>| !std::sync::Arc::ptr_eq(f, bytes) && f.as_slice() != bytes.as_slice()) {
                return Err(EditError::Invalid("contextual/bidi text across different font programs needs a uniform full-face paragraph".into()));
            }
            face = Some(bytes.clone());
            family = name.clone();
        }
        // Distinct CID maps of the same complete font do not represent distinct styles. Keep
        // one source identity while shaping so editing a saved Arabic row stays contextual.
        for item in chars.get_mut(range.clone()).unwrap_or_default() {
            item.1 = seed.1;
            item.2 = seed.2.clone();
        }
        let mut bytes: Vec<_> = paragraph.char_indices().map(|(at, _)| at).collect();
        bytes.push(paragraph.len());
        contexts.push(ContextualParagraph {
            chars: range.clone(),
            bytes,
            layout: pdfcraft_fonts::source_shaping::Paragraph::new(paragraph).map_err(EditError::Invalid)?,
            face: face.ok_or_else(|| EditError::Invalid("the source face is missing".into()))?,
            family,
        });
    }
    let context_for = |range: std::ops::Range<usize>| {
        contexts
            .get(contexts.partition_point(|p| p.chars.start <= range.start).saturating_sub(1))
            .filter(|p| p.chars.start <= range.start && range.end <= p.chars.end)
    };
    let shape_at = |range: std::ops::Range<usize>| -> Result<Option<_>, EditError> {
        let Some(context) = context_for(range.clone()) else { return Ok(None) };
        let index = chars.get(context.chars.start).ok_or_else(|| EditError::Invalid("the shaped paragraph has no source style".into()))?.1;
        let source = lines.get(index).unwrap_or(first);
        let size = size_of(source, index);
        let scale = overrides.get(&index).and_then(|r| r.scale).or(style.scale).map_or(source.origin.state.scale, |s| (s / 100.0).clamp(0.1, 10.0));
        let unit = size * scale * source.origin.k;
        if !unit.is_finite() || unit <= 0.0 {
            return Err(EditError::Invalid("the shaped source text has an unsupported scale".into()));
        }
        let spacing = overrides
            .get(&index)
            .and_then(|r| r.char_spacing)
            .or(style.char_spacing)
            .map_or(source.origin.state.char_spacing, |s| s / source.origin.k.max(1e-6));
        let shaped = context
            .layout
            .shape_fitted(&context.face, context.byte_range(range)?, width / unit, spacing / size, source.origin.state.word_spacing / size)
            .map_err(EditError::Invalid)?;
        Ok(Some((context.face.clone(), shaped, context.family.clone())))
    };
    // A source run may carry TJ kerning/justification that is absent from its text state.
    // Keep its measured advance when rebuilding the run instead of making it wrap on entry.
    // Each font's space width, worked out once: when a font can't show a space, finding one
    // reads every font on the page, and doing that for every character of a paragraph made each
    // keystroke in a long paragraph take half a second (#2616).
    // Only for the fonts the paragraph uses, when first asked: working out every page font's width
    // up front would scan the page once per font for a short edit on a page of many fonts.
    let spaces: std::cell::RefCell<HashMap<String, f64>> = std::cell::RefCell::new(HashMap::new());
    let space_of = |name: &str, m: &Metrics| {
        if let Some(w) = spaces.borrow().get(name) {
            return *w;
        }
        let w = source_space(doc, &fonts, m);
        spaces.borrow_mut().insert(name.to_owned(), w);
        w
    };
    let mut calibration = HashMap::new();
    for (index, source) in lines.iter().enumerate() {
        if let Some(m) = metrics.get(&source.font) {
            let nominal = source_width(
                m,
                &source.text,
                source.origin.state.font.as_ref().map_or(1.0, |(_, s)| *s),
                space_of(&source.font, m),
                source.origin.state.char_spacing,
                source.origin.state.word_spacing,
            ) * source.origin.state.scale
                * across(source);
            let changed_face = style.resource.is_some() || overrides.get(&index).is_some_and(|r| r.family.is_some() || r.resource.is_some());
            let factor = if nominal > 0.0 && !changed_face { ((source.rect[2] - source.rect[0]) / nominal).clamp(0.5, 2.0) } else { 1.0 };
            calibration.insert(index, factor);
        }
    }
    let advance = |item: &(char, usize, String)| -> f64 {
        let source = lines.get(item.1).unwrap_or(first);
        let Some(m) = metrics.get(&item.2) else { return 0.0 };
        source_width(
            m,
            &item.0.to_string(),
            size_of(source, item.1),
            space_of(&item.2, m),
            overrides
                .get(&item.1)
                .and_then(|r| r.char_spacing)
                .or(style.char_spacing)
                .map_or(source.origin.state.char_spacing, |c| c / source.origin.k.max(1e-6)),
            source.origin.state.word_spacing,
        ) * overrides.get(&item.1).and_then(|r| r.scale).or(style.scale).map_or(source.origin.state.scale, |s| (s / 100.0).clamp(0.1, 10.0))
            * across(source)
            * calibration.get(&item.1).copied().unwrap_or(1.0)
    };
    let measure = |range: std::ops::Range<usize>| -> Result<f64, EditError> {
        let items = chars.get(range.clone()).unwrap_or_default();
        let mut width = 0.0;
        let mut at = 0;
        while let Some(item) = items.get(at) {
            let mut end = at + 1;
            while items.get(end).is_some_and(|v| v.1 == item.1 && v.2 == item.2) {
                end += 1;
            }
            let segment = items.get(at..end).unwrap_or_default();
            if let Some((_, shaped, _)) = shape_at(range.start + at..range.start + end)? {
                let source = lines.get(item.1).unwrap_or(first);
                let scale =
                    overrides.get(&item.1).and_then(|r| r.scale).or(style.scale).map_or(source.origin.state.scale, |s| (s / 100.0).clamp(0.1, 10.0));
                let spacing = overrides
                    .get(&item.1)
                    .and_then(|r| r.char_spacing)
                    .or(style.char_spacing)
                    .map_or(source.origin.state.char_spacing, |s| s / source.origin.k.max(1e-6));
                let size = size_of(source, item.1);
                width += shaped.spaced_advance(spacing / size, source.origin.state.word_spacing / size) * size * scale * source.origin.k;
            } else {
                width += segment.iter().map(&advance).sum::<f64>();
            }
            at = end;
        }
        Ok(width)
    };
    let mut rows: Vec<Vec<(char, usize, String)>> = Vec::new();
    // Each row's characters, as indexes into the draft's characters (`chars` holds all of them).
    let mut row_ranges = Vec::new();
    let mut row_start = 0;
    let mut row = Vec::new();
    let mut row_width = 0.0;
    // Wrap at word boundaries, retaining explicit source row breaks and whitespace.
    let mut at = 0;
    while at < chars.len() {
        if let Some(context) = context_for(at..at).filter(|p| p.chars.start == at) {
            let failure = std::cell::RefCell::new(None);
            let ranges = pdfcraft_fonts::appearance_text::wrap_ranges(context.layout.text(), width, |bytes| {
                let begin = context.chars.start + context.bytes.partition_point(|b| *b < bytes.start);
                let end = context.chars.start + context.bytes.partition_point(|b| *b < bytes.end);
                measure(begin..end).unwrap_or_else(|error| {
                    *failure.borrow_mut() = Some(error);
                    f64::INFINITY
                })
            });
            if let Some(error) = failure.into_inner() {
                return Err(error);
            }
            for bytes in ranges {
                let start = context.chars.start + context.bytes.partition_point(|b| *b < bytes.start);
                let end = context.chars.start + context.bytes.partition_point(|b| *b < bytes.end);
                row_ranges.push(start..end);
                rows.push(chars.get(start..end).unwrap_or_default().to_vec());
            }
            at = context.chars.end;
            if chars.get(at).is_some_and(|item| item.0 == '\n') {
                at += 1;
            }
            row_start = at;
            continue;
        }
        let Some(item) = chars.get(at) else { break };
        if item.0 == '\n' {
            row_ranges.push(row_start..row_start + row.len());
            rows.push(std::mem::take(&mut row));
            row_width = 0.0;
            at += 1;
            row_start = at;
            continue;
        }
        let mut end = at + 1;
        if !item.0.is_whitespace() {
            // A word ends at a space, or where Chinese or Japanese text may break between characters.
            while let (Some(before), Some(c)) = (chars.get(end - 1), chars.get(end))
                && !c.0.is_whitespace()
                && !crate::linebreak::between(before.0, c.0)
            {
                end += 1;
            }
        }
        let word = chars.get(at..end).unwrap_or_default();
        let w = measure(at..end)?;
        if !row.is_empty() && !item.0.is_whitespace() && row_width + w > width + 0.5 {
            while row.last().is_some_and(|c: &(char, usize, String)| c.0.is_whitespace()) {
                row.pop();
            }
            row_ranges.push(row_start..row_start + row.len());
            rows.push(std::mem::take(&mut row));
            row_width = 0.0;
            row_start = at;
        }
        row.extend_from_slice(word);
        row_width += w;
        at = end;
    }
    if !row.is_empty() {
        row_ranges.push(row_start..row_start + row.len());
        rows.push(row);
    }
    // A row the edit doesn't reach keeps its source operators, so its kerning, TJ word gaps and
    // glyph widths stay exactly as they were instead of being rebuilt from the text state.
    let plain = BlockStyle { ranges: Vec::new(), ..style.clone() } == BlockStyle::default();
    let mut kept = if plain {
        untouched_rows(block, draft, &row_ranges).into_iter().zip(&rows).map(|(same, row)| same && row.iter().all(|c| c.1 < source_count)).collect()
    } else {
        vec![false; rows.len()]
    };
    let kept_lines: std::collections::HashSet<usize> =
        kept.iter().enumerate().filter(|(_, k)| **k).filter_map(|(r, _)| block.rows.get(r)).flatten().copied().collect();
    let rewritten: Vec<&TextLine> = block.lines.iter().filter(|i| !kept_lines.contains(i)).filter_map(|i| lines.get(*i)).collect();
    // The rewrite goes where the last rewritten row was drawn, after any backgrounds painted
    // between the rows. A dropped `'` or `"` also moved to the next line, which a kept row relies on.
    let lead =
        rewritten.last().copied().filter(|_| !rewritten.iter().flat_map(|l| &l.ops).any(|i| ops.get(*i).is_some_and(|op| op.is("'") || op.is("\""))));
    let (members, lead) = match lead {
        Some(lead) if !kept_lines.is_empty() => (rewritten, lead),
        _ => {
            kept.iter_mut().for_each(|k| *k = false);
            let all: Vec<&TextLine> = block.lines.iter().filter_map(|i| lines.get(*i)).collect();
            let last = all.last().copied().unwrap_or(first);
            (all, last)
        }
    };
    let o = &lead.origin;
    for (row, _) in rows.iter().zip(&kept).filter(|(_, k)| !**k) {
        let text: String = row.iter().map(|v| v.0).collect();
        if pdfcraft_fonts::source_shaping::required(&text) && row.first().is_some_and(|first| row.iter().any(|v| v.1 != first.1 || v.2 != first.2)) {
            return Err(EditError::Invalid(
                "contextual/bidi text across font or style boundaries needs a uniform full-face row; mixed contextual runs are not yet supported"
                    .into(),
            ));
        }
    }
    let measured: Vec<_> = row_ranges.iter().cloned().map(measure).collect::<Result<_, _>>()?;
    let mut shaped_runs = HashMap::new();
    let mut segment_advances = HashMap::new();
    for (row_index, row) in rows.iter().enumerate().filter(|(r, _)| !kept.get(*r).copied().unwrap_or(false)) {
        let mut at = 0;
        while let Some(item) = row.get(at) {
            let mut end = at + 1;
            while row.get(end).is_some_and(|v| v.1 == item.1 && v.2 == item.2) {
                end += 1;
            }
            segment_advances.insert((row_index, at), row.get(at..end).unwrap_or_default().iter().map(&advance).sum::<f64>());
            let start = row_ranges.get(row_index).map_or(0, |r| r.start);
            if let Some((bytes, shaped, family)) = shape_at(start + at..start + end)? {
                shaped_runs.insert((row_index, at), (bytes, shaped, family));
            }
            at = end;
        }
    }
    // One full program per CID-map chunk, shared across wrapped rows. Embedding an entire
    // font for every short row amplifies a narrow paragraph into hundreds of font programs.
    let mut groups = Vec::new();
    let mut active_groups = HashMap::new();
    let mut glyph_resources = HashMap::new();
    let mut shaped_keys: Vec<_> = shaped_runs.keys().copied().collect();
    shaped_keys.sort_unstable();
    for key in shaped_keys {
        let (bytes, shaped, family) = &shaped_runs[&key];
        let face = std::sync::Arc::as_ptr(bytes) as usize;
        let mut codes = Vec::new();
        for glyph in &shaped.glyphs {
            let index = *active_groups.entry(face).or_insert_with(|| {
                groups.push((bytes.clone(), family.clone(), Vec::new(), HashMap::new()));
                groups.len() - 1
            });
            let mapping = (glyph.id.to_u32(), glyph.text.clone());
            let index = if groups[index].2.len() >= 65_534 && !groups[index].3.contains_key(&mapping) {
                groups.push((bytes.clone(), family.clone(), Vec::new(), HashMap::new()));
                let next = groups.len() - 1;
                active_groups.insert(face, next);
                next
            } else {
                index
            };
            let group = &mut groups[index];
            let code = if let Some(code) = group.3.get(&mapping) {
                *code
            } else {
                let code = u16::try_from(group.2.len() + 1).map_err(|_| EditError::Invalid("shaped glyph limit".into()))?;
                group.2.push((glyph.text.clone(), glyph.id));
                group.3.insert(mapping, code);
                code
            };
            codes.push((index, code));
        }
        glyph_resources.insert(key, codes);
    }
    let resources = groups
        .iter()
        .map(|(bytes, family, mappings, _)| {
            pdfcraft_fonts::embedding::embed_mapping(doc, &mut fonts, bytes, family, mappings).map_err(EditError::Invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let n = pdfcraft_content::num;
    let mut block_ops = vec![Op::new("q", vec![])];
    block_ops.extend(reveal_ops(block, lines, style, o.ctm)?);
    let back = Matrix([o.ctm[0], o.ctm[1], o.ctm[2], o.ctm[3], 0.0, 0.0])
        .invert()
        .ok_or_else(|| EditError::Invalid("the paragraph is drawn in a space it cannot be edited in".into()))?;
    let (tx, ty) = back.apply(dx, dy);
    block_ops.push(Op::new("cm", vec![n(1.0), n(0.0), n(0.0), n(1.0), n(tx), n(ty)]));
    let centre = (block.rect[0] + block.rect[2]) / 2.0;
    let lefts: Vec<_> = block.rows.iter().filter_map(|r| row_rect(r, lines).map(|r| r[0])).collect();
    let left_span = lefts.iter().copied().fold(f64::NEG_INFINITY, f64::max) - lefts.iter().copied().fold(f64::INFINITY, f64::min);
    let centered = block.rows.len() > 1
        && left_span > first.size * 0.5
        && block.rows.iter().all(|row| row_rect(row, lines).is_some_and(|r| ((r[0] + r[2]) / 2.0 - centre).abs() < first.size));
    let source_leading = block
        .rows
        .windows(2)
        .filter_map(|rows| {
            let a = rows[0].first().and_then(|i| lines.get(*i))?;
            let b = rows[1].first().and_then(|i| lines.get(*i))?;
            Some((a.origin.baseline - b.origin.baseline).abs())
        })
        .rfind(|v| *v > 0.0)
        .unwrap_or(first.size * 1.5);
    let mut baseline = first.origin.baseline;
    for (row_index, row) in rows.iter().enumerate() {
        let anchor = block.rows.get(row_index).and_then(|r| r.first()).and_then(|i| lines.get(*i));
        if style.line_spacing.is_none()
            && style.size.is_none()
            && let Some(anchor) = anchor
        {
            baseline = anchor.origin.baseline;
        }
        let anchor = anchor.unwrap_or(first);
        if kept.get(row_index).copied().unwrap_or(false) {
            // Kept rows only occur without spacing or size changes, so this is the step below.
            baseline -= source_leading;
            continue;
        }
        let w = measured.get(row_index).copied().unwrap_or(0.0);
        let row_rtl = row_ranges.get(row_index).cloned().and_then(context_for).is_some_and(|p| p.layout.rtl());
        let align = style.align.unwrap_or(if row_rtl {
            crate::added::Align::Right
        } else if centered {
            crate::added::Align::Center
        } else {
            crate::added::Align::Left
        });
        let original_rect = block.rows.get(row_index).and_then(|row| row_rect(row, lines)).unwrap_or(block.rect);
        let left = if style.width.is_none() && style.align.is_none() { original_rect[0] } else { block.rect[0] };
        let available = if style.width.is_none() && style.align.is_none() { original_rect[2] - original_rect[0] } else { width };
        let mut x = left
            + match align {
                crate::added::Align::Center => (available - w) / 2.0,
                crate::added::Align::Right => available - w,
                _ => 0.0,
            };
        let mut pos = 0;
        while pos < row.len() {
            let Some(item) = row.get(pos) else { break };
            let source = lines.get(item.1).unwrap_or(first);
            let mut end = pos + 1;
            while row.get(end).is_some_and(|other| other.1 == item.1 && other.2 == item.2) {
                end += 1;
            }
            let segment = row.get(pos..end).unwrap_or_default();
            let string: String = segment.iter().map(|c| c.0).collect();
            let m = metrics.get(&item.2).ok_or_else(|| EditError::Invalid("the paragraph font is missing".into()))?;
            let mut state = source.origin.state.clone();
            state.font = Some((item.2.clone(), size_of(source, item.1)));
            if let Some(c) = overrides.get(&item.1).and_then(|r| r.char_spacing).or(style.char_spacing) {
                state.char_spacing = c / source.origin.k.max(1e-6);
            }
            if let Some(s) = overrides.get(&item.1).and_then(|r| r.scale).or(style.scale) {
                state.scale = (s / 100.0).clamp(0.1, 10.0);
            }
            if !shaped_runs.contains_key(&(row_index, pos)) {
                state.scale *= calibration.get(&item.1).copied().unwrap_or(1.0);
            }
            if let Some([r, g, b]) = overrides.get(&item.1).and_then(|r| r.color).or(style.color) {
                state.fill = vec![Op::new("rg", vec![n(r), n(g), n(b)])];
            }
            let inverse = Matrix(o.ctm).invert().ok_or_else(|| EditError::Invalid("the paragraph has a singular transform".into()))?;
            let mut tm = Matrix(source.origin.tm).then(&Matrix(source.origin.ctm)).then(&inverse).0;
            let (ax, ay) = back.apply(x - source.origin.x, baseline - source.origin.baseline);
            tm[4] += ax;
            tm[5] += ay;
            let mut segment_width = segment_advances.get(&(row_index, pos)).copied().unwrap_or(0.0);
            if let Some((_, shaped, _)) = shaped_runs.get(&(row_index, pos)) {
                let codes = glyph_resources.get(&(row_index, pos)).ok_or_else(|| EditError::Invalid("shaped font is missing".into()))?;
                let mut properties = Dict::new();
                properties.set(b"ActualText".to_vec(), Object::String(PdfString::text(&string)));
                block_ops.push(Op::new("BDC", vec![Object::name("Span"), Object::Dict(properties)]));
                // Absolute glyph matrices preserve marks and contextual positions; font-program
                // codes are separate from Unicode and ToUnicode/ActualText keep logical text.
                let size = size_of(source, item.1);
                let mut extra = 0.0;
                for (i, glyph) in shaped.glyphs.iter().enumerate() {
                    let mut matrix = tm;
                    let x = (glyph.position[0] * size + extra) * state.scale;
                    let y = glyph.position[1] * size;
                    matrix[4] += matrix[0] * x + matrix[2] * y;
                    matrix[5] += matrix[1] * x + matrix[3] * y;
                    let (font, code) = codes.get(i).ok_or_else(|| EditError::Invalid("shaped glyph mapping is missing".into()))?;
                    let resource = resources.get(*font).ok_or_else(|| EditError::Invalid("shaped font is missing".into()))?;
                    state.font = Some((resource.clone(), size_of(source, item.1)));
                    block_ops.push(Op::new("BT", vec![]));
                    block_ops.extend(state.ops());
                    block_ops.push(Op::new("Tm", matrix.map(n).to_vec()));
                    block_ops.push(Op::new("Tj", vec![Object::String(PdfString::literal(code.to_be_bytes().to_vec()))]));
                    block_ops.push(Op::new("ET", vec![]));
                    extra += state.char_spacing * glyph.text.chars().filter(|c| !pdfcraft_fonts::embedding::default_ignorable(*c)).count() as f64
                        + state.word_spacing * glyph.text.matches(' ').count() as f64;
                }
                block_ops.push(Op::new("EMC", vec![]));
                segment_width = (shaped.advance * size + extra) * state.scale * source.origin.k;
            } else {
                block_ops.push(Op::new("BT", vec![]));
                block_ops.extend(state.ops());
                block_ops.push(Op::new("Tm", tm.iter().map(|v| n(*v)).collect()));
                block_ops.extend(source_show(
                    m,
                    &string,
                    size_of(source, item.1),
                    spaces.borrow().get(&item.2).copied().unwrap_or_else(|| source_space(doc, &fonts, m)),
                    state.char_spacing,
                    state.word_spacing,
                )?);
                block_ops.push(Op::new("ET", vec![]));
            }
            if underlines.get(&item.1).copied().or(style.underline) == Some(true) {
                let [r, g, b] = overrides.get(&item.1).and_then(|r| r.color).or(style.color).unwrap_or(source.color);
                let underline_size = size_of(source, item.1) * source.origin.k;
                let (x0, y0) = back.apply(x - o.ctm[4], baseline - o.ctm[5] - underline_size * 0.12);
                let (x1, y1) = back.apply(x + segment_width - o.ctm[4], baseline - o.ctm[5] - underline_size * 0.12);
                block_ops.push(Op::new("RG", vec![n(r), n(g), n(b)]));
                block_ops.push(Op::new("w", vec![n(underline_size * 0.06 / o.ctm[2].hypot(o.ctm[3]).max(1e-6))]));
                block_ops.push(Op::new("m", vec![n(x0), n(y0)]));
                block_ops.push(Op::new("l", vec![n(x1), n(y1)]));
                block_ops.push(Op::new("S", vec![]));
            }
            x += segment_width;
            pos = end;
        }
        baseline -= if style.line_spacing.is_none() && style.size.is_none() {
            source_leading
        } else {
            style.line_spacing.unwrap_or(1.5).clamp(0.5, 5.0) * style.size.unwrap_or(anchor.size)
        };
    }
    block_ops.push(Op::new("Q", vec![]));
    let same_scope: Vec<_> = lines.iter().filter(|line| members.iter().any(|target| line.scope == target.scope)).cloned().collect();
    let mut drop = coincident_ops(&same_scope, &members);
    drop.extend(members.iter().flat_map(|l| l.ops.iter().copied()));
    let source = shown
        .iter()
        .find(|s| lead.ops.contains(&s.op))
        .ok_or_else(|| EditError::Invalid("the paragraph has no source text-show operation".into()))?;
    let at = source.op;
    let mut patches = HashMap::new();
    for source in shown.iter().filter(|s| drop.contains(&s.op)) {
        if source.state.render_mode >= 4 {
            return Err(EditError::Invalid("coincident text used as a clipping path cannot be rewritten".into()));
        }
        patches.insert(source.op, super::shaped::source_advance(source, &ops)?);
    }
    let mut prefix = patches.remove(&at).ok_or_else(|| EditError::Invalid("the source advance is missing".into()))?;
    let advance = prefix.pop().ok_or_else(|| EditError::Invalid("the source advance is missing".into()))?;
    block_ops.insert(0, Op::new("ET", vec![]));
    block_ops.push(Op::new("BT", vec![]));
    block_ops.extend(lead.origin.state.ops());
    block_ops.extend(super::shaped::restore_text_matrix(source)?);
    prefix.extend(block_ops);
    prefix.push(advance);
    patches.insert(at, prefix);
    let data = splice(&pieces, &ops, |i| (patches.remove(&i).unwrap_or_default(), !drop.contains(&i)));
    let contents = rewritten_contents(doc, &streams, data);
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), contents);
        d.set(b"Resources".to_vec(), Object::Dict(res));
    })?;
    Ok(LineEdit { substituted })
}

#[cfg(test)]
mod range_tests {
    use super::ContextualParagraph;

    #[test]
    fn contextual_byte_ranges_reject_outside_or_reversed_input() {
        let paragraph = ContextualParagraph {
            chars: 4..6,
            bytes: vec![0, 2, 4],
            layout: pdfcraft_fonts::source_shaping::Paragraph::new("اب").unwrap(),
            face: std::sync::Arc::new(Vec::new()),
            family: String::new(),
        };
        assert_eq!(paragraph.byte_range(4..6).unwrap(), 0..4);
        for (start, end) in [(0, 1), (3, 4), (4, 7), (6, 5), (usize::MAX, usize::MAX)] {
            assert!(paragraph.byte_range(start..end).is_err());
        }
    }
}
