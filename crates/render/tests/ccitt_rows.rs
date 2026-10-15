//! #2352: a scanned page whose CCITT G4 image says `/Rows 0` ("use the image height", valid per
//! ISO 32000) rendered white: the vendored hayro-syntax decoded zero rows. The image's height now
//! takes precedence over `/Rows` (`vendor/hayro-syntax/src/filter/ccitt.rs`, PdfCraft patch 17).

use std::sync::Arc;

use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};

/// An 8 × 4 Group 4 strip, its left half black (ghostscript's `tiffg4` device).
const G4: [u8; 6] = [0x26, 0xaf, 0xff, 0x00, 0x10, 0x01];

/// A page the size of the image, filled with it, `rows` as the image's `/Rows`.
fn page(rows: &str) -> Vec<u8> {
    let content = b"q 8 0 0 4 0 0 cm /Im Do Q";
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 8 4] /Contents 4 0 R /Resources << /XObject << /Im 5 0 R >> >> >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content, b"\nendstream"].concat(),
        [
            format!(
                "<< /Type /XObject /Subtype /Image /Width 8 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode /DecodeParms << /K -1 /Columns 8 {rows} >> /Length {} >>\nstream\n",
                G4.len()
            )
            .as_bytes(),
            &G4,
            b"\nendstream",
        ]
        .concat(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
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

/// The red channel of pixel (x, y) of page 1 rendered at 1:1.
fn red_at(bytes: Vec<u8>, x: u32, y: u32) -> u8 {
    let mut renderer = PageRenderer::new(Arc::new(bytes), RenderConfig::default());
    let page = renderer.render(RenderRequest { page: 0, kind: RequestKind::Pixels, tile: None, scale: 1.0, tag: 0, preview: false });
    assert!(page.error.is_none(), "{:?}", page.error);
    assert_eq!((page.width, page.height), (8, 4));
    page.rgba[((y * page.width + x) * 4) as usize]
}

#[test]
fn a_ccitt_image_whose_rows_is_zero_decodes_to_its_height() {
    for rows in ["/Rows 4", "/Rows 0", ""] {
        assert!(red_at(page(rows), 1, 1) < 64, "{rows:?}: the left half is black");
        assert!(red_at(page(rows), 6, 1) > 191, "{rows:?}: the right half is white");
    }
}
