//! Text drawn as filled glyph outlines, for generated appearances that have nowhere to keep a font.
//!
//! An annotation's appearance stream is built from its dictionary alone (`pdfcraft_annot::
//! appearance::build`), with direct resources, so it can't carry a Type 3 font (its glyph
//! procedures would have to be indirect streams). Text the standard fonts can't draw, which would
//! come out as `?`, is drawn instead as the outlines of a craft-fonts face: today Korean, with the
//! Gothic face. The words stay where they belong (`/Contents`, `/V`), so copy and search use them;
//! the outlines are only the picture. A line that needs outlines is drawn entirely from the face,
//! Latin included, so it keeps one look.

use crate::{GlyphError, ShapedScript, korean_has, shape_korean, shaped_glyph, win_ansi_encodable};

/// The face outlines are drawn from: the Korean Gothic, regular.
const SERIF: bool = false;
const BOLD: bool = false;

/// Whether `s` has a character the standard fonts can't draw (line breaks don't count), so that
/// it is drawn with [`outlined_path`] rather than in Helvetica.
pub fn needs_outlines(s: &str) -> bool {
    s.chars().any(|c| !matches!(c, '\n' | '\r') && !win_ansi_encodable(c))
}

/// The first character of `s` that neither the standard fonts nor the outline face can draw: the
/// one an appearance would still show as `?`. `None` when all of it can be drawn. Without a
/// Korean face this is [`crate::first_undrawable`].
pub fn first_unoutlinable(s: &str) -> Option<char> {
    // Korean only for now: whatever the Korean face has (Hangul, Hanja, CJK punctuation). A line
    // that needs outlines is drawn wholly from the face, so its Latin must be in the face too;
    // checked per paragraph, which holds for every line it is wrapped into.
    s.split(['\n', '\r']).filter(|para| needs_outlines(para)).find_map(|para| {
        let lacks = |c: char| c != '\t' && !korean_has(SERIF, BOLD, c);
        para.chars().find(|&c| !win_ansi_encodable(c) && lacks(c)).or_else(|| para.chars().find(|&c| lacks(c)))
    })
}

/// The advance of `s` at `size` points in the outline face, or `None` when it can't shape `s`.
pub fn outlined_width(s: &str, size: f64) -> Option<f64> {
    // A tab is a space, as in WinAnsi text.
    let clusters = shape_korean(&s.replace('\t', " "), SERIF, BOLD).ok()?;
    let w = clusters.iter().map(|c| c.advance).sum::<f64>() * size;
    w.is_finite().then_some(w)
}

/// Path construction operators (`m`, `l`, `h`) for `s` at `size` points with its baseline
/// starting at (`x`, `y`), to be filled by the caller (`f`). Inside no text object: these are
/// path operators. Errors when the face is missing or lacks a character.
pub fn outlined_path(s: &str, size: f64, x: f64, y: f64) -> Result<String, GlyphError> {
    if !(size.is_finite() && x.is_finite() && y.is_finite()) {
        return Err(GlyphError::TooComplex);
    }
    let script = ShapedScript::Korean { serif: SERIF, bold: BOLD };
    let mut out = String::new();
    let mut pen = 0.0;
    for cluster in shape_korean(&s.replace('\t', " "), SERIF, BOLD)? {
        for (id, [dx, dy]) in &cluster.glyphs {
            let glyph = shaped_glyph(script, *id)?;
            for contour in &glyph.contours {
                for (i, p) in contour.iter().enumerate() {
                    let px = x + (pen + dx + p[0]) * size;
                    let py = y + (dy + p[1]) * size;
                    out.push_str(&format!("{} {} {}\n", num(px), num(py), if i == 0 { "m" } else { "l" }));
                }
                if !contour.is_empty() {
                    out.push_str("h\n");
                }
            }
        }
        pen += cluster.advance;
    }
    Ok(out)
}

/// A coordinate with at most three decimals, without trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_text_the_standard_fonts_cannot_draw_needs_outlines() {
        assert!(!needs_outlines("Café — 5€\r\nok"));
        assert!(needs_outlines("안녕 Ada"));
        assert!(needs_outlines("签名"));
        assert_eq!(first_unoutlinable("Café\nok"), None);
    }

    #[test]
    fn korean_is_outlined_with_the_korean_face() {
        if crate::document_korean_fonts_for_style(false, false).is_empty() {
            eprintln!("skipping: built without a craft-fonts Kore face (set CRAFT_FONTS_DIR)");
            assert_eq!(first_unoutlinable("안녕 Ada"), Some('안'));
            assert_eq!(outlined_path("안녕", 12.0, 0.0, 0.0), Err(GlyphError::NoFont));
            assert!(outlined_width("안녕", 12.0).is_none());
            return;
        }
        assert_eq!(first_unoutlinable("안녕하세요, Ada! 2026년"), None);
        // Arabic is in neither the standard fonts nor the Korean face.
        assert_eq!(first_unoutlinable("안녕 \u{0627}"), Some('\u{0627}'));
        // Latin lines stay in Helvetica, so only a paragraph with Korean needs its Latin in the face.
        assert_eq!(first_unoutlinable("Ada\t€\n안녕\tAda"), None);
        assert!(outlined_path("안녕\tAda", 10.0, 0.0, 0.0).is_ok(), "a tab is drawn as a space");
        let w = outlined_width("안녕", 10.0).unwrap();
        assert!(w > 15.0 && w < 25.0, "two syllables are about two em: {w}");
        let path = outlined_path("안녕", 10.0, 100.0, 200.0).unwrap();
        assert!(path.contains(" m\n") && path.contains(" l\n") && path.ends_with("h\n"), "{path}");
        // Every point lies near the baseline origin, within the advance.
        for line in path.lines().filter(|l| l.ends_with(" m") || l.ends_with(" l")) {
            let mut it = line.split(' ');
            let (x, y): (f64, f64) = (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap());
            assert!((99.0..=100.0 + w + 1.0).contains(&x) && (197.0..=210.0).contains(&y), "{line}");
        }
        assert_eq!(outlined_path("안녕", f64::NAN, 0.0, 0.0), Err(GlyphError::TooComplex));
        assert_eq!(outlined_path("", 12.0, 0.0, 0.0), Ok(String::new()));
    }
}
