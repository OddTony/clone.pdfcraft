//! Clearing the pixels of an image under redaction regions. The image is decoded, every pixel
//! whose centre maps into a region is set to zero (no ink for image masks), and the result is
//! written as a new Flate image; the original object is untouched (other pages may use it).
//! A JPEG (`/DCTDecode`) image — the one image a scanned page is made of — is decoded to samples
//! and written back as a new JPEG, so only the region goes and the rest of the page survives.
//! Images whose codec PdfCraft can't clear in place (JPX, JBIG2, CCITT, a CMYK or YCCK JPEG)
//! return `None`, and the caller removes the whole image instead; that is fail-closed.

use image::{ColorType, ExtendedColorType, ImageFormat};
use pdfcraft_content::{Matrix, overlaps};
use pdfcraft_cos::{Document, Object, Stream};

/// Most pixels an image may have to be cleared in place (larger ones are removed).
const MAX_PIXELS: u64 = 64 << 20;

/// Quality of a re-encoded `/DCTDecode` image, which a JPEG can never copy losslessly: high
/// enough that the scan reads as itself, and the same for every cleared image so the result does
/// not depend on what the source happened to be.
const JPEG_QUALITY: u8 = 90;

fn components(doc: &Document, s: &Stream) -> Option<usize> {
    let cs = doc.resolve(s.dict.get(b"ColorSpace")?);
    let cs = &*cs;
    let name = match cs {
        Object::Name(n) => n.as_slice(),
        Object::Array(a) => a.first()?.as_name()?,
        _ => return None,
    };
    Some(match name {
        b"DeviceGray" | b"G" | b"CalGray" | b"Indexed" | b"I" | b"Separation" => 1,
        b"DeviceRGB" | b"RGB" | b"CalRGB" | b"Lab" => 3,
        b"DeviceCMYK" | b"CMYK" => 4,
        b"ICCBased" => {
            // /N lives in the ICC profile stream.
            match &*doc.resolve(cs.as_array()?.get(1)?) {
                Object::Stream(icc) => icc.dict.int(b"N")? as usize,
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// The `/Filter` names in canonical form (so an abbreviation resolves to the same name), or `None`
/// when the filter is one this module doesn't know — an unknown filter is never decoded here.
fn filter_chain(s: &Stream) -> Option<Vec<Vec<u8>>> {
    let canonical = |n: Option<&[u8]>| {
        Some(
            match n? {
                b"FlateDecode" | b"Fl" => "FlateDecode",
                b"LZWDecode" | b"LZW" => "LZWDecode",
                b"ASCII85Decode" | b"A85" => "ASCII85Decode",
                b"ASCIIHexDecode" | b"AHx" => "ASCIIHexDecode",
                b"RunLengthDecode" | b"RL" => "RunLengthDecode",
                b"DCTDecode" | b"DCT" => "DCTDecode",
                _ => return None,
            }
            .as_bytes()
            .to_vec(),
        )
    };
    match s.dict.get(b"Filter") {
        None => Some(Vec::new()),
        Some(Object::Name(n)) => Some(vec![canonical(Some(n))?]),
        Some(Object::Array(a)) => a.iter().map(|f| canonical(f.as_name())).collect(),
        _ => None,
    }
}

/// Is a pixel's sample row, at row `j` of `h`, anywhere near a region? The image is walked row by
/// row, so the whole rows outside every region are skipped without touching a pixel.
///
/// A degenerate row (zero extent on either axis — a 90° rotation makes every row vertical, a
/// flip makes them horizontal, skew can collapse one) must *not* be skipped: `overlaps` is
/// strict in both axes, so a zero-width box would fail it and the mark would be silently ignored
/// while the image survived untouched. When either axis of the transformed row is degenerate the
/// conservative answer is "run the per-pixel test".
fn row_hits(ctm: &Matrix, h: usize, j: usize, rects: &[[f64; 4]]) -> bool {
    let v = 1.0 - (j as f64 + 0.5) / h as f64;
    let (ax, ay) = ctm.apply(0.0, v);
    let (bx, by) = ctm.apply(1.0, v);
    let row = [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)];
    if (row[2] - row[0]).abs() < f64::EPSILON || (row[3] - row[1]).abs() < f64::EPSILON {
        return true;
    }
    rects.iter().any(|r| overlaps(*r, [row[0], row[1] - 0.01, row[2], row[3] + 0.01], 0.0))
}

/// Is the pixel centre at sample `(i, j)` of a `w`×`h` image inside a region?
fn pixel_hit(ctm: &Matrix, w: usize, h: usize, i: usize, j: usize, rects: &[[f64; 4]]) -> bool {
    let (u, v) = ((i as f64 + 0.5) / w as f64, 1.0 - (j as f64 + 0.5) / h as f64);
    let (x, y) = ctm.apply(u, v);
    rects.iter().any(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
}

/// `Ok(Some(copy))` with the covered pixels cleared, `Ok(None)` when no pixel centre is covered,
/// `Err(())` when the image can't be cleared (the caller removes it).
#[allow(clippy::result_unit_err)]
pub(crate) fn clear(doc: &Document, s: &Stream, ctm: &Matrix, rects: &[[f64; 4]]) -> Result<Option<Stream>, ()> {
    let chain = filter_chain(s).ok_or(())?;
    // A scanned page is one JPEG: clear its pixels in place rather than remove the only image on
    // the page. JPEG is lossy, so the copy is re-encoded; every other codec is copied as raw
    // samples, byte for byte.
    let jpeg = matches!(chain.as_slice(), [f] if f == b"DCTDecode");
    if !jpeg && !chain.iter().all(|f| f != b"DCTDecode") {
        return Err(());
    }
    let (w, h) = (s.dict.int(b"Width").ok_or(())?, s.dict.int(b"Height").ok_or(())?);
    if w <= 0 || h <= 0 || (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(());
    }
    let (w, h) = (w as usize, h as usize);
    if jpeg { jpeg_clear(doc, s, ctm, rects, w, h).ok_or(()) } else { raw_clear(doc, s, ctm, rects, w, h).ok_or(()) }
}

/// A decoded JPEG: its 8-bit samples and the colour space they are in.
struct Jpeg {
    px: Vec<u8>,
    space: ColorType,
    ncomp: usize,
}

/// Does any region's rectangle intersect the image's transformed unit square? (Conservative:
/// a mark that only clips the square's edge still counts — the per-pixel centre test may find
/// no sample centre inside such a sliver, and an image the mark touches must not survive as-is.)
fn image_touched(ctm: &Matrix, rects: &[[f64; 4]]) -> bool {
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
    let mut box_ = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (u, v) in corners {
        let (x, y) = ctm.apply(u, v);
        box_[0] = box_[0].min(x);
        box_[1] = box_[1].min(y);
        box_[2] = box_[2].max(x);
        box_[3] = box_[3].max(y);
    }
    rects.iter().any(|r| overlaps(*r, box_, 0.0))
}

fn jpeg_clear(doc: &Document, s: &Stream, ctm: &Matrix, rects: &[[f64; 4]], w: usize, h: usize) -> Option<Option<Stream>> {
    let Jpeg { mut px, space, ncomp } = decode_jpeg(doc, s, w, h)?;
    let mut cleared = 0usize;
    for j in 0..h {
        if !row_hits(ctm, h, j, rects) {
            continue;
        }
        for i in 0..w {
            if !pixel_hit(ctm, w, h, i, j, rects) {
                continue;
            }
            cleared += 1;
            let at = (j * w + i) * ncomp;
            if let Some(sample) = px.get_mut(at..at + ncomp) {
                sample.fill(0);
            }
        }
    }
    if cleared == 0 {
        // A mark that intersects the image (its unit square) but clears no pixel centre — an
        // edge-only overlap, or a mark thinner than one pixel footprint — must not leave the
        // original untouched: the overlay hides it visually, but the samples survive. Fail
        // closed; the caller removes the image whole. (A mark entirely outside the image is
        // the legitimate `Ok(None)` case.)
        if image_touched(ctm, rects) {
            return None;
        }
        return Some(None);
    }
    let encoded = encode_jpeg(&px, w, h, space)?;
    let mut dict = s.dict.clone();
    dict.remove(b"Length");
    dict.remove(b"DecodeParms");
    // /SMask and /Mask still apply: the copy keeps the image's dimensions and colour space.
    Some(Some(Stream::from_raw(dict, encoded)))
}

/// `/Decode` must be the identity mapping: any other one would show the cleared samples as white
/// rather than black, or invert the rest of the scan, so such an image is removed whole instead.
fn identity_decode(doc: &Document, s: &Stream, ncomp: usize) -> bool {
    let Some(d) = s.dict.get(b"Decode") else { return true };
    let v: Vec<f64> = doc.resolve(d).as_array().map(|a| a.iter().filter_map(|o| doc.resolve(o).as_f64()).collect()).unwrap_or_default();
    v.len() == 2 * ncomp && v.as_chunks::<2>().0.iter().all(|&[lo, hi]| lo == 0.0 && hi == 1.0)
}

fn decode_jpeg(doc: &Document, s: &Stream, w: usize, h: usize) -> Option<Jpeg> {
    // A JPEG carries its own dimensions and colour space; a dictionary that disagrees is not one
    // this can safely clear.
    if s.dict.get(b"BitsPerComponent").is_some_and(|b| b.as_int() != Some(8)) {
        return None;
    }
    let space = match components(doc, s)? {
        1 => ColorType::L8,
        3 => ColorType::Rgb8,
        _ => return None,
    };
    // `/Decode` must match the *image's* component count (one for DeviceGray, three for RGB):
    // a gray scan with an identity `/Decode [0 1]` would otherwise fail the check and be
    // removed whole — the very blanking this fix exists to prevent.
    if !identity_decode(doc, s, if space == ColorType::L8 { 1 } else { 3 }) {
        return None;
    }
    // The JPEG header is inspected *before* the sample allocation: a dictionary that declares a
    // small image but carries a much larger JPEG must not be decoded first — the crate's
    // `MAX_PIXELS` cap is enforced against the header, and only then are the samples decoded.
    let reader = image::ImageReader::with_format(std::io::Cursor::new(s.raw.as_ref()), ImageFormat::Jpeg);
    let (iw, ih) = reader.into_dimensions().ok()?;
    if (iw as usize, ih as usize) != (w, h) {
        return None;
    }
    let reader = image::ImageReader::with_format(std::io::Cursor::new(s.raw.as_ref()), ImageFormat::Jpeg);
    let decoded = reader.decode().ok()?;
    debug_assert_eq!((decoded.width() as usize, decoded.height() as usize), (w, h));
    let (px, ncomp) = match space {
        ColorType::L8 => (decoded.into_luma8().into_raw(), 1),
        _ => (decoded.into_rgb8().into_raw(), 3),
    };
    (px.len() == w.checked_mul(h)?.checked_mul(ncomp)?).then_some(Jpeg { px, space, ncomp })
}

fn encode_jpeg(px: &[u8], w: usize, h: usize, space: ColorType) -> Option<Vec<u8>> {
    let color = match space {
        ColorType::L8 => ExtendedColorType::L8,
        _ => ExtendedColorType::Rgb8,
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode(px, w as u32, h as u32, color).ok()?;
    Some(out)
}

fn raw_clear(doc: &Document, s: &Stream, ctm: &Matrix, rects: &[[f64; 4]], w: usize, h: usize) -> Option<Option<Stream>> {
    let mask = matches!(s.dict.get(b"ImageMask"), Some(Object::Bool(true)));
    let (ncomp, bpc) = if mask { (1, 1) } else { (components(doc, s)?, s.dict.int(b"BitsPerComponent")? as usize) };
    if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
        return None;
    }
    let mut data = s.decoded().ok()?;
    let row = (w * ncomp * bpc).div_ceil(8);
    if data.len() < row * h {
        return None;
    }
    data.truncate(row * h);
    // Image masks: a sample of 0 paints unless /Decode is [1 0]; cleared samples must not paint.
    let clear_bit = if mask {
        let inverted = s.dict.get(b"Decode").and_then(Object::as_array).and_then(|d| d.first()).and_then(Object::as_f64) == Some(1.0);
        !inverted
    } else {
        false
    };
    let bits_per_pixel = ncomp * bpc;
    let mut cleared = 0usize;
    for j in 0..h {
        // Image space: (0,0) is the bottom-left of the unit square; row 0 is the top.
        if !row_hits(ctm, h, j, rects) {
            continue;
        }
        for i in 0..w {
            if !pixel_hit(ctm, w, h, i, j, rects) {
                continue;
            }
            cleared += 1;
            let start = j * row * 8 + i * bits_per_pixel;
            for bit in start..start + bits_per_pixel {
                let (byte, shift) = (bit / 8, 7 - bit % 8);
                if clear_bit {
                    data[byte] |= 1 << shift;
                } else {
                    data[byte] &= !(1 << shift);
                }
            }
        }
    }
    if cleared == 0 {
        // Same fail-closed rule as the JPEG path: a mark that touches this image but whose
        // rectangle covers no sample centre (an edge sliver) removes the image whole.
        if image_touched(ctm, rects) {
            return None;
        }
        return Some(None);
    }
    let mut dict = s.dict.clone();
    dict.remove(b"Length");
    dict.remove(b"DecodeParms");
    Some(Some(Stream::flate(dict, &data)))
}
