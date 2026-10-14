use super::{FONT, pdf, stream};
use crate::headings;

fn titles(content: &str, properties: &str, marks: &str) -> Vec<String> {
    let doc = pdf(
        &[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /Properties << {properties} >> >> >>"
            ),
            stream(content),
            FONT.into(),
            format!("<< /Type /StructTreeRoot /K << /S /H1 /Pg 3 0 R /K {marks} >> >>"),
            "<< /ActualText 8 0 R >>".into(),
            "<FEFF004E0061006D00650064>".into(),
        ],
        "",
    );
    headings(&doc).into_iter().map(|h| h.title).collect()
}

#[test]
fn marked_actualtext_replaces_all_shows_once_per_occurrence() {
    let text = "BT /F1 12 Tf /H1 << /MCID 0 >> BDC (Before ) Tj /Span << /ActualText (Replacement) >> BDC (wrong) Tj [(also) -500 (wrong)] TJ (wrong) ' 0 0 (wrong) \" EMC ( After) Tj /Span << /ActualText (Replacement) >> BDC (wrong) Tj EMC EMC ET";
    assert_eq!(titles(text, "", "0"), ["Before Replacement AfterReplacement"]);
}

#[test]
fn marked_actualtext_resolves_named_and_indirect_properties() {
    let text = "BT /F1 12 Tf /H1 << /MCID 0 >> BDC /Span /Direct BDC (bad) Tj EMC ( ) Tj /Span /Indirect BDC (bad) Tj EMC ( ) Tj /Span /Indirect BDC (bad) Tj EMC ( ) Tj /Span << /ActualText 8 0 R >> BDC (bad) Tj EMC EMC ET";
    assert_eq!(titles(text, "/Direct << /ActualText (Direct) >> /Indirect 7 0 R", "0"), ["Direct Named Named Named"]);
}

#[test]
fn marked_actualtext_outer_replacement_wins_and_empty_deletes() {
    let text = "BT /F1 12 Tf /H1 << /MCID 0 >> BDC (A) Tj /Span << /ActualText () >> BDC (bad) ' 0 0 (bad) \" 10 10 Td EMC (B ) Tj /Span << /ActualText (Outer) >> BDC /Span << /ActualText (Inner) >> BDC (bad) Tj EMC (bad) Tj EMC ( C) Tj EMC ET";
    assert_eq!(titles(text, "", "0"), ["AB Outer C"]);
}

#[test]
fn marked_actualtext_does_not_require_font_metrics_and_keeps_font_changes() {
    let text = "BT /H1 << /MCID 0 /ActualText (Accessible) >> BDC (bad) Tj /F1 12 Tf EMC /H1 << /MCID 0 >> BDC ( tail) Tj EMC ET";
    assert_eq!(titles(text, "", "0"), ["Accessible tail"]);
}

#[test]
fn marked_actualtext_without_mcid_does_not_leak_into_later_owner() {
    let text = "BT /F1 12 Tf /Span << /ActualText (No owner) >> BDC (bad) Tj /H1 << /MCID 0 >> BDC (bad) Tj EMC EMC /H1 << /MCID 0 >> BDC (Visible) Tj EMC ET";
    assert_eq!(titles(text, "", "0"), ["Visible"]);
}

#[test]
fn marked_actualtext_crossing_mcids_is_emitted_only_to_first_owner() {
    let text = "BT /F1 12 Tf /Span << /ActualText (Once) >> BDC /H1 << /MCID 0 >> BDC (bad) Tj EMC /H1 << /MCID 1 >> BDC (bad) Tj EMC EMC /H1 << /MCID 1 >> BDC (Tail) Tj EMC ET";
    assert_eq!(titles(text, "", "[0 1]"), ["Once Tail"]);
}

#[test]
fn marked_actualtext_malformed_values_and_unmatched_boundaries_are_safe() {
    let text = "EMC BT /F1 12 Tf /H1 << /MCID 0 >> BDC /Span << /ActualText 42 >> BDC (Visible) Tj EMC /Span << /ActualText ( Unclosed) >> BDC (bad) Tj (bad) Tj";
    assert_eq!(titles(text, "", "0"), ["Visible Unclosed"]);
    assert!(titles("/H1 << /MCID 0 /ActualText (No show) >> BDC EMC", "", "0").is_empty());
    assert!(titles("/H1 << /MCID 0 /ActualText (No show) >> BDC", "", "0").is_empty());
    assert_eq!(titles("/H1 << /MCID 0 /ActualText <C2> >> BDC (bad) Tj EMC", "", "0"), ["Â"]);
    assert_eq!(titles("/H1 << /MCID 0 /ActualText <4142C2> >> BDC (bad) Tj EMC", "", "0"), ["ABÂ"]);
}

#[test]
fn marked_actualtext_long_and_deep_replacements_are_bounded() {
    let replacement = "A".repeat(100_000);
    let nested = "/Span << /ActualText (ignored) >> BDC ".repeat(512);
    let text = format!("BT /H1 << /MCID 0 /ActualText ({replacement}) >> BDC {nested}(bad) Tj");
    assert_eq!(titles(&text, "", "0"), ["A".repeat(256)]);
    let unicode = "<FEFF".to_owned() + &"D83DDE00".repeat(2000) + ">";
    let text = format!("/H1 << /MCID 0 /ActualText {unicode} >> BDC (bad) Tj");
    assert_eq!(titles(&text, "", "0"), ["😀".repeat(256)]);
    let unicode = "<EFBBBF".to_owned() + &"F09F9880".repeat(2000) + ">";
    let text = format!("/H1 << /MCID 0 /ActualText {unicode} >> BDC (bad) Tj");
    assert_eq!(titles(&text, "", "0"), ["😀".repeat(256)]);
    let unicode = "<41".to_owned() + &"F09F9880".repeat(2000) + ">";
    let text = format!("/H1 << /MCID 0 /ActualText {unicode} >> BDC (bad) Tj");
    assert_eq!(titles(&text, "", "0"), ["A".to_owned() + &"😀".repeat(255)]);
    let text = "BT /H1 << /MCID 0 >> BDC /Span /Long BDC (bad) Tj EMC /Span /Long BDC (bad) Tj EMC EMC ET";
    assert_eq!(titles(text, &format!("/Long << /ActualText ({replacement}) >>"), "0"), ["A".repeat(256)]);
}
