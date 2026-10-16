//! Metadata comparison regressions for PR #2598 comment 6213443550.
use super::*;

fn packet(payload: &str, attributes: &str) -> String {
    format!(
        "<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description rdf:about='' xmlns:z='https://example.org/ns/' {attributes}>{payload}</rdf:Description></rdf:RDF></x:xmpmeta>"
    )
}

fn collected(src: &[u8]) -> Vec<String> {
    let mut doc = one_page(src, "", vec![]);
    let page = pdfcraft_model::pages(&doc).swap_remove(0);
    let (_, data) = page_streams(&doc, &page.dict, 0).unwrap();
    let res = page.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let mut rep = Report::default();
    let rects = [[-1_000_000.0, -1_000_000.0, 1_000_000.0, 1_000_000.0]];
    let mut scope = Scope::new(&rects, Mode::Apply, &mut rep);
    process(&mut doc, &mut scope, &data, &res, pdfcraft_content::Matrix::IDENTITY);
    scope.flush_run();
    scope.removed_fragments
}

fn redact_all(src: &[u8], key: &str, value: &str, packet: Option<&str>) -> Document {
    let mut doc = meta_doc(src, &[(key, value), ("Subject", "Spy stories")], packet, None);
    let rects = [[0.0, 0.0, 300.0, 300.0]];
    assert!(under(&mut doc, 0, &rects) > 0, "fixture has marked glyphs");
    mark(&mut doc, 0, &rects, "");
    apply(&mut doc, None).unwrap();
    assert_eq!(under(&mut doc, 0, &rects), 0, "page content was removed");
    let saved = Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap();
    assert_eq!(info_value(&saved, "Subject").as_deref(), Some("Spy stories"));
    saved
}

#[test]
fn tj_word_gaps_and_literal_spaces_clear_the_same_name() {
    for (label, src, fragment, survives) in [
        ("TJ -333", "BT /F1 10 Tf 10 100 Td [(James)-333(Bond)]TJ ET", "JamesBond", false),
        ("mixed kerns", "BT /F1 10 Tf 10 100 Td [(J)-200(ames)-333(Bond)]TJ ET", "JamesBond", false),
        ("rotated", "BT /F1 10 Tf 0 1 -1 0 100 100 Tm [(James)-333(Bond)]TJ ET", "JamesBond", false),
        ("literal space", "BT /F1 10 Tf 10 100 Td (James Bond)Tj ET", "James Bond", false),
    ] {
        assert_eq!(collected(src.as_bytes()), [fragment], "{label}");
        let packet = packet("<z:name>James Bond</z:name>", "");
        let saved = redact_all(src.as_bytes(), "Author", "James Bond", Some(&packet));
        let actual = info_value(&saved, "Author");
        assert_eq!(actual.as_deref(), survives.then_some("James Bond"), "the removed name must not survive");
        assert_eq!(xmp_text(&saved).is_some(), survives);
    }
}

#[test]
fn metadata_whitespace_is_normalized_without_broadening_name_matching() {
    for (label, text, key, value) in [
        ("partial phrase", "JAMES BOND IS A SPY", "Title", "Case: James Bond"),
        ("partial first name", "JAMES BOND IS A SPY", "Title", "Case: James"),
        ("reordered", "John Smith", "Author", "Smith, John"),
        ("double space metadata", "James Bond", "Author", "James  Bond"),
        ("tab metadata", "James Bond", "Author", "James\tBond"),
        ("newline metadata", "James Bond", "Author", "James\nBond"),
        ("double space page", "James  Bond", "Author", "James Bond"),
    ] {
        let src = format!("BT /F1 10 Tf 10 100 Td ({text})Tj ET");
        let packet = packet(&format!("<z:value>{value}</z:value>"), "");
        let saved = redact_all(src.as_bytes(), key, value, Some(&packet));
        let actual = info_value(&saved, key);
        let unsupported = label.starts_with("partial") || label == "reordered";
        assert_eq!(actual.as_deref(), unsupported.then_some(value), "whitespace normalizes; partial and reordered names remain unsupported");
        assert_eq!(xmp_text(&saved).as_deref(), unsupported.then_some(packet.as_str()));
    }
}

