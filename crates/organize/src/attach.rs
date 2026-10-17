//! Document-level file attachments (ISO 32000-2 §7.11.4, §7.7.4): the catalog's
//! `/Names /EmbeddedFiles` name tree, as Edit a PDF ▸ Attach file writes it.
//!
//! A new file gets a `/Filespec` (`/F`, `/UF`, `/EF`, `/Desc`, `/AFRelationship`) and an
//! `/EmbeddedFile` stream (`/Subtype` MIME, `/Params` `/Size` `/ModDate` `/CheckSum` MD5). It enters
//! the tree at its sorted place; an existing tree keeps its shape (`/Kids` take the key in the leaf
//! whose range covers it, `/Limits` on the way are recomputed).
use md5::{Digest as _, Md5};
use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};

use crate::OrganizeError;

const MAX_DEPTH: u8 = 32;

/// MIME type from a file name's extension; `application/octet-stream` when unknown.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "txt" | "log" | "lock" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "json" => "application/json",
        "toml" => "application/toml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Embed `data` as `name` (folder part dropped) in the catalog's `/Names /EmbeddedFiles` tree. A name
/// already in the tree gets " (2)", " (3)"… appended. `mod_date` is a PDF date for `/Params /ModDate`.
/// Returns the name it was filed under.
pub fn attach_file(doc: &mut Document, name: &str, data: &[u8], description: &str, mod_date: Option<&str>) -> Result<String, OrganizeError> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    if base.is_empty() {
        return Err(OrganizeError::Invalid("the attached file needs a name".into()));
    }
    let root = doc.root().ok_or(OrganizeError::NoPageTree)?;
    let tree = embedded_files(doc, root);
    let mut taken = Vec::new();
    if let Some((_, node)) = &tree {
        collect_keys(doc, node, 0, &mut taken);
    }
    let mut final_name = base.to_string();
    let mut n = 2;
    while taken.iter().any(|k| PdfString::literal(k.as_slice()).to_text() == final_name) {
        final_name = format!("{base} ({n})");
        n += 1;
    }
    let key = PdfString::text(&final_name);

    let mut params = Dict::new();
    params.set(b"Size".to_vec(), Object::Int(i64::try_from(data.len()).unwrap_or(i64::MAX)));
    if let Some(date) = mod_date {
        params.set(b"ModDate".to_vec(), PdfString::literal(date.as_bytes().to_vec()));
    }
    params.set(b"CheckSum".to_vec(), Object::String(PdfString::literal(Md5::digest(data).to_vec())));
    let mut ef = Dict::new();
    ef.set(b"Type".to_vec(), Object::name("EmbeddedFile"));
    ef.set(b"Subtype".to_vec(), Object::name(mime_for(&final_name)));
    ef.set(b"Params".to_vec(), Object::Dict(params));
    let ef = doc.add(Object::Stream(Stream::flate(ef, data)));
    let mut efd = Dict::new();
    efd.set(b"F".to_vec(), Object::Ref(ef));
    efd.set(b"UF".to_vec(), Object::Ref(ef));
    let mut fs = Dict::new();
    fs.set(b"Type".to_vec(), Object::name("Filespec"));
    fs.set(b"F".to_vec(), Object::String(PdfString::text(&final_name)));
    fs.set(b"UF".to_vec(), Object::String(PdfString::text(&final_name)));
    fs.set(b"EF".to_vec(), Object::Dict(efd));
    if !description.trim().is_empty() {
        fs.set(b"Desc".to_vec(), Object::String(PdfString::text(description.trim())));
    }
    fs.set(b"AFRelationship".to_vec(), Object::name("Unspecified"));
    let fs = doc.add(Object::Dict(fs));

    match tree {
        None => {
            let mut leaf = Dict::new();
            leaf.set(b"Names".to_vec(), Object::Array(vec![Object::String(key), Object::Ref(fs)]));
            let leaf = doc.add(Object::Dict(leaf));
            set_embedded_files(doc, root, Some(Object::Ref(leaf)))?;
        }
        Some((place, mut node)) => {
            insert(doc, &mut node, key, Object::Ref(fs), true, 0)?;
            match place {
                Place::Ref(r) => doc.set(r, Object::Dict(node)),
                Place::Inline => set_embedded_files(doc, root, Some(Object::Dict(node)))?,
            }
        }
    }
    Ok(final_name)
}

