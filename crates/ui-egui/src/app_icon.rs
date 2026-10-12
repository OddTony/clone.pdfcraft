//! A custom app icon (Preferences ▸ Appearance). The chosen image (PNG, JPEG, WebP, ICO, ICNS or
//! SVG) is converted to a square PNG kept in the settings folder; the window and taskbar show it at
//! once, and the app opens with it from then on (the macOS Dock takes its icon when PdfCraft opens).

use std::path::Path;

/// Side of the icon kept, in pixels (the largest size the platforms ask for in a window icon).
pub const SIZE: u32 = 512;

/// Larger files are not read: an image path is still untrusted input.
const MAX_BYTES: u64 = 32 << 20;

/// The image at `path` as a [`SIZE`]-pixel square PNG: scaled to fit, centred on transparency.
pub fn to_png(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(format!("{}: not an image file of at most 32 MB", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let rgba = match ext.as_str() {
        "svg" => svg(&bytes)?,
        "icns" => icns(&bytes)?,
        _ => image::load_from_memory(&bytes).map_err(|e| e.to_string())?.into_rgba8(),
    };
    let fitted = fit(&rgba);
    let mut png = Vec::new();
    fitted.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(png)
}

/// The window icon for a PNG [`to_png`] wrote.
pub fn icon_data(png: &[u8]) -> Option<egui::IconData> {
    let rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData { rgba: rgba.into_raw(), width, height })
}

fn fit(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let scale = SIZE as f32 / w.max(h).max(1) as f32;
    let (nw, nh) = (((w as f32 * scale).round() as u32).clamp(1, SIZE), ((h as f32 * scale).round() as u32).clamp(1, SIZE));
    let scaled = image::imageops::resize(img, nw, nh, image::imageops::FilterType::Lanczos3);
    let mut square = image::RgbaImage::new(SIZE, SIZE);
    image::imageops::overlay(&mut square, &scaled, i64::from((SIZE - nw) / 2), i64::from((SIZE - nh) / 2));
    square
}

fn svg(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = SIZE as f32 / size.width().max(size.height()).max(1.0);
    let (w, h) = (((size.width() * scale).ceil() as u32).max(1), ((size.height() * scale).ceil() as u32).max(1));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("the SVG has no size")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied alpha; the image wants it straight.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| "the SVG couldn't be drawn".to_string())
}

/// The largest image in an ICNS file.
fn icns(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let family = icns::IconFamily::read(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let largest = family.available_icons().into_iter().max_by_key(|t| t.pixel_width() * t.pixel_height()).ok_or("the ICNS file has no images")?;
    let icon = family.get_icon_with_type(largest).map_err(|e| e.to_string())?.convert_to(icns::PixelFormat::RGBA);
    image::RgbaImage::from_raw(icon.width(), icon.height(), icon.into_data().into_vec()).ok_or_else(|| "the ICNS image couldn't be read".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfcraft-app-icon-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        let _ = std::fs::write(&path, bytes);
        path
    }

    #[test]
    fn images_become_square_pngs() {
        // A wide PNG: fitted to 512 wide, centred, transparent above and below.
        let mut wide = image::RgbaImage::new(200, 100);
        for p in wide.pixels_mut() {
            *p = image::Rgba([10, 20, 30, 255]);
        }
        let mut png = Vec::new();
        wide.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let out = to_png(&write("wide.png", &png)).unwrap();
        let icon = icon_data(&out).unwrap();
        assert_eq!((icon.width, icon.height), (SIZE, SIZE));
        assert_eq!(icon.rgba[3], 0, "the corner is transparent");
        let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        assert_eq!(&icon.rgba[centre..centre + 4], &[10, 20, 30, 255]);

        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#f00"/></svg>"##;
        let icon = icon_data(&to_png(&write("red.svg", svg)).unwrap()).unwrap();
        assert_eq!(&icon.rgba[centre..centre + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn what_isnt_an_image_is_an_error() {
        assert!(to_png(&write("text.png", b"not an image")).is_err());
        assert!(to_png(&write("text.svg", b"<not svg")).is_err());
        assert!(to_png(&write("text.icns", b"icns")).is_err());
        assert!(to_png(Path::new("/definitely/not/here.png")).is_err());
    }
}
