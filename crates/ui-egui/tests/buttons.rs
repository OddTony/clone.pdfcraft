//! Push buttons run their actions (Reset form, Named, …) without a JavaScript engine.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

/// Two pages; page 1 has a text field "name" (filled), a Reset button, a Next page button and a
/// Print button (a JavaScript one-liner).
fn fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R] >>",
        "<< /Type /Page /Parent 2 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /Rect [20 340 280 360] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (reset) /Rect [20 300 100 320] /P 3 0 R /A << /S /ResetForm >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (next) /Rect [120 300 200 320] /P 3 0 R /A << /S /Named /N /NextPage >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (print) /Rect [220 300 280 320] /P 3 0 R /AA << /U << /S /JavaScript /JS (this.print\\({bUI: true}\\);) >> >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (save) /Rect [20 260 100 280] /P 3 0 R /A << /S /Named /N /Save >> >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn click_field(h: &mut Harness<'static, PdfCraftApp>, name: &str) {
    let p = {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        let f = doc.form.iter().find(|f| f.name == name).unwrap();
        pdfcraft_ui_egui::forms_ui::field_screen_rect(&s.views[0], &doc.info, f, 0).expect("on screen").center()
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
}

#[test]
fn buttons_reset_navigate_and_print() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-buttons-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("buttons.pdf");
    std::fs::write(&path, fixture()).unwrap();
    let path_str = path.to_string_lossy().into_owned();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("buttons.pdf", Some(path_str), fixture()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    click_field(&mut h, "reset");
    {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        assert!(doc.form.iter().find(|f| f.name == "name").unwrap().value.is_empty(), "Reset form cleared the field");
    }
    click_field(&mut h, "print");
    assert_eq!(h.state().dialog, Some(Dialog::Print), "this.print() opens the Print dialog");
    h.state_mut().dialog = None;
    h.run_steps(2);
    // A document's Save named action never writes the file unasked (PdfCraft's XFA buttons
    // use SaveAs, which opens the Save As dialog, as a script's `execMenuItem` does): the
    // reset above made the document dirty, and the click leaves it so.
    let before = std::fs::read(&path).unwrap();
    click_field(&mut h, "save");
    h.run_steps(4);
    h.get_by_label_contains("isn't supported");
    assert!(h.state().session.get(h.state().views[0].id).unwrap().dirty, "not saved");
    assert_eq!(std::fs::read(&path).unwrap(), before, "the file is untouched");
    click_field(&mut h, "next");
    h.run_steps(4);
    assert_eq!(h.state().views[0].current, 1, "the NextPage action");
}

/// One page: a text field "name" and two push buttons whose Hide actions hide it and show it again.
fn show_hide_fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R 6 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /F 4 /Rect [20 340 280 360] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (hide) /Rect [20 300 100 320] /P 3 0 R /A << /S /Hide /T (name) >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (show) /Rect [120 300 200 320] /P 3 0 R /A << /S /Hide /T 4 0 R /H false >> >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// Whether the "name" widget (object 4) has the Hidden annotation flag in the saved document.
fn name_hidden(h: &Harness<'static, PdfCraftApp>) -> bool {
    let s = h.state();
    let bytes = s.session.save_bytes(s.views[0].id).unwrap();
    let doc = pdfcraft_cos::Document::open(bytes).unwrap();
    let f = doc.get(pdfcraft_cos::ObjRef::new(4, 0)).as_dict().and_then(|d| d.int(b"F")).unwrap_or(0);
    f & 2 != 0
}

#[test]
fn hide_actions_hide_and_show_fields() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("show-hide.pdf", None, show_hide_fixture()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(!name_hidden(&h));
    // A push button with an action shows the pointing hand, not the "not allowed" cursor.
    let hide_at = {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        let f = doc.form.iter().find(|f| f.name == "hide").unwrap();
        pdfcraft_ui_egui::forms_ui::field_screen_rect(&s.views[0], &doc.info, f, 0).expect("on screen").center()
    };
    h.hover_at(hide_at);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
    click_field(&mut h, "hide");
    assert!(name_hidden(&h), "the Hide action hid the field");
    // A hidden field takes no clicks: clicking where it was doesn't start editing it.
    click_field(&mut h, "name");
    assert!(h.state().views[0].forms.focus.is_none(), "a hidden field can't be focused");
    click_field(&mut h, "show");
    assert!(!name_hidden(&h), "the Hide action with /H false showed it again");
}

/// One page: a hidden text field "name" (object 4) and a radio group "married" whose Yes button
/// shows it and whose No button hides it, each button with its own action.
fn radio_show_hide_fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 6 0 R 7 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /F 6 /Rect [20 340 280 360] /P 3 0 R >>",
        "<< /FT /Btn /T (married) /Ff 49152 /V /Off /Kids [6 0 R 7 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /AS /Off /F 4 /Rect [20 290 50 320] /P 3 0 R /AP << /N << /Yes 8 0 R /Off 8 0 R >> >> /A << /S /Hide /T (name) /H false >> >>",
        "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /AS /Off /F 4 /Rect [80 290 110 320] /P 3 0 R /AP << /N << /No 8 0 R /Off 8 0 R >> >> /A << /S /Hide /T (name) >> >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[test]
