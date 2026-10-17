//! Embeds the fonts of the optional craft-fonts build input (`CRAFT_FONTS_DIR`) as `CRAFT_FONTS`.
//! The recipe is craft-fonts' `docs/integration.md`; unset, `CRAFT_FONTS` is empty. It only reads
//! the local checkout (no network). On wasm32 it defaults to only the UI faces (BIZ UDPGothic Regular
//! and any `Arab`, `Telu`, `Hebr` or `Thai` face), to keep the web build within hosting limits (Cloudflare Pages:
//! 25 MiB per file). `CRAFT_FONTS_WEB=all` opts into every otherwise permitted manifest face.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_WEB");
    // Validate configuration even without a font input, and never downgrade a typo to the
    // optional-input warning controlled by CRAFT_FONTS_REQUIRED.
    let web_all = match std::env::var_os("CRAFT_FONTS_WEB") {
        None => false,
        Some(value) if value == "all" => true,
        Some(value) => {
            println!("cargo::error=unsupported CRAFT_FONTS_WEB={value:?}; leave it unset for the default WASM font selection or set it to all");
            return;
        }
    };
    let mut src = String::from("pub static CRAFT_FONTS: &[CraftFont] = &[\n");
    if let Some(dir) = std::env::var_os("CRAFT_FONTS_DIR").map(PathBuf::from) {
        match craft_fonts(&dir, web_all) {
            Ok(entries) => src.push_str(&entries),
            Err(e) if std::env::var_os("CRAFT_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=CRAFT_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without craft-fonts: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// Family-name prefixes of the faces AGENTS.md §1.1 forbids: Adobe's Source families and Noto
/// CJK, which is Source Han under another name. The test in `src/craft.rs` has the same list.
const BARRED_FAMILIES: [&str; 5] = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];

/// One `CraftFont { .. }` initialiser per manifest line.
fn craft_fonts(dir: &std::path::Path, web_all: bool) -> Result<String, String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        // AGENTS.md §1.1 bars Adobe's type designs whatever their licence, so a checkout that
        // carries one (craft-fonts has Noto Sans CJK, which is Source Han) never embeds it.
        if BARRED_FAMILIES.iter().any(|barred| family.starts_with(barred)) {
            println!("cargo::warning=craft-fonts: not embedding {family} {style} (AGENTS.md §1.1)");
            continue;
        }
        // Arabic interface faces are small, so the web build keeps them too.
        let arabic = scripts.split(',').any(|s| s.trim() == "Arab");
        // Telugu faces too, for the Telugu interface.
        let telugu = scripts.split(',').any(|s| s.trim() == "Telu");
        // And the Hebrew ones (small), for Hebrew text and Edit text on the web.
        let hebrew = scripts.split(',').any(|s| s.trim() == "Hebr");
        let web_face =
            (*family == "BIZ UDPGothic" && *style == "Regular") || arabic || telugu || hebrew || scripts.split(',').any(|s| s.trim() == "Thai");
        if wasm && !web_all && !web_face {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    CraftFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}
