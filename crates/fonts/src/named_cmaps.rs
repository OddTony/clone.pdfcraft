//! Shared standard GB/GBK mappings for reading and rewriting existing PDF text.
//! Only the fixed embedded CMaps are read, never document-supplied PostScript or URLs.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use hayro_cmap::{BfString, CMap, CMapName, load_embedded};

pub(crate) struct NamedCMap {
    pub spaces: Vec<(usize, Vec<u8>, Vec<u8>)>,
    pub cids: Vec<(u32, u32, u32)>,
    pub unicode: HashMap<u32, String>,
}

pub(crate) fn get(name: &[u8]) -> Option<Arc<NamedCMap>> {
    static GB: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    static GB_V: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    static GBK: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    static GBK_V: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    static GBKP: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    static GBKP_V: OnceLock<Option<Arc<NamedCMap>>> = OnceLock::new();
    let cache = match name {
        b"GB-EUC-H" => &GB,
        b"GB-EUC-V" => &GB_V,
        b"GBK-EUC-H" => &GBK,
        b"GBK-EUC-V" => &GBK_V,
        b"GBKp-EUC-H" => &GBKP,
        b"GBKp-EUC-V" => &GBKP_V,
        _ => return None,
    };
    cache.get_or_init(|| load(name).map(Arc::new)).clone()
}

fn load(name: &[u8]) -> Option<NamedCMap> {
    let encoding = CMap::parse(load_embedded(CMapName::from_bytes(name))?, load_embedded)?;
    let unicode = CMap::parse(load_embedded(CMapName::AdobeGb1Ucs2)?, load_embedded)?;
    let mut result = NamedCMap { spaces: Vec::new(), cids: Vec::new(), unicode: HashMap::new() };
    // All supported CMaps have one- and two-byte codes, so initialization has a fixed budget.
    // Use their actual CID mapping (GBK bytes are not CIDs), including proportional ASCII.
    for length in 1usize..=2 {
        let mut space: Option<(u32, u32)> = None;
        let limit = if length == 1 { 0xFF } else { 0xFFFF };
        for code in 0..=limit {
            let Some(cid) = encoding.lookup_cid_code(code, length as u8) else {
                if let Some((lo, hi)) = space.take() {
                    result.spaces.push((length, bytes(lo, length), bytes(hi, length)));
                }
                continue;
            };
            match space.as_mut() {
                Some((_, hi)) => *hi = code,
                None => space = Some((code, code)),
            }
            // Consecutive codes and CIDs can share one compact range. Never expand them in
            // Metrics::width, which is called for every glyph and must stay cheap.
            if let Some((lo, hi, start)) = result.cids.last_mut()
                && hi.checked_add(1) == Some(code)
                && start.checked_add(code.saturating_sub(*lo)) == Some(cid)
            {
                *hi = code;
            } else {
                result.cids.push((code, code, cid));
            }
            let text = match unicode.lookup_bf_string(cid) {
                Some(BfString::Char(c)) => Some(c.to_string()),
                Some(BfString::String(s)) => Some(s),
                None => (1..=2).find_map(|len| unicode.lookup_cid_code(cid, len).and_then(char::from_u32).map(|c| c.to_string())),
            };
            if let Some(text) = text {
                result.unicode.insert(code, text);
            }
        }
        if let Some((lo, hi)) = space {
            result.spaces.push((length, bytes(lo, length), bytes(hi, length)));
        }
    }
    Some(result)
}

fn bytes(code: u32, length: usize) -> Vec<u8> {
    if length == 1 { vec![code as u8] } else { (code as u16).to_be_bytes().to_vec() }
}
