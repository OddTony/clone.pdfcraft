//! Row-level typography compatibility, independent of inline style boundaries.

use super::TextLine;

fn family(line: &TextLine) -> String {
    let mut key = pdfcraft_fonts::embedding::face_key(&line.base_font);
    for suffix in ["psmt", "mt", "bolditalic", "boldoblique", "semibold", "demibold", "regular", "italic", "oblique", "bold", "roman"] {
        if let Some(base) = key.strip_suffix(suffix) {
            key = base.to_owned();
        }
    }
    key
}

pub(super) fn compatible(previous: &[usize], next: &[usize], lines: &[TextLine]) -> bool {
    let representative =
        |row: &[usize]| row.iter().filter_map(|index| lines.get(*index)).max_by(|a, b| (a.rect[2] - a.rect[0]).total_cmp(&(b.rect[2] - b.rect[0])));
    match (representative(previous), representative(next)) {
        (Some(previous), Some(next)) => previous.size.min(next.size) >= previous.size.max(next.size) * 0.65 && family(previous) == family(next),
        _ => false,
    }
}
