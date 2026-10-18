//! Regression: Linux builds must retain winit's vector Wayland controls without
//! accidentally pulling in the optional title font. Parse Cargo metadata instead
//! of matching textual TOML formatting, so equivalent manifests remain valid.

const MANIFEST: &str = include_str!("../Cargo.toml");
const LOCK: &str = include_str!("../../../Cargo.lock");

fn dependency<'a>(packages: &'a [toml::Value], parent: &toml::Value, name: &str) -> &'a toml::Value {
    let reference = parent
        .get("dependencies")
        .and_then(toml::Value::as_array)
        .and_then(|deps| deps.iter().filter_map(toml::Value::as_str).find(|dep| dep.split_whitespace().next() == Some(name)))
        .unwrap_or_else(|| panic!("missing {name} dependency"));
    let mut parts = reference.split_whitespace();
    let resolved_name = parts.next().expect("nonempty dependency reference");
    let version = parts.next();

    let mut candidates = packages.iter().filter(|p| {
        p.get("name").and_then(toml::Value::as_str) == Some(resolved_name)
            && version.is_none_or(|v| p.get("version").and_then(toml::Value::as_str) == Some(v))
    });
    let resolved = candidates.next().unwrap_or_else(|| panic!("missing locked package {reference}"));
    assert!(candidates.next().is_none(), "ambiguous locked package {reference}");
    resolved
}

#[test]
fn linux_winit_enables_the_adwaita_frame_without_a_title_font() {
    let manifest: toml::Value = toml::from_str(MANIFEST).expect("valid Cargo.toml");
    let winit = &manifest["target"]["cfg(target_os = \"linux\")"]["dependencies"]["winit"];
    let features = winit["features"].as_array().expect("Linux winit feature list");
    let names: Vec<&str> = features.iter().map(|feature| feature.as_str().expect("string feature")).collect();

    assert!(names.contains(&"wayland-csd-adwaita-notitle"), "vector Adwaita controls must remain enabled");
    assert!(!names.contains(&"wayland-csd-adwaita"), "the title-font feature must stay disabled");
}

#[test]
fn lock_resolves_adwaita_without_title_font_dependencies() {
    let lock: toml::Value = toml::from_str(LOCK).expect("valid Cargo.lock");
    let packages = lock["package"].as_array().expect("locked packages");
    let app = packages.iter().find(|p| p["name"].as_str() == Some("pdfcraft")).expect("locked application package");
    let winit = dependency(packages, app, "winit");
    let adwaita = dependency(packages, winit, "sctk-adwaita");
    let names: Vec<&str> = adwaita["dependencies"]
        .as_array()
        .expect("Adwaita dependencies")
        .iter()
        .map(|item| item.as_str().expect("string dependency").split_whitespace().next().expect("nonempty dependency"))
        .collect();
    for name in ["ab_glyph", "crossfont", "memmap2"] {
        assert!(!names.contains(&name), "{name} must not be pulled in by the Adwaita title renderer");
    }
}
