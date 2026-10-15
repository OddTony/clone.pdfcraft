use super::*;

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c ^= *b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c ^ 0xFFFF_FFFF
}

/// A minimal stored-method zip: just enough for the reader (no compression,
/// no zip64, no descriptors).
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([20u8, 0]); // version needed
        out.extend([0u8, 0]); // flags
        out.extend([0u8, 0]); // method: stored
        out.extend([0u8, 0, 0x21, 0]); // time/date
        out.extend(crc32(data).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0u8, 0]); // extra
        out.extend(name.as_bytes());
        out.extend(*data);
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend([20u8, 0, 20, 0, 0, 0, 0, 0]);
        central.extend([0u8, 0, 0x21, 0]);
        central.extend(crc32(data).to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8, 0]); // extra
        central.extend([0u8, 0]); // comment
        central.extend([0u8, 0]); // disk
        central.extend([0u8, 0]); // internal attributes
        central.extend([0u8, 0, 0, 0]); // external attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend([0u8, 0]);
    out
}

fn docx(document: &str) -> Vec<u8> {
    zip(&[("[Content_Types].xml", b"<Types/>"), ("word/document.xml", document.as_bytes())])
}

#[test]
fn headings_lists_tables_and_runs() {
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Annual Report</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>Over</w:t></w:r><w:r><w:t>view</w:t></w:r></w:p>
<w:p><w:r><w:t>Body with </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold &amp; plain</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>First item</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>A2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:t xml:space="preserve">  spaced  </w:t></w:r></w:p>
</w:body></w:document>"#;
    let text = docx_text("raw.docx", &docx(xml)).expect("docx");
    let lines: Vec<&str> = text.split("\n\n").collect();
    assert_eq!(lines[0], "# Annual Report", "{text}");
    assert_eq!(lines[1], "## Overview", "{text}");
    assert_eq!(lines[2], "Body with bold & plain", "{text}");
    assert_eq!(lines[3], "- First item", "{text}");
    assert_eq!(lines[4], "A1\tB1", "{text}");
    assert_eq!(lines[5], "A2\tB2", "{text}");
    assert_eq!(lines[6], "spaced", "{text}");
}

#[test]
fn fields_deletions_and_drawings_are_left_out() {
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>PAGE</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t>kept</w:t></w:r></w:p>
<w:p><w:r><w:t>gone</w:t></w:r><w:del><w:r><w:delText>removed</w:delText></w:r></w:del></w:p>
<w:p><w:r><w:drawing><wp:inline><w:t>picture text</w:t></wp:inline></w:drawing></w:r></w:p>
</w:body></w:document>"#;
    let text = docx_text("raw.docx", &docx(xml)).expect("docx");
    assert_eq!(text, "kept\n\ngone", "{text}");
}

#[test]
fn broken_inputs_are_refused() {
    assert!(docx_text("raw.txt", b"hello").is_err());
    assert!(docx_text("raw.docx", b"PK\x03\x04garbage").is_err());
    assert!(docx_text("raw.docx", &zip(&[("[Content_Types].xml", b"<Types/>")])).is_err());
    // A zip bomb by declaration: the central directory claims gigabytes.
    let mut bomb = docx("<w:document/>");
    let mut seen = 0;
    let mut i = 0;
    while i + 4 <= bomb.len() {
        if bomb[i..i + 4] == *b"PK\x01\x02" {
            seen += 1;
            if seen == 2 {
                bomb[i + 28..i + 32].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]);
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    assert!(docx_text("raw.docx", &bomb).is_err());
}

#[test]
fn detection_needs_extension_and_magic() {
    assert!(is_docx("raw.docx", b"PK\x03\x04----"));
    assert!(!is_docx("raw.pdf", b"PK\x03\x04----"));
    assert!(!is_docx("raw.docx", b"%PDF----"));
}