#[test]
fn xmp_matches_values_and_preserves_structure_only_matches() {
    for (label, text, payload, attributes, removed) in [
        ("tag name", "DC", "<z:dc>Harmless</z:dc>", "", false),
        ("attribute name", "ID", "<z:item>Harmless</z:item>", "z:id='safe'", false),
        ("short language value", "AGENT", "<z:lang>en</z:lang>", "", true),
        ("short version value", "RELEASE 1.7 APPROVED", "<z:version>1.7</z:version>", "", true),
        ("unrelated control", "ZX", "<z:item>Harmless</z:item>", "z:id='safe'", false),
    ] {
        let packet = packet(payload, attributes);
        let src = format!("BT /F1 10 Tf 10 100 Td ({text})Tj ET");
        if label.starts_with("short") {
            assert!(!packet.to_lowercase().contains(&text.to_lowercase()), "the whole removed run is absent");
        }
        let saved = redact_all(src.as_bytes(), "Author", "Ada", Some(&packet));
        let actual = xmp_text(&saved);
        assert_eq!(actual.is_none(), removed, "{label}");
        assert_eq!(info_value(&saved, "Author").as_deref(), Some("Ada"));
    }
}

#[test]
fn xmp_values_are_decoded_and_markup_is_not_a_value() {
    for (payload, attributes, removed) in [
        ("<z:name>&#74;ames&#32;Bond</z:name>", "", true),
        ("<z:name><![CDATA[James Bond]]></z:name>", "", true),
        ("<z:clean>Harmless</z:clean>", "z:name = 'James  Bond'", true),
        ("<!--James Bond--><z:clean>Harmless</z:clean>", "", true),
        ("<z:JamesBond/>", "", false),
    ] {
        let packet = packet(payload, attributes);
        let text = if removed { "James Bond" } else { "JamesBond" };
        let src = format!("BT /F1 10 Tf 10 100 Td ({text})Tj ET");
        let saved = redact_all(src.as_bytes(), "Author", "Ada", Some(&packet));
        assert_eq!(xmp_text(&saved).is_none(), removed);
    }
    let packet = packet("<z:clean>Harmless</z:clean>", "xmlns:dc='https://example.org/dc/' z:id = 'safe > tail'");
    for text in ["DC", "ID"] {
        let src = format!("BT /F1 10 Tf 10 100 Td ({text})Tj ET");
        assert!(xmp_text(&redact_all(src.as_bytes(), "Author", "Ada", Some(&packet))).is_some());
    }
    for broken in ["<broken>", "<!DOCTYPE x [<!ENTITY secret 'James Bond'>]><x>&secret;</x>"] {
        assert!(xmp_text(&redact_all(b"BT /F1 10 Tf 10 100 Td (James Bond)Tj ET", "Author", "Ada", Some(broken))).is_none());
    }
}

#[test]
fn optional_space_offsets_survive_unicode_lowercase_expansion() {
    let pattern = KernedFragment { text: "İB".into(), gaps: vec![2] }.normalized();
    assert!(pattern.matches(&normalize("İ B")));
    assert!(pattern.matches(&normalize("İB")));
    assert!(!pattern.matches(&normalize("B İ")));
    assert!(pattern.matches(&normalize("Case: İ B, unrelated")));
}

#[test]
fn optional_tj_spaces_preserve_literal_boundaries_and_complete_value_matching() {
    let src = b"BT /F1 10 Tf 10 100 Td [(J)-200(ames)-333(Bond) ( IS A SPY)]TJ ET";
    for (value, removed) in [("James Bond", true), ("JamesBond", true), ("Case: James Bond", false), ("Spy stories", false)] {
        let saved = redact_all(src, "Title", value, None);
        assert_eq!(info_value(&saved, "Title").is_none(), removed, "{value}");
    }
    // Literal spaces must never become optional merely because the run also contains a kern.
    let saved = redact_all(b"BT /F1 10 Tf 10 100 Td [(James Bond)-200(X)]TJ ET", "Author", "JamesBondX", None);
    assert_eq!(info_value(&saved, "Author").as_deref(), Some("JamesBondX"));
    for kern in [0, 333] {
        let src = format!("BT /F1 10 Tf 10 100 Td [(James){kern}(Bond)]TJ ET");
        let saved = redact_all(src.as_bytes(), "Author", "James Bond", None);
        assert_eq!(info_value(&saved, "Author").as_deref(), Some("James Bond"), "zero/backward kern {kern} is not a word gap");
    }
}
