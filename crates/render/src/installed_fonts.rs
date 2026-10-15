//! Reading substitutes for Chinese fonts a Windows PDF names but does not embed.
//!
//! These programs are read from the user's installed fonts at runtime, never bundled or added
//! to the PDF object graph. The document writer and font-embedding APIs do not use this module.
//! `PDFCRAFT_SYSTEM_FONTS=0` disables it, as it does the interface's installed-font fallback.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use hayro::hayro_interpret::font::{FallbackFontQuery, FontData, FontQuery};
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use skrifa::MetadataProvider as _;

const MAX_BYTES: u64 = 32 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    Song,
    Kai,
    Hei,
    Deng,
    FangSong,
}

impl Face {
    fn file(self) -> &'static str {
        match self {
            Self::Song => "simsun.ttc",
            Self::Kai => "simkai.ttf",
            Self::Hei => "simhei.ttf",
            Self::Deng => "Deng.ttf",
            Self::FangSong => "simfang.ttf",
        }
    }

    fn cache(self) -> &'static OnceLock<Option<(FontData, u32)>> {
        static SONG: OnceLock<Option<(FontData, u32)>> = OnceLock::new();
        static KAI: OnceLock<Option<(FontData, u32)>> = OnceLock::new();
        static HEI: OnceLock<Option<(FontData, u32)>> = OnceLock::new();
        static DENG: OnceLock<Option<(FontData, u32)>> = OnceLock::new();
        static FANG_SONG: OnceLock<Option<(FontData, u32)>> = OnceLock::new();
        match self {
            Self::Song => &SONG,
            Self::Kai => &KAI,
            Self::Hei => &HEI,
            Self::Deng => &DENG,
            Self::FangSong => &FANG_SONG,
        }
    }
}

/// Only fixed known Windows font names and Adobe-GB1 fonts are eligible. A PDF name is never
/// interpreted as a file path, and unrelated or embedded fonts keep their original resolver.
pub(super) fn resolve(query: &FontQuery) -> Option<(FontData, u32)> {
    if std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_some_and(|v| v == "0") {
        return None;
    }
    let FontQuery::Fallback(query) = query else { return None };
    let face = select(query)?;
    // If the exact face is absent, preserve serif/sans character with a GB1-capable face.
    let alternate = if matches!(face, Face::Hei | Face::Deng) { Face::Hei } else { Face::Song };
    installed(face).or_else(|| installed(alternate))
}

fn select(query: &FallbackFontQuery) -> Option<Face> {
    let name = query.post_script_name.as_deref().unwrap_or_default();
    // Subset prefixes are normally already stripped by hayro. Keep this helper total when
    // called directly, and do not turn an arbitrary '+' in a family name into a known face.
    let name = match name.split_once('+') {
        Some((prefix, rest)) if prefix.len() == 6 && prefix.bytes().all(|c| c.is_ascii_uppercase()) => rest,
        _ => name,
    };
    match name.to_ascii_lowercase().as_str() {
        "simsun" | "nsimsun" | "宋体" | "新宋体" => Some(Face::Song),
        "kaiti" | "kaiti_gb2312" | "楷体" | "楷体_gb2312" => Some(Face::Kai),
        "simhei" | "黑体" => Some(Face::Hei),
        "dengxian" | "等线" => Some(Face::Deng),
        "fangsong" | "fangsong_gb2312" | "仿宋" | "仿宋_gb2312" => Some(Face::FangSong),
        _ if query.character_collection.as_ref().is_some_and(|c| c.family == CidFamily::AdobeGB1) => {
            Some(if query.is_serif { Face::Song } else { Face::Hei })
        }
        _ => None,
    }
}

fn installed(face: Face) -> Option<(FontData, u32)> {
    face.cache().get_or_init(|| directories().iter().find_map(|root| read(&root.join(face.file())))).clone()
}

/// The exact installed reading face for an editor's on-screen preview. The bytes stay in
/// memory; callers must never pass this font to embedding or document-writing APIs.
pub(crate) fn preview(name: &str) -> Option<(Vec<u8>, u32)> {
    let query = FontQuery::Fallback(FallbackFontQuery { post_script_name: Some(name.into()), ..Default::default() });
    let (data, index) = resolve(&query)?;
    Some((data.as_ref().as_ref().to_vec(), index))
}

fn directories() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        roots.push(PathBuf::from(windows).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
    }
    roots
}

fn read(path: &Path) -> Option<(FontData, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    // The first face of SimSun's TTC is SimSun; other candidates are standalone sfnt faces.
    let font = skrifa::FontRef::from_index(&bytes, 0).ok()?;
    if !['中', '文'].into_iter().all(|c| font.charmap().map(c).is_some_and(|g| g.to_u32() != 0)) {
        return None;
    }
    log::debug!("PDF reading font: {}", path.display());
    Some((Arc::new(bytes), 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hayro::hayro_interpret::hayro_cmap::CharacterCollection;

    #[test]
    fn names_and_collection_select_chinese_faces_without_paths() {
        for (name, face) in [
            ("SimSun", Face::Song),
            ("ABCDEF+SimSun", Face::Song),
            ("KaiTi_GB2312", Face::Kai),
            ("黑体", Face::Hei),
            ("DengXian", Face::Deng),
            ("FangSong", Face::FangSong),
        ] {
            assert_eq!(select(&FallbackFontQuery { post_script_name: Some(name.into()), ..Default::default() }), Some(face));
        }
        for name in ["Helvetica", "HeiseiMin-W3", "../SimSun", "C:/Windows/Fonts/simsun.ttc", "bad+SimSun"] {
            assert_eq!(select(&FallbackFontQuery { post_script_name: Some(name.into()), ..Default::default() }), None);
        }
        let mut query = FallbackFontQuery {
            character_collection: Some(CharacterCollection { family: CidFamily::AdobeGB1, supplement: 2 }),
            ..Default::default()
        };
        assert_eq!(select(&query), Some(Face::Hei));
        query.is_serif = true;
        assert_eq!(select(&query), Some(Face::Song));
        query.character_collection = Some(CharacterCollection { family: CidFamily::AdobeJapan1, supplement: 2 });
        assert_eq!(select(&query), None);
    }

    #[test]
    fn missing_and_non_font_inputs_are_skipped() {
        assert!(read(Path::new("definitely/not/a/font.ttf")).is_none());
        assert!(read(&std::env::temp_dir()).is_none());
    }
}
