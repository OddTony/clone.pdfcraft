// Contributor-original synthetic imported field: /V is already populated, and
// /AP uses a non-embedded CID font without ToUnicode, like legacy form viewers.
pub const CASES: [(&str, &str, &str, &str); 3] = [
    ("GBK-EUC-H", "GB1", "c9bdccefccabc0c9", "山田太郎"),
    ("GBK-EUC-H", "GB1", "bcf2cce5d6d0cec4", "简体中文"),
    ("B5pc-H", "CNS1", "c163c5e9a4a4a4e5", "繁體中文"),
];

pub fn prefilled(encoding: &str, collection: &str, codes: &str, value: &str) -> Vec<u8> {
    let unicode: String = value.encode_utf16().map(|c| format!("{c:04X}")).collect();
    let body = format!("q BT /Missing 22 Tf 0 g 1 0 0 1 5 20 Tm <{codes}> Tj ET Q");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 640 100] /Annots [5 0 R] >>".into(),
        "<< /Fields [5 0 R] /DA (/Missing 22 Tf 0 g) /DR << /Font << /Missing 7 0 R >> >> >>".into(),
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /P 3 0 R /Rect [10 20 630 80] /V <FEFF{unicode}> /AP << /N 6 0 R >> >>"),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 620 60] /Resources << /Font << /Missing 7 0 R >> >> /Length {} >>\nstream\n{body}\nendstream", body.len()),
        format!("<< /Type /Font /Subtype /Type0 /BaseFont /MissingCjk /Encoding /{encoding} /DescendantFonts [8 0 R] >>"),
        format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /MissingCjk /CIDSystemInfo << /Registry (Adobe) /Ordering ({collection}) /Supplement 2 >> /DW 1000 /FontDescriptor 9 0 R >>"),
        "<< /Type /FontDescriptor /FontName /MissingCjk /Flags 6 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>".into(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    bytes
}
