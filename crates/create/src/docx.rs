//! Read the text (with structure) back out of a `.docx` file (L4).
//!
//! The reverse of `export::docx` for the Format Studio raw side: paragraphs
//! become marked text (`# ` headings, `- ` bullets, tables as tab-joined
//! rows), so an unformatted Word file feeds the same pipeline as pasted text.
//! Only `word/document.xml` is read; headers, footers, images, drawings and
//! tracked deletions are left out. Run-level bold and italic are flattened:
//! the pipeline restyles everything from the reference anyway.

use std::io::Read;

use quick_xml::{Reader, events::Event};

use crate::CreateError;

/// A `.docx` file is a zip archive this large at most.
const MAX_FILE_BYTES: u64 = 256 << 20;
/// `word/document.xml`, inflated, this large at most.
const MAX_PART_BYTES: u64 = 64 << 20;
/// Zip entries (files) this many at most.
const MAX_PARTS: usize = 10_000;
/// Paragraphs and table rows this many at most.
const MAX_BLOCKS: usize = 50_000;
/// Marked text this many bytes at most.
const MAX_CHARS: usize = 4_000_000;

/// Whether `bytes` named `name` look like a Word document (extension plus zip magic).
pub fn is_docx(name: &str, bytes: &[u8]) -> bool {
    name.to_ascii_lowercase().ends_with(".docx") && bytes.starts_with(b"PK\x03\x04")
}

/// The marked text of a `.docx` file: `# `/`## `/`### ` headings, `- ` list
/// paragraphs, tables as tab-joined rows, everything else as body text.
pub fn docx_text(name: &str, bytes: &[u8]) -> Result<String, CreateError> {
    if !is_docx(name, bytes) {
        return Err(CreateError::Invalid(format!("{name}: not a .docx file")));
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(CreateError::Invalid(format!("{name}: the file is too large")));
    }
    let xml = part(bytes, "word/document.xml")?;
    let text = std::str::from_utf8(&xml).map_err(|_| CreateError::Invalid(format!("{name}: word/document.xml is not UTF-8")))?;
    Ok(marked(&blocks(text)?))
}

/// One paragraph or table row: 0 body, 1–3 heading, 4 bullet.
struct Block {
    level: u8,
    text: String,
}

fn marked(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        match b.level {
            1 => out.push_str("# "),
            2 => out.push_str("## "),
            3 => out.push_str("### "),
            4 => out.push_str("- "),
            _ => {}
        }
        out.push_str(&b.text);
    }
    out
}

fn u16le(bytes: &[u8], at: usize) -> Option<u16> {
    bytes.get(at..at + 2).map(|w| u16::from_le_bytes([w[0], w[1]]))
}

fn u32le(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

/// A zip entry from the central directory: name, method, sizes, and where its
/// local header sits. The central directory is authoritative, so entries with
/// data descriptors (sizes missing from their local header) still resolve.
struct Entry {
    name: Vec<u8>,
    method: u16,
    csize: u64,
    usize: u64,
    offset: u64,
}

/// The entries of a zip archive without zip64, encryption, or multi-disk splits.
fn entries(bytes: &[u8]) -> Result<Vec<Entry>, CreateError> {
    let bad = |m: &str| CreateError::Invalid(m.into());
    // The end-of-central-directory record sits in the last 64 KiB + 22 bytes.
    let tail = bytes.len().saturating_sub(65_558);
    let mut at = bytes.len().saturating_sub(22);
    let eocd = loop {
        if bytes.get(at..at + 4) == Some(b"PK\x05\x06") {
            break at;
        }
        if at == tail {
            return Err(bad("not a zip archive"));
        }
        at -= 1;
    };
    let count = u16le(bytes, eocd + 10).ok_or_else(|| bad("the zip directory is truncated"))? as usize;
    let central_at = u32le(bytes, eocd + 16).ok_or_else(|| bad("the zip directory is truncated"))? as usize;
    if count > MAX_PARTS {
        return Err(bad("the zip archive has too many files"));
    }
    let mut out = Vec::with_capacity(count.min(1024));
    let mut at = central_at;
    for _ in 0..count {
        if bytes.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(bad("the zip directory is damaged"));
        }
        let flags = u16le(bytes, at + 8).ok_or_else(|| bad("the zip directory is truncated"))?;
        let method = u16le(bytes, at + 10).ok_or_else(|| bad("the zip directory is truncated"))?;
        let csize = u32le(bytes, at + 20).ok_or_else(|| bad("the zip directory is truncated"))? as u64;
        let usize = u32le(bytes, at + 24).ok_or_else(|| bad("the zip directory is truncated"))? as u64;
        let name_len = u16le(bytes, at + 28).ok_or_else(|| bad("the zip directory is truncated"))? as usize;
        let extra_len = u16le(bytes, at + 30).ok_or_else(|| bad("the zip directory is truncated"))? as usize;
        let comment_len = u16le(bytes, at + 32).ok_or_else(|| bad("the zip directory is truncated"))? as usize;
        let offset = u32le(bytes, at + 42).ok_or_else(|| bad("the zip directory is truncated"))? as u64;
        let name_at = at.checked_add(46).ok_or_else(|| bad("the zip directory is truncated"))?;
        let name_end = name_at.checked_add(name_len).ok_or_else(|| bad("the zip directory is truncated"))?;
        let name = bytes.get(name_at..name_end).ok_or_else(|| bad("the zip directory is truncated"))?.to_vec();
        if flags & 1 != 0 {
            return Err(bad("encrypted Office files aren't supported"));
        }
        if csize == 0xFFFF_FFFF || usize == 0xFFFF_FFFF || offset == 0xFFFF_FFFF {
            return Err(bad("zip64 archives aren't supported"));
        }
        if method != 0 && method != 8 {
            return Err(bad("an unsupported compression method"));
        }
        out.push(Entry { name, method, csize, usize, offset });
        at = name_end.checked_add(extra_len).and_then(|v| v.checked_add(comment_len)).ok_or_else(|| bad("the zip directory is truncated"))?;
    }
    Ok(out)
}