/// Remove the document-level attachment filed under `name`. Emptied leaves and an emptied tree go
/// with it; `/Limits` are recomputed. Error when there is no such attachment.
pub fn detach_file(doc: &mut Document, name: &str) -> Result<(), OrganizeError> {
    let missing = || OrganizeError::Invalid(format!("the document has no attached file named {name}"));
    let root = doc.root().ok_or(OrganizeError::NoPageTree)?;
    let Some((place, mut node)) = embedded_files(doc, root) else { return Err(missing()) };
    if !remove(doc, &mut node, name, true, 0)? {
        return Err(missing());
    }
    if is_empty(doc, &node) {
        set_embedded_files(doc, root, None)?;
    } else {
        match place {
            Place::Ref(r) => doc.set(r, Object::Dict(node)),
            Place::Inline => set_embedded_files(doc, root, Some(Object::Dict(node)))?,
        }
    }
    Ok(())
}

enum Place {
    Ref(ObjRef),
    Inline,
}

fn embedded_files(doc: &Document, root: ObjRef) -> Option<(Place, Dict)> {
    let catalog = doc.get(root).as_dict().cloned()?;
    let names = doc.dict(catalog.get(b"Names")?)?;
    let ef = names.get(b"EmbeddedFiles")?;
    let node = doc.dict(ef)?;
    Some((if let Object::Ref(r) = ef { Place::Ref(*r) } else { Place::Inline }, node))
}

/// Set or (with `None`) remove the catalog's `/Names /EmbeddedFiles`, keeping the other name trees.
fn set_embedded_files(doc: &mut Document, root: ObjRef, value: Option<Object>) -> Result<(), OrganizeError> {
    let catalog = doc.get(root).as_dict().cloned().ok_or(OrganizeError::NoPageTree)?;
    let apply = |names: &mut Dict| match &value {
        Some(v) => names.set(b"EmbeddedFiles".to_vec(), v.clone()),
        None => {
            names.remove(b"EmbeddedFiles");
        }
    };
    match catalog.get(b"Names") {
        Some(Object::Ref(r)) => {
            let r = *r;
            let mut names = doc.get(r).as_dict().cloned().unwrap_or_default();
            apply(&mut names);
            doc.set(r, Object::Dict(names));
        }
        other => {
            let mut names = match other {
                Some(Object::Dict(d)) => d.clone(),
                _ => Dict::new(),
            };
            apply(&mut names);
            doc.update_dict(root, |c| {
                if names.is_empty() {
                    c.remove(b"Names");
                } else {
                    c.set(b"Names".to_vec(), Object::Dict(names));
                }
            })?;
        }
    }
    Ok(())
}

fn key_of(doc: &Document, o: &Object) -> Option<Vec<u8>> {
    doc.resolve(o).as_string().map(|s| s.bytes.clone())
}

fn pairs(doc: &Document, node: &Dict) -> Vec<Object> {
    node.get(b"Names").map(|n| doc.resolve(n)).and_then(|n| n.as_array().cloned()).unwrap_or_default()
}

fn kids(doc: &Document, node: &Dict) -> Vec<Object> {
    node.get(b"Kids").map(|n| doc.resolve(n)).and_then(|n| n.as_array().cloned()).unwrap_or_default()
}

fn is_empty(doc: &Document, node: &Dict) -> bool {
    pairs(doc, node).is_empty() && kids(doc, node).is_empty()
}

fn collect_keys(doc: &Document, node: &Dict, depth: u8, out: &mut Vec<Vec<u8>>) {
    if depth > MAX_DEPTH {
        return;
    }
    out.extend(pairs(doc, node).chunks(2).filter_map(|p| p.first().and_then(|k| key_of(doc, k))));
    for k in kids(doc, node) {
        if let Some(d) = doc.dict(&k) {
            collect_keys(doc, &d, depth + 1, out);
        }
    }
}