fn radio_buttons_run_their_own_show_hide_actions() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("radio-show-hide.pdf", None, radio_show_hide_fixture()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(name_hidden(&h));
    let click_widget = |h: &mut Harness<'static, PdfCraftApp>, widget: usize| {
        let p = {
            let s = h.state();
            let doc = s.session.get(s.views[0].id).unwrap();
            let f = doc.form.iter().find(|f| f.name == "married").unwrap();
            pdfcraft_ui_egui::forms_ui::field_screen_rect(&s.views[0], &doc.info, f, widget).expect("on screen").center()
        };
        h.hover_at(p);
        h.run_steps(1);
        h.drag_at(p);
        h.run_steps(1);
        h.drop_at(p);
        h.run_steps(4);
    };
    let married = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().form.iter().find(|f| f.name == "married").unwrap().value.clone()
    };
    click_widget(&mut h, 0);
    assert_eq!(married(&h), ["Yes"], "the click selected the button");
    assert!(!name_hidden(&h), "and its action showed the field");
    click_widget(&mut h, 1);
    assert_eq!(married(&h), ["No"]);
    assert!(name_hidden(&h), "the other button's action hid it again");
    // Undo takes back the action, then the selection.
    h.state_mut().run_command("edit.undo");
    h.run_steps(3);
    assert!(!name_hidden(&h));
    assert_eq!(married(&h), ["No"]);
}

/// A PDF of `objs` (numbered from 1) with a proper xref table.
fn pdf(objs: &[&str]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn shell(bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, bytes).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    h
}

/// Click the centre of `name`'s widget `widget`.
fn click_widget(h: &mut Harness<'static, PdfCraftApp>, name: &str, widget: usize) {
    let p = {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        let f = doc.form.iter().find(|f| f.name == name).unwrap();
        pdfcraft_ui_egui::forms_ui::field_screen_rect(&s.views[0], &doc.info, f, widget).expect("on screen").center()
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
}

fn value(h: &Harness<'static, PdfCraftApp>, name: &str) -> Vec<String> {
    let s = h.state();
    s.session.get(s.views[0].id).unwrap().form.iter().find(|f| f.name == name).unwrap().value.clone()
}

#[test]
fn clicks_go_to_the_topmost_field_that_responds() {
    // Radio button A sits inside a read-only label drawn over it; radio button B sits inside an
    // editable text field drawn under it; the check box sits under an editable text field drawn
    // over it. /Annots order is drawing order: [A label note B box top].
    let h = &mut shell(pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 8 0 R 9 0 R 10 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 420 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [6 0 R 4 0 R 8 0 R 7 0 R 9 0 R 10 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (label) /Ff 1 /V (Married?) /F 4 /Rect [20 300 200 330] /P 3 0 R >>",
        "<< /FT /Btn /T (pick) /Ff 49152 /V /Off /Kids [6 0 R 7 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /AS /Off /F 4 /Rect [30 305 50 325] /P 3 0 R /AP << /N << /A 11 0 R /Off 11 0 R >> >> >>",
        "<< /Type /Annot /Subtype /Widget /Parent 5 0 R /AS /Off /F 4 /Rect [230 305 250 325] /P 3 0 R /AP << /N << /B 11 0 R /Off 11 0 R >> >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (note) /F 4 /Rect [220 300 400 330] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (box) /V /Off /AS /Off /F 4 /Rect [30 205 50 225] /P 3 0 R /AP << /N << /Yes 11 0 R /Off 11 0 R >> >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (top) /F 4 /Rect [20 200 200 230] /P 3 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ]));
    click_widget(h, "pick", 0);
    assert_eq!(value(h, "pick"), ["A"], "a read-only label over the button lets the click through");
    click_widget(h, "pick", 1);
    assert_eq!(value(h, "pick"), ["B"], "the button drawn over the text field takes the click");
    assert!(h.state().views[0].forms.focus.is_none(), "and the text field under it isn't edited");
    click_widget(h, "box", 0);
    assert!(value(h, "box").is_empty(), "the text field drawn over the check box takes the click");
    assert_eq!(h.state().views[0].forms.focus.as_ref().map(|f| f.name.as_str()), Some("top"));
}

fn png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 8, 4);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[200u8; 8 * 4 * 3]).unwrap();
    }
    out
}

#[test]
fn an_image_field_takes_its_picture_after_typing_in_another_field() {
    let h = &mut shell(pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /F 4 /Rect [20 340 280 360] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (photo) /F 4 /Rect [20 150 280 320] /P 3 0 R /A << /S /JavaScript /JS (event.target.buttonImportIcon();) >> >>",
    ]));
    let dir = std::env::temp_dir().join(format!("pdfcraft-image-field-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let picture = dir.join("photo.png");
    std::fs::write(&picture, png()).unwrap();
    // Type in a field, then click the image field while that field is still being edited.
    click_widget(h, "name", 0);
    h.state_mut().views[0].forms.focus.as_mut().unwrap().text = "Ada".into();
    h.state_mut().pick_override = Some(vec![picture.to_string_lossy().into_owned()]);
    click_widget(h, "photo", 0);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.form.iter().find(|f| f.name == "name").unwrap().value, ["Ada"], "the typing was kept");
    assert_eq!(doc.can_undo(), Some("Set the image of photo"), "and the picture went in: {:?}", s.toast);
    let _ = std::fs::remove_dir_all(&dir);
}