/// The inflated bytes of the zip entry `name`, bounded.
fn part(bytes: &[u8], name: &str) -> Result<Vec<u8>, CreateError> {
    let bad = |m: String| CreateError::Invalid(m);
    let entry = entries(bytes)?.into_iter().find(|e| e.name == name.as_bytes()).ok_or_else(|| bad(format!("{name} is missing")))?;
    if entry.usize > MAX_PART_BYTES {
        return Err(bad(format!("{name} is too large")));
    }
    let offset = usize::try_from(entry.offset).map_err(|_| bad("the zip archive is too large".into()))?;
    if bytes.get(offset..offset + 4) != Some(b"PK\x03\x04") {
        return Err(bad("the zip entry is damaged".into()));
    }
    let name_len = u16le(bytes, offset + 26).ok_or_else(|| bad("the zip entry is truncated".into()))? as usize;
    let extra_len = u16le(bytes, offset + 28).ok_or_else(|| bad("the zip entry is truncated".into()))? as usize;
    let data = offset
        .checked_add(30)
        .and_then(|d| d.checked_add(name_len))
        .and_then(|d| d.checked_add(extra_len))
        .ok_or_else(|| bad("the zip entry is truncated".into()))?;
    let csize = usize::try_from(entry.csize).map_err(|_| bad("the zip entry is too large".into()))?;
    let end = data.checked_add(csize).ok_or_else(|| bad("the zip entry is truncated".into()))?;
    let stored = bytes.get(data..end).ok_or_else(|| bad("the zip entry is truncated".into()))?;
    if entry.method == 0 {
        return Ok(stored.to_vec());
    }
    let cap = usize::try_from(entry.usize).map_err(|_| bad("the zip entry is too large".into()))?.saturating_add(1);
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(stored).take(cap as u64).read_to_end(&mut out).map_err(|_| bad(format!("{name} can't be decompressed")))?;
    if out.len() as u64 > MAX_PART_BYTES {
        return Err(bad(format!("{name} is too large")));
    }
    Ok(out)
}

/// A paragraph being collected: its style name, whether it is numbered, and its text.
#[derive(Default)]
struct Para {
    style: Option<String>,
    numbered: bool,
    text: String,
}

/// Whether the element stack sits inside a table cell.
fn in_table(stack: &[Vec<u8>]) -> bool {
    stack.iter().any(|t| t == b"tc")
}

/// Heading level from a paragraph style name: `Heading1`–`Heading9` (capped
/// at 3) and `Title` map, `Subtitle` reads as a second level, numbered
/// paragraphs as bullets.
fn level_of(style: Option<&str>, numbered: bool) -> u8 {
    if let Some(s) = style {
        if s == "Title" {
            return 1;
        }
        if s == "Subtitle" {
            return 2;
        }
        if let Some(n) = s.strip_prefix("Heading").and_then(|n| n.parse::<u8>().ok()) {
            return n.clamp(1, 3);
        }
    }
    if numbered { 4 } else { 0 }
}

/// The decoded value of attribute `key`, if present.
fn attr(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.local_name().as_ref() == key)
        .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Explicit1_0).ok())
        .map(|v| v.into_owned())
}

/// Append `text` to the open paragraph unless inside a skipped subtree.
fn push(skip: u32, text: &str, para: &mut Option<Para>, chars: &mut usize) {
    if skip > 0 || text.is_empty() {
        return;
    }
    if let Some(p) = para.as_mut() {
        p.text.push_str(text);
        *chars = chars.saturating_add(text.len());
    }
}

