//! Thin adapter from the shared theme-management API to CLI/MCP tools.
#[cfg(not(target_arch = "wasm32"))]
use pdfcraft_themes::ThemeStore;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;

use crate::{Automation, Result, ToolError};

impl Automation {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn theme_call(&self, _tool: &str, _args: &Value) -> Result<Value> {
        Err(ToolError::Theme {
            code: "unsupported_platform".into(),
            message: "Filesystem theme tools require a native host; browser editors use ThemeState directly".into(),
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn theme_call(&self, tool: &str, args: &Value) -> Result<Value> {
        let store_arg = args
            .get("store")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Theme { code: "invalid_args".into(), message: "store must be a file path".into() })?;
        use crate::file_access::Access;
        let invalid_path = |error: ToolError| ToolError::Theme { code: "invalid_path".into(), message: error.to_string() };
        // The store may not exist yet, but every operation reads its current state.
        // Missing-file handling is independent of read/write authority.
        let path = self.resolve_for(store_arg, Access::Read, true).map_err(invalid_path)?;
        let store = ThemeStore::new(&path);
        if !crate::tools::find(tool).is_some_and(|definition| definition.read_only) {
            self.check_roots(store_arg, &path, Access::Write).map_err(invalid_path)?;
            // Mutations open a separate read/write lock file before loading the store.
            let lock = store.lock_path();
            let lock = lock.to_string_lossy();
            self.resolve_for(&lock, Access::Read, true).map_err(invalid_path)?;
            self.resolve_for(&lock, Access::Write, true).map_err(invalid_path)?;
        }
        // A conventional themes.json can read app.ron when no dedicated store exists.
        // Check authority before probing the legacy file, including when it is missing.
        if let Some(legacy) = store.legacy_path() {
            self.resolve_for(&legacy.to_string_lossy(), Access::Read, true).map_err(invalid_path)?;
        }

        let mut operation = args.clone();
        if let Some(object) = operation.as_object_mut() {
            object.remove("store");
        }
        store.execute(tool, &operation).map_err(|e| ToolError::Theme { code: e.code, message: e.message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Automation, Content};

    #[test]
    fn theme_store_path_and_lock_are_confined() {
        let base = std::env::temp_dir().join(format!("pdfcraft-theme-tool-{}", std::process::id()));
        let root = base.join("root");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        let mut automation = Automation::new().with_root(&root).unwrap();
        let outside = base.join("outside.json");
        let result = automation.call("theme_list", &json!({ "store": outside }));
        assert!(matches!(result, Err(ToolError::Theme { code, .. }) if code == "invalid_path"));
        let result = automation.call("theme_list", &json!({ "store": "themes.json" }));
        assert!(matches!(result, Ok(content) if matches!(content.first(), Some(Content::Json(_)))));
        let _ = std::fs::remove_dir_all(base);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn read_only_theme_tools_use_only_read_authority() {
        let base = std::env::temp_dir().join(format!("pdfcraft-theme-read-authority-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("library.json");
        let created = ThemeStore::new(&path).execute("theme_create", &json!({"name":"Read-only fixture", "base_theme_id":"ocean"})).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let roots = crate::file_access::AutomationRoots::new(Some(&base), None).unwrap();
        let mut automation = Automation::new().with_automation_roots(roots);
        for tool in ["theme_list", "theme_inspect", "theme_export"] {
            let mut args = json!({"store": path});
            if tool != "theme_list" {
                args["id"] = created["id"].clone();
            }
            let result = automation.call(tool, &args);
            assert!(result.is_ok(), "{tool} must work with read authority: {result:?}");
        }
        let missing = base.join("missing.json");
        assert!(automation.call("theme_list", &json!({"store": missing})).is_ok());
        assert!(!missing.exists());
        assert!(!ThemeStore::new(&missing).lock_path().exists());
        let result = automation.call("theme_rename", &json!({"store": path, "id": created["id"], "name":"Denied"}));
        assert!(matches!(result, Err(ToolError::Theme { code, .. }) if code == "invalid_path"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn theme_store_reads_require_read_authority_even_when_writes_are_allowed() {
        let base = std::env::temp_dir().join(format!("pdfcraft-theme-write-authority-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let allowed = base.join("store");
        let other = base.join("other");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let path = allowed.join("library.json");
        let created = ThemeStore::new(&path).execute("theme_create", &json!({"name":"Private fixture", "base_theme_id":"ocean"})).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        for read in [None, Some(other.as_path())] {
            let roots = crate::file_access::AutomationRoots::new(read, Some(&allowed)).unwrap();
            let mut automation = Automation::new().with_automation_roots(roots);
            for tool in ["theme_list", "theme_inspect", "theme_export", "theme_rename"] {
                let mut args = json!({"store": path});
                if tool != "theme_list" {
                    args["id"] = created["id"].clone();
                }
                if tool == "theme_rename" {
                    args["name"] = json!("Denied");
                }
                let result = automation.call(tool, &args);
                assert!(matches!(result, Err(ToolError::Theme { code, .. }) if code == "invalid_path"), "{tool} must require read authority");
                assert_eq!(std::fs::read(&path).unwrap(), bytes);
            }
        }
        let roots = crate::file_access::AutomationRoots::new(Some(&allowed), Some(&allowed)).unwrap();
        let mut automation = Automation::new().with_automation_roots(roots);
        assert!(automation.call("theme_rename", &json!({"store": path, "id":created["id"], "name":"Allowed"})).is_ok());
        std::fs::remove_dir_all(base).unwrap();
    }
}
