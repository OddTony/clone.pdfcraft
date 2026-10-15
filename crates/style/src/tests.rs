use super::*;

fn sample(text: &str, size: f64, bold: bool, rect: [f64; 4]) -> BlockSample {
    BlockSample { text: text.into(), size, bold, rect, base_font: "Helvetica".into() }
}

#[test]
fn body_and_headings_cluster() {
    let blocks = vec![
        sample("Title of the Document", 18.0, true, [72.0, 700.0, 400.0, 720.0]),
        sample("A section heading here", 14.0, true, [72.0, 660.0, 400.0, 676.0]),
        sample("This is a long body paragraph with enough words to count as real text for the median.", 11.0, false, [72.0, 600.0, 540.0, 640.0]),
        sample("Another long body paragraph with enough words to count as real text for the median.", 11.0, false, [72.0, 560.0, 540.0, 596.0]),
    ];
    let profile = extract([612.0, 792.0], &blocks).expect("extract");
    assert!((profile.body_size - 11.0).abs() < 0.01);
    assert_eq!(profile.headings.len(), 2);
    assert_eq!(profile.headings[0].level, 1);
    assert!((profile.headings[0].size - 18.0).abs() < 0.6);
}

#[test]
fn empty_reference_gives_defaults() {
    let profile = extract([612.0, 792.0], &[]).expect("extract");
    assert_eq!(profile, StyleProfile::default());
}

#[test]
fn bad_page_is_refused() {
    assert!(extract([f64::NAN, 792.0], &[]).is_err());
    assert!(extract([0.5, 792.0], &[]).is_err());
}

#[test]
fn plain_markers_parse() {
    let items = parse_plain("# Title\n\nBody text here\n\n- a bullet\n\n1. numbered").expect("parse");
    assert_eq!(items.len(), 4);
    assert!(matches!(items[0], StyledItem::Heading { level: 1, .. }));
    assert!(matches!(items[2], StyledItem::Bullet(_)));
}

#[test]
fn styled_document_renders_pages() {
    let profile = StyleProfile::default();
    let items = vec![
        StyledItem::Heading { level: 1, text: "Title".into() },
        StyledItem::Body("A body paragraph with some words in it.".into()),
        StyledItem::Bullet("A bullet point".into()),
    ];
    let doc = render("Test", &items, &profile).expect("render");
    let bytes = pdfcraft_cos::write_full(&doc, &pdfcraft_cos::SaveOptions::default()).expect("write");
    assert!(bytes.len() > 500);
}

#[test]
fn undrawable_text_is_refused() {
    let profile = StyleProfile::default();
    let items = vec![StyledItem::Body("日本語".into())];
    assert!(render("Test", &items, &profile).is_err());
}