/// The paragraphs and table rows of `word/document.xml`.
fn blocks(xml: &str) -> Result<Vec<Block>, CreateError> {
    let bad = |m: &str| CreateError::Invalid(m.into());
    let xml = xml.strip_prefix('\u{FEFF}').unwrap_or(xml);
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    // Open element stack (local names, capped): positioning decisions read it.
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut chars = 0usize;
    // The open paragraph (also inside table cells), the stack of open cells'
    // paragraphs, and the stack of open rows' cells.
    let mut para: Option<Para> = None;
    let mut cell: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<Vec<String>> = Vec::new();
    // Inside w:del, w:instrText, alternate content, or a drawing: text there is left out.
    let mut skip: u32 = 0;

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|_| bad("word/document.xml is damaged"))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.local_name().into_inner();
                if stack.len() > 512 {
                    return Err(bad("word/document.xml is too deeply nested"));
                }
                let entered = skip == 0;
                // Subtrees whose text is left out: tracked deletions, field codes,
                // alternate representations, drawings and pictures.
                if entered && matches!(tag, b"del" | b"AlternateContent" | b"drawing" | b"pict" | b"object") {
                    skip += 1;
                }
                if tag == b"instrText" {
                    skip += 1;
                }
                if skip == 0 {
                    match tag {
                        b"p" => {
                            if para.is_none() {
                                para = Some(Para::default());
                            }
                        }
                        b"pStyle" => {
                            if let Some(p) = para.as_mut()
                                && stack.last().is_some_and(|t| t == b"pPr")
                                && let Some(val) = attr(&e, b"val")
                            {
                                p.style = Some(val);
                            }
                        }
                        b"numPr" => {
                            if let Some(p) = para.as_mut() {
                                p.numbered = true;
                            }
                        }
                        b"tr" => {
                            row.push(Vec::new());
                        }
                        b"tc" => {
                            cell.push(Vec::new());
                        }
                        b"tab" => push(skip, "\t", &mut para, &mut chars),
                        b"br" => push(skip, " ", &mut para, &mut chars),
                        b"noBreakHyphen" => push(skip, "-", &mut para, &mut chars),
                        b"sym" => {
                            if let Some(code) = attr(&e, b"char")
                                && matches!(code.as_str(), "F0B7" | "F0A7")
                            {
                                push(skip, "• ", &mut para, &mut chars);
                            }
                        }
                        _ => {}
                    }
                }
                if empty {
                    if tag == b"instrText" {
                        skip = skip.saturating_sub(1);
                    }
                } else {
                    stack.push(tag.to_vec());
                }
                if chars > MAX_CHARS || blocks.len() > MAX_BLOCKS {
                    return Err(bad("the Word document is too large"));
                }
            }
            // Entity and character references arrive as their own events.
            Event::Text(e) => {
                if skip == 0 {
                    let text = e.xml_content(quick_xml::XmlVersion::Explicit1_0).map_err(|_| bad("word/document.xml has unreadable text"))?;
                    push(skip, &text, &mut para, &mut chars);
                    if chars > MAX_CHARS {
                        return Err(bad("the Word document is too large"));
                    }
                }
            }
            Event::GeneralRef(e) => {
                if skip == 0 {
                    // The reference without its `&…;` delimiters: resolve it as one.
                    let raw = std::str::from_utf8(e.as_ref()).map_err(|_| bad("word/document.xml has unreadable text"))?;
                    let full = format!("&{raw};");
                    let text = quick_xml::escape::unescape(&full).map_err(|_| bad("word/document.xml has unreadable text"))?;
                    push(skip, &text, &mut para, &mut chars);
                    if chars > MAX_CHARS {
                        return Err(bad("the Word document is too large"));
                    }
                }
            }
            Event::End(e) => {
                let tag = e.local_name().into_inner();
                if matches!(tag, b"del" | b"AlternateContent" | b"drawing" | b"pict" | b"object" | b"instrText") {
                    skip = skip.saturating_sub(1);
                }
                if skip == 0 {
                    match tag {
                        b"p" => {
                            if let Some(p) = para.take() {
                                let text = p.text.trim().to_string();
                                if !text.is_empty() {
                                    if in_table(&stack) {
                                        if let Some(c) = cell.last_mut() {
                                            c.push(text);
                                        }
                                    } else {
                                        blocks.push(Block { level: level_of(p.style.as_deref(), p.numbered), text });
                                    }
                                }
                            }
                        }
                        b"tc" => {
                            if let Some(c) = cell.pop() {
                                let joined = c.join(" ").trim().to_string();
                                if let Some(r) = row.last_mut() {
                                    r.push(joined);
                                } else if !joined.is_empty() {
                                    blocks.push(Block { level: 0, text: joined });
                                }
                            }
                        }
                        b"tr" => {
                            if let Some(r) = row.pop() {
                                let cells: Vec<String> = r.into_iter().filter(|c| !c.is_empty()).collect();
                                if !cells.is_empty() {
                                    blocks.push(Block { level: 0, text: cells.join("\t") });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if stack.last().is_some_and(|t| t == tag) {
                    stack.pop();
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(blocks)
}
