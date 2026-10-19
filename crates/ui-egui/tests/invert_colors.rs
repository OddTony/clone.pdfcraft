//! View ▸ Invert colors: the app-wide color inversion,
//! how it survives a restart, and how documents open with it.

use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

/// One valid page (proper xref): black text on white.
fn pdf() -> Vec<u8> {
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        {
            let body = b"BT /F1 14 Tf 20 150 Td (The quick brown fox) Tj ET";
            format!("<< /Length {} >>\nstream\n{}\nendstream", body.len(), String::from_utf8_lossy(body)).into_bytes()
        },
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// A small PNG for the Create ▸ Images path.
fn png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 8, 4);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[120u8; 8 * 4 * 3]).unwrap();
    }
    out
}

#[test]
fn invert_colors_preference_round_trips() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, pdf()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(!h.state().invert_colors, "initially false");

    // Command view.invert_colors toggles it on
    h.state_mut().execute("view.invert_colors");
    h.run_steps(1);
    assert!(h.state().invert_colors, "toggled on");
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.invert_colors(id), Some(true));

    // Toggle back off
    h.state_mut().execute("view.invert_colors");
    h.run_steps(1);
    assert!(!h.state().invert_colors, "toggled off");
    assert_eq!(h.state().session.invert_colors(id), Some(false));

    // Via set_option
    h.state_mut().set_option("invert-colors", "true").unwrap();
    assert!(h.state().invert_colors);
    assert_eq!(h.state().session.invert_colors(id), Some(true));

    h.state_mut().set_option("invert_colors", "false").unwrap();
    assert!(!h.state().invert_colors);
    assert_eq!(h.state().session.invert_colors(id), Some(false));

    // Persist and restore
    h.state_mut().set_invert_colors(true);
    let json = h.state().persist();

    let mut restored = PdfCraftApp::new();
    restored.restore(&json);
    assert!(restored.invert_colors, "survives restart");

    // A settings file without the field defaults to false
    let mut fresh = PdfCraftApp::new();
    fresh.restore(r#"{"theme": "dark"}"#);
    assert!(!fresh.invert_colors);
}

#[test]
fn invert_colors_seed_on_open() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.invert_colors = true;
        app.open_bytes("doc.pdf", None, pdf()).expect("opens");
        app
    });
    h.run_steps(4);

    let id = h.state().views[0].id;
    assert_eq!(h.state().session.invert_colors(id), Some(true));

    // Changing the preference applies to all open documents
    h.state_mut().set_invert_colors(false);
    h.run_steps(1);
    assert_eq!(h.state().session.invert_colors(id), Some(false));

    h.state_mut().set_invert_colors(true);
    h.run_steps(1);
    assert_eq!(h.state().session.invert_colors(id), Some(true));

    // Revisions inherit the preference
    {
        let s = &mut h.state_mut().session;
        s.apply(id, pdfcraft_engine::Edit::SetInfo { key: "Title".into(), value: "Second".into() }).unwrap();
        let saved = s.save_bytes(id).unwrap();
        s.mark_saved(id, saved, None).unwrap();
    }
    h.state_mut().open_revision(1);
    let revision = h.state().views.last().unwrap().id;
    assert_ne!(revision, id);
    assert_eq!(h.state().session.invert_colors(revision), Some(true));

    // Signed version inherits the preference
    let len = h.state().session.get(id).unwrap().bytes.len();
    h.state_mut().view_signed_version(len, 1);
    let signed = h.state().views.last().unwrap().id;
    assert_ne!(signed, id);
    assert_eq!(h.state().session.invert_colors(signed), Some(true));

    // Created documents inherit the preference
    h.state_mut().summarize_comments();
    let summary = h.state().views.last().unwrap().id;
    assert_ne!(summary, signed);
    assert_eq!(h.state().session.invert_colors(summary), Some(true));

    h.state_mut().create_from_images(vec![("scan.png".into(), png())]);
    let created = h.state().views.last().unwrap().id;
    assert_ne!(created, summary);
    assert_eq!(h.state().session.invert_colors(created), Some(true));

    // Toggle off applies to all
    h.state_mut().set_invert_colors(false);
    h.run_steps(1);
    for view in &h.state().views {
        assert_eq!(h.state().session.invert_colors(view.id), Some(false));
    }
}
