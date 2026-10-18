//! Read-only interaction benchmark; never saves or changes the input file.
//! cargo run -p pdfcraft-engine --example profile_form -- PDF FIELD VALUE1 VALUE2
//! Checkbox values are true/false. Alternates values for six measured edits.

use std::{error::Error, io::Write, path::Path, sync::Arc, time::Instant};

use pdfcraft_engine::{Edit, Session};
use pdfcraft_forms::{FieldKind, FieldValue};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, name, first, second] = args.as_slice() else {
        return Err("usage: profile_form PDF FIELD VALUE1 VALUE2".into());
    };
    let bytes = Arc::new(std::fs::read(path)?);
    let mut session = Session::new();
    let started = Instant::now();
    let file_name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or("benchmark.pdf");
    let id = session.open(file_name, None, bytes, None)?;
    writeln!(std::io::stderr().lock(), "open: {:.3} ms", started.elapsed().as_secs_f64() * 1000.0)?;
    let kind = session.get(id).and_then(|d| d.form.iter().find(|f| f.name == *name)).map(|f| f.kind).ok_or("field not found")?;
    for i in 0..6 {
        let text = if i % 2 == 0 { first } else { second };
        let value = match kind {
            FieldKind::CheckBox => FieldValue::Check(text.parse()?),
            FieldKind::Text => FieldValue::Text(text.clone()),
            _ => return Err("benchmark supports text fields and checkboxes".into()),
        };
        let started = Instant::now();
        session.apply(id, Edit::SetFieldValue { name: name.clone(), value })?;
        let edit_ms = started.elapsed().as_secs_f64() * 1000.0;
        let output = session.take_js_output(id);
        if !output.errors.is_empty() {
            return Err(format!("script errors: {:?}", output.errors).into());
        }
        let doc = session.get(id).ok_or("document closed")?;
        let page = doc.form.iter().find(|f| f.name == *name).and_then(|f| f.widgets.first()).and_then(|w| w.page).unwrap_or(0);
        let started = Instant::now();
        let config = pdfcraft_render::RenderConfig { password: doc.password.as_deref().map(Arc::from), ..Default::default() };
        let mut renderer = pdfcraft_render::PageRenderer::new(doc.display.clone(), config);
        let rendered = renderer.render(pdfcraft_render::RenderRequest { page, scale: 1.0, ..Default::default() });
        if let Some(error) = rendered.error {
            return Err(error.into());
        }
        writeln!(
            std::io::stderr().lock(),
            "sample {i}: edit {edit_ms:.3} ms, renderer setup + page {:.3} ms (raster {} ms)",
            started.elapsed().as_secs_f64() * 1000.0,
            rendered.millis
        )?;
    }
    Ok(())
}
