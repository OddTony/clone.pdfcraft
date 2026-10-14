//! Keep the shared reader pinned while preserving the app-owned inspector boundary.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const SHARED_REV: &str = "89dcbb3060377a26c99a6d80caa16084366704c4";
const PACKAGES: &[(&str, &str)] =
    &[("craft-pdf", "0.1.0"), ("hayro", "0.7.1"), ("hayro-interpret", "0.7.0"), ("hayro-syntax", "0.7.2"), ("hayro-jbig2", "0.3.0")];

pub fn run(root: &Path) -> Result<(), String> {
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo metadata: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    validate(root, &metadata)?;
    println!("PDF reader: craft-pdf and all four Hayro packages pinned to {SHARED_REV}; local lopdf inspector retained");
    Ok(())
}

fn validate(root: &Path, metadata: &Value) -> Result<(), String> {
    let packages = metadata.get("packages").and_then(Value::as_array).ok_or("cargo metadata has no packages")?;
    let one_package = |name: &str| -> Result<&Value, String> {
        let matches: Vec<_> = packages.iter().filter(|p| p.get("name").and_then(Value::as_str) == Some(name)).collect();
        if matches.len() != 1 {
            return Err(format!("{name}: expected one resolved package, found {}", matches.len()));
        }
        matches.first().copied().ok_or_else(|| format!("missing {name}"))
    };
    let source = format!("git+https://github.com/storytold/craft-libs?rev={SHARED_REV}#{SHARED_REV}");
    for &(name, version) in PACKAGES {
        let package = one_package(name)?;
        if package.get("version").and_then(Value::as_str) != Some(version) || package.get("source").and_then(Value::as_str) != Some(source.as_str()) {
            return Err(format!("{name}: expected {version} from {source}; got {package}"));
        }
    }
    let inspector = one_package("lopdf")?;
    let manifest = root.join("vendor/lopdf/Cargo.toml");
    if inspector.get("version").and_then(Value::as_str) != Some("0.45.0")
        || inspector.get("source") != Some(&Value::Null)
        || inspector.get("manifest_path").and_then(Value::as_str).map(Path::new) != Some(manifest.as_path())
    {
        return Err("lopdf: expected the existing local vendor/lopdf inspector at version 0.45.0".into());
    }
    let nodes = metadata.get("resolve").and_then(|r| r.get("nodes")).and_then(Value::as_array).ok_or("cargo metadata has no resolved nodes")?;
    let shared_id = one_package("craft-pdf")?.get("id").and_then(Value::as_str).ok_or("craft-pdf has no package id")?;
    let node = nodes.iter().find(|n| n.get("id").and_then(Value::as_str) == Some(shared_id)).ok_or("craft-pdf has no resolved node")?;
    let features = node.get("features").and_then(Value::as_array).ok_or("craft-pdf has no resolved features")?;
    let actual: BTreeSet<_> = features.iter().filter_map(Value::as_str).collect();
    let expected: BTreeSet<_> = ["read", "interpret", "raster"].into_iter().collect();
    if actual != expected {
        return Err(format!("craft-pdf: expected reader features {expected:?}; got {features:?}"));
    }
    // COS/editing crates keep their existing syntax access. Only the rendering adapter
    // moves its production dependencies behind craft-pdf; backend tests may use Hayro.
    let dependencies = one_package("pdfcraft-render")?.get("dependencies").and_then(Value::as_array).ok_or("pdfcraft-render has no dependencies")?;
    let mut has_shared = false;
    for dependency in dependencies {
        let kind = dependency.get("kind").ok_or("render dependency has no kind")?;
        if kind.as_str() == Some("dev") {
            continue;
        }
        let name = dependency.get("name").and_then(Value::as_str).ok_or("render dependency has no name")?;
        if name == "hayro" || name.starts_with("hayro-") {
            return Err(format!("pdfcraft-render has a direct production dependency on {name}"));
        }
        has_shared |= name == "craft-pdf" && kind.is_null();
    }
    if !has_shared {
        return Err("pdfcraft-render must depend on craft-pdf in production".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata() -> Value {
        let source = format!("git+https://github.com/storytold/craft-libs?rev={SHARED_REV}#{SHARED_REV}");
        let mut packages =
            PACKAGES.iter().map(|(name, version)| json!({"id":name, "name":name, "version":version, "source":source})).collect::<Vec<_>>();
        packages.push(json!({"name":"lopdf", "version":"0.45.0", "source":null, "manifest_path":"/workspace/vendor/lopdf/Cargo.toml"}));
        packages.push(json!({"name":"pdfcraft-render", "dependencies":[
            {"name":"craft-pdf", "kind":null}, {"name":"hayro", "kind":"dev"}, {"name":"hayro-syntax", "kind":"dev"}
        ]}));
        json!({"packages":packages, "resolve":{"nodes":[{"id":"craft-pdf", "features":["read","interpret","raster"]}]}})
    }

    fn check(metadata: &Value) -> Result<(), String> {
        validate(Path::new("/workspace"), metadata)
    }

    #[test]
    fn accepts_pinned_reader_and_local_inspector() {
        assert!(check(&metadata()).is_ok());
    }

    #[test]
    fn rejects_registry_path_and_revision_drift_for_every_shared_package() {
        for index in 0..PACKAGES.len() {
            for source in [
                Value::Null,
                json!("registry+https://github.com/rust-lang/crates.io-index"),
                json!("git+https://github.com/storytold/craft-libs#other"),
            ] {
                let mut m = metadata();
                m["packages"][index]["source"] = source;
                assert!(check(&m).is_err());
            }
        }
    }

    #[test]
    fn rejects_duplicate_missing_and_wrong_version_packages() {
        for index in 0..PACKAGES.len() {
            let mut m = metadata();
            m["packages"][index]["version"] = json!("99.0.0");
            assert!(check(&m).is_err());
            let mut m = metadata();
            let duplicate = m["packages"][index].clone();
            m["packages"].as_array_mut().unwrap().push(duplicate);
            assert!(check(&m).is_err());
            let mut m = metadata();
            m["packages"].as_array_mut().unwrap().remove(index);
            assert!(check(&m).is_err());
        }
    }

    #[test]
    fn rejects_writer_features_and_missing_interpreter() {
        for features in [json!(["read"]), json!(["read", "interpret", "raster", "write"]), json!(["read", "interpret", "raster", "default"])] {
            let mut m = metadata();
            m["resolve"]["nodes"][0]["features"] = features;
            assert!(check(&m).is_err());
        }
    }

    #[test]
    fn rejects_renderer_bypass_and_development_only_shared_dependency() {
        for kind in [Value::Null, json!("build")] {
            let mut m = metadata();
            m["packages"][6]["dependencies"][1]["kind"] = kind;
            assert!(check(&m).is_err());
        }
        let mut m = metadata();
        m["packages"][6]["dependencies"][0]["kind"] = json!("dev");
        assert!(check(&m).is_err());
    }

    #[test]
    fn rejects_inspector_replacement_and_incomplete_metadata() {
        let mut m = metadata();
        m["packages"][5]["source"] = json!("registry+https://github.com/rust-lang/crates.io-index");
        assert!(check(&m).is_err());
        let mut m = metadata();
        m["packages"][5]["manifest_path"] = json!("/different/vendor/lopdf/Cargo.toml");
        assert!(check(&m).is_err());
        let mut m = metadata();
        m["resolve"] = Value::Null;
        assert!(check(&m).is_err());
    }
}