/// Least and greatest key under a node, from its content (not trusting stale `/Limits`).
fn range(doc: &Document, node: &Dict, depth: u8) -> Option<(Vec<u8>, Vec<u8>)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut keys: Vec<Vec<u8>> = pairs(doc, node).chunks(2).filter_map(|p| p.first().and_then(|k| key_of(doc, k))).collect();
    for k in kids(doc, node) {
        if let Some((lo, hi)) = doc.dict(&k).and_then(|d| range(doc, &d, depth + 1)) {
            keys.push(lo);
            keys.push(hi);
        }
    }
    Some((keys.iter().min()?.clone(), keys.iter().max()?.clone()))
}

fn refresh_limits(doc: &Document, node: &mut Dict) {
    node.remove(b"Limits");
    if let Some((lo, hi)) = range(doc, node, 0) {
        node.set(b"Limits".to_vec(), Object::Array(vec![Object::String(PdfString::literal(lo)), Object::String(PdfString::literal(hi))]));
    }
}

/// File `(key, value)` under `node`: its own `/Names` when a leaf, else the first kid reaching the
/// key (the last kid for a key beyond them all).
fn insert(doc: &mut Document, node: &mut Dict, key: PdfString, value: Object, is_root: bool, depth: u8) -> Result<(), OrganizeError> {
    if depth > MAX_DEPTH {
        return Err(OrganizeError::Invalid("the attachment tree is nested too deeply".into()));
    }
    let children = kids(doc, node);
    if children.is_empty() {
        let mut list = pairs(doc, node);
        let even = list.len() - list.len() % 2;
        let at = list.chunks(2).position(|p| p.first().and_then(|k| key_of(doc, k)).is_some_and(|k| k > key.bytes)).map_or(even, |i| i * 2);
        list.splice(at..at, [Object::String(key), value]);
        node.set(b"Names".to_vec(), Object::Array(list));
    } else {
        let mut chosen = 0;
        for (i, k) in children.iter().enumerate() {
            chosen = i;
            if doc.dict(k).and_then(|d| range(doc, &d, 0)).is_some_and(|(_, hi)| hi >= key.bytes) {
                break;
            }
        }
        let Some(Object::Ref(r)) = children.get(chosen) else {
            return Err(OrganizeError::Invalid("the attachment tree has a child that is not an indirect object".into()));
        };
        let r = *r;
        let mut kid = doc.get(r).as_dict().cloned().ok_or_else(|| OrganizeError::Invalid("the attachment tree has a damaged child".into()))?;
        insert(doc, &mut kid, key, value, false, depth + 1)?;
        doc.set(r, Object::Dict(kid));
    }
    if !is_root {
        refresh_limits(doc, node);
    }
    Ok(())
}

fn remove(doc: &mut Document, node: &mut Dict, name: &str, is_root: bool, depth: u8) -> Result<bool, OrganizeError> {
    if depth > MAX_DEPTH {
        return Ok(false);
    }
    let mut found = false;
    let mut list = pairs(doc, node);
    if let Some(i) = list.chunks(2).position(|p| p.first().and_then(|k| key_of(doc, k)).is_some_and(|k| PdfString::literal(k).to_text() == name)) {
        list.drain(i * 2..(i * 2 + 2).min(list.len()));
        node.set(b"Names".to_vec(), Object::Array(list));
        found = true;
    }
    if !found {
        let mut children = kids(doc, node);
        for i in 0..children.len() {
            let Some(Object::Ref(r)) = children.get(i) else { continue };
            let r = *r;
            let Some(mut kid) = doc.get(r).as_dict().cloned() else { continue };
            if remove(doc, &mut kid, name, false, depth + 1)? {
                if is_empty(doc, &kid) {
                    children.remove(i);
                    doc.free(r);
                } else {
                    doc.set(r, Object::Dict(kid));
                }
                node.set(b"Kids".to_vec(), Object::Array(children));
                found = true;
                break;
            }
        }
    }
    if found && !is_root {
        refresh_limits(doc, node);
    }
    Ok(found)
}
