//! Readable native preferences, with a private working copy for eframe's compact serializer.
//!
//! Keep eframe's string map and our JSON schema: old installations and older binaries read the
//! same values. This is presentation, not a preference migration.

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::rc::Rc;

// Settings can contain a signature image. Skip unusually large files rather than allocating
// from an unchecked file length; indentation also has a separate, bounded output budget.
const MAX_INPUT: u64 = 16 * 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

pub struct Store {
    path: std::path::PathBuf,
    keys: Vec<String>,
    working: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

impl Store {
    pub fn prepare(path: &Path) -> std::io::Result<Self> {
        make_readable(path)?;
        let bytes = match read_bounded(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
            Err(e) => return Err(e),
        };
        let values: BTreeMap<String, String> = ron::de::from_bytes(&bytes).map_err(std::io::Error::other)?;
        let mut keys: std::collections::BTreeSet<String> = values.into_keys().collect();
        // eframe 0.36 writes these two framework keys before it calls App::save. Keep every
        // existing key too, so legacy and unknown storage values survive.
        keys.extend(["window", "egui", "pdfcraft"].map(str::to_string));
        let directory = tempfile::Builder::new().prefix("pdfcraft-preferences-").tempdir()?;
        let working = directory.path().join("app.ron");
        std::fs::write(&working, bytes)?;
        Ok(Self { path: path.to_path_buf(), keys: keys.into_iter().collect(), working, _directory: directory })
    }

    pub fn working_path(&self) -> &Path {
        &self.working
    }

    fn save(&self, storage: &dyn eframe::Storage) -> std::io::Result<()> {
        let values: BTreeMap<String, String> = self.keys.iter().filter_map(|key| storage.get_string(key).map(|value| (key.clone(), value))).collect();
        let formatted = formatting_budget(&values).and_then(|()| readable_values(values.clone()));
        if let Err(e) = &formatted {
            // A newly saved large signature must still persist, even if it exceeds the
            // prettification budget. The compact representation is the existing format.
            log::warn!("saving compact preferences because formatting failed: {e}");
        }
        let before = target_metadata(&self.path)?;
        let parent = self.path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(parent)?;
        publish(
            &self.path,
            before.as_ref().map(std::fs::Metadata::permissions),
            |file| match &formatted {
                Ok(bytes) => file.write_all(bytes),
                Err(_) => {
                    let mut writer = std::io::BufWriter::new(file);
                    ron::Options::default()
                        .to_io_writer_pretty(&mut writer, &values, ron::ser::PrettyConfig::default())
                        .map_err(std::io::Error::other)?;
                    writer.flush()
                }
            },
            || {
                let after = target_metadata(&self.path)?;
                let stamp = |m: &std::fs::Metadata| (m.len(), m.modified().ok());
                if before.as_ref().map(stamp) != after.as_ref().map(stamp) {
                    return Err(std::io::Error::other("preferences changed during saving"));
                }
                Ok(())
            },
        )
    }
}

pub fn wrap(app: pdfcraft_ui_egui::PdfCraftApp, store: Rc<Store>) -> Box<dyn eframe::App> {
    Box::new(NativeApp { app, store })
}

struct NativeApp {
    app: pdfcraft_ui_egui::PdfCraftApp,
    store: Rc<Store>,
}

impl eframe::App for NativeApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, input);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
        // Cocoa's terminate: does not return through run_native. Publish synchronously here,
        // before termination; eframe's later background flush only touches the working copy.
        if let Err(e) = self.store.save(storage) {
            log::warn!("could not save preferences at {}: {e}", self.store.path.display());
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        self.app.auto_save_interval()
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.clear_color(visuals)
    }

    fn persist_egui_memory(&self) -> bool {
        self.app.persist_egui_memory()
    }
}

fn target_metadata(path: &Path) -> std::io::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.permissions().readonly() => Ok(Some(metadata)),
        Ok(_) => Err(std::io::Error::other("preferences are not a writable regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn make_readable(path: &Path) -> std::io::Result<()> {
    let Some(metadata) = target_metadata(path)? else { return Ok(()) };
    let original = read_bounded(path)?;
    let formatted = readable_text(&original)?;
    if formatted == original {
        return Ok(());
    }
    replace(path, &original, &formatted, metadata.permissions())
}

fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT {
        return Err(std::io::Error::other("preferences exceed the 16 MiB formatting limit"));
    }
    Ok(bytes)
}

fn readable_text(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let values: BTreeMap<String, String> = ron::de::from_bytes(bytes).map_err(std::io::Error::other)?;
    readable_values(values)
}

fn readable_values(mut values: BTreeMap<String, String>) -> std::io::Result<Vec<u8>> {
    formatting_budget(&values)?;
    for key in ["pdfcraft", "printcraft"] {
        if let Some(json) = values.get_mut(key) {
            let value: serde_json::Value = serde_json::from_str(json).map_err(std::io::Error::other)?;
            if !value.is_object() {
                return Err(std::io::Error::other("preferences must be a JSON object"));
            }
            let mut pretty = Bounded(Vec::new());
            serde_json::to_writer_pretty(&mut pretty, &value).map_err(std::io::Error::other)?;
            *json = String::from_utf8(pretty.0).map_err(std::io::Error::other)?;
        }
    }
    let mut output = Bounded(Vec::new());
    // JSON whitespace alone would become literal `\n` escapes in eframe's RON string. Raw
    // strings keep physical lines and let RON choose enough hashes for any embedded delimiter.
    ron::Options::default()
        .to_io_writer_pretty(&mut output, &values, ron::ser::PrettyConfig::default().escape_strings(false))
        .map_err(std::io::Error::other)?;
    Ok(output.0)
}

fn formatting_budget(values: &BTreeMap<String, String>) -> std::io::Result<()> {
    let size = values.iter().try_fold(0usize, |sum, (key, value)| sum.checked_add(key.len())?.checked_add(value.len()));
    if size.is_none_or(|size| size > MAX_INPUT as usize) {
        return Err(std::io::Error::other("preferences exceed the 16 MiB formatting limit"));
    }
    Ok(())
}

fn replace(path: &Path, original: &[u8], formatted: &[u8], permissions: std::fs::Permissions) -> std::io::Result<()> {
    publish(
        path,
        Some(permissions),
        |file| file.write_all(formatted),
        || {
            // Refuse an observed change; eframe itself does not lock concurrent writers.
            if read_bounded(path)? != original {
                return Err(std::io::Error::other("preferences changed during formatting"));
            }
            Ok(())
        },
    )
}

fn publish(
    path: &Path,
    permissions: Option<std::fs::Permissions>,
    write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
    unchanged: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    // tempfile creates private, unpredictable, exclusive staging files and removes them on
    // failure. Keep the target's permissions before writing any preference data.
    let mut file = tempfile::Builder::new().prefix(".app.ron-format-").tempfile_in(parent)?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }
    write(file.as_file_mut())?;
    file.as_file().sync_all()?;
    unchanged()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

struct Bounded(Vec<u8>);
impl std::io::Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_OUTPUT.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("formatted preferences exceed the 64 MiB limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Memory(BTreeMap<String, String>);
    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn app_save_publishes_without_waiting_for_the_native_loop_to_return() {
        let fixture = Fixture::new();
        let path = fixture.0.join("app.ron");
        std::fs::write(&path, saved("printcraft", r#"{"author":"Synthetic contributor","theme":"Dark"}"#)).unwrap();
        let store = Rc::new(Store::prepare(&path).unwrap());
        assert_ne!(store.working_path(), path);
        let mut memory = Memory(ron::de::from_bytes(&std::fs::read(store.working_path()).unwrap()).unwrap());
        memory.0.insert("window".to_string(), "new window size".to_string());
        memory.0.insert("egui".to_string(), "new window memory".to_string());
        let mut app = pdfcraft_ui_egui::PdfCraftApp::new();
        app.restore(&memory.0["printcraft"]);
        let mut native = NativeApp { app, store };
        for theme in ["light", "dark"] {
            native.app.set_option("theme", theme).unwrap();
            eframe::App::save(&mut native, &mut memory);
            let bytes = std::fs::read(&path).unwrap();
            let loaded: BTreeMap<String, String> = ron::de::from_bytes(&bytes).unwrap();
            let current: serde_json::Value = serde_json::from_str(&loaded["pdfcraft"]).unwrap();
            assert_eq!(current["theme"].as_str().unwrap().to_lowercase(), theme);
            assert!(std::str::from_utf8(&bytes).unwrap().lines().any(|line| line.trim() == r#""author": "Synthetic contributor","#));
            for key in ["egui", "window", "future_storage"] {
                assert_eq!(loaded[key], memory.0[key]);
            }
            assert!(loaded.contains_key("printcraft"));
        }
    }

    #[test]
    fn first_save_creates_preferences_without_changing_the_location() {
        let fixture = Fixture::new();
        let path = fixture.0.join("new/config/app.ron");
        let store = Store::prepare(&path).unwrap();
        assert!(!path.exists());
        store.save(&Memory(BTreeMap::from([("pdfcraft".to_string(), r#"{"theme":"Dark"}"#.to_string())]))).unwrap();
        let loaded: BTreeMap<String, String> = ron::de::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&loaded["pdfcraft"]).unwrap()["theme"], "Dark");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_new_large_snapshot_still_persists_in_compact_form() {
        let fixture = Fixture::new();
        let path = fixture.0.join("app.ron");
        let store = Store::prepare(&path).unwrap();
        let json = serde_json::json!({"future_large_setting": "a".repeat(MAX_INPUT as usize)}).to_string();
        let memory = Memory(BTreeMap::from([("pdfcraft".to_string(), json)]));
        store.save(&memory).unwrap();
        let loaded: BTreeMap<String, String> = ron::de::from_reader(std::fs::File::open(path).unwrap()).unwrap();
        assert_eq!(loaded, memory.0, "a formatting limit must not discard a saved preference");
    }

    // Same map and default PrettyConfig as eframe 0.36's FileStorage::save_to_disk.
    fn saved(key: &str, json: &str) -> Vec<u8> {
        ron::ser::to_string_pretty(
            &BTreeMap::from([
                (key.to_string(), json.to_string()),
                ("egui".to_string(), "(memory:(),data:{})".to_string()),
                ("window".to_string(), "(inner_size_points:Some((1440.0,920.0)))".to_string()),
                ("future_storage".to_string(), "leave me \\".to_string()),
            ]),
            ron::ser::PrettyConfig::default(),
        )
        .unwrap()
        .into_bytes()
    }

    #[test]
    fn preferences_have_physical_lines_for_each_setting() {
        let original = saved("pdfcraft", r#"{"date_format":"d/m/yyyy","date_language":null,"default_zoom":"100%","javascript":false}"#);
        let formatted = readable_text(&original).unwrap();
        let text = std::str::from_utf8(&formatted).unwrap();
        for setting in [r#""date_format": "d/m/yyyy""#, r#""date_language": null"#, r#""default_zoom": "100%""#, r#""javascript": false"#] {
            assert!(text.lines().any(|line| line.trim().trim_end_matches(',') == setting), "no readable line for {setting}: {text}");
        }
        let old: BTreeMap<String, String> = ron::de::from_bytes(&original).unwrap();
        let new: BTreeMap<String, String> = ron::de::from_bytes(&formatted).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&old["pdfcraft"]).unwrap(),
            serde_json::from_str::<serde_json::Value>(&new["pdfcraft"]).unwrap()
        );
        for key in ["window", "egui", "future_storage"] {
            assert_eq!(old[key], new[key], "eframe and unknown storage values must survive");
        }
    }

    #[test]
    fn legacy_printcraft_and_special_strings_keep_their_values() {
        let value = serde_json::json!({
            "author": "Test \"name\" \\ path\nnext line\tПривіт",
            "recent": [{"name": "r###\"closing\"###.pdf", "path": "C:\\PDFs\\demo.pdf"}],
            "future_setting": {"nested": [true, null, u64::MAX]},
        });
        for key in ["pdfcraft", "printcraft"] {
            let formatted = readable_text(&saved(key, &value.to_string())).unwrap();
            let loaded: BTreeMap<String, String> = ron::de::from_bytes(&formatted).unwrap();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&loaded[key]).unwrap(), value);
            let mut app = pdfcraft_ui_egui::PdfCraftApp::new();
            app.restore(&loaded[key]);
            let restored: serde_json::Value = serde_json::from_str(&app.persist()).unwrap();
            assert_eq!(restored["author"], value["author"]);
            assert_eq!(readable_text(&formatted).unwrap(), formatted, "formatting is idempotent");
        }
    }

    #[test]
    fn legacy_preferences_restore_then_save_in_the_readable_format() {
        let legacy =
            r#"{"author":"Synthetic contributor","language":"en","theme":"Dark","default_zoom":"150%","default_mode":"edit","javascript":false}"#;
        let loaded: BTreeMap<String, String> = ron::de::from_bytes(&saved("printcraft", legacy)).unwrap();
        let mut app = pdfcraft_ui_egui::PdfCraftApp::new();
        app.restore(&loaded["printcraft"]);
        let compact = app.persist();
        let formatted = readable_text(&saved("pdfcraft", &compact)).unwrap();
        let text = std::str::from_utf8(&formatted).unwrap();
        assert!(text.lines().any(|line| line.trim() == r#""default_zoom": "150%","#), "{text}");
        let loaded: BTreeMap<String, String> = ron::de::from_bytes(&formatted).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&loaded["pdfcraft"]).unwrap(),
            serde_json::from_str::<serde_json::Value>(&compact).unwrap(),
            "formatting keeps every saved setting, including per-tool style arrays"
        );
        let mut reopened = pdfcraft_ui_egui::PdfCraftApp::new();
        reopened.restore(&loaded["pdfcraft"]);
        let restored: serde_json::Value = serde_json::from_str(&reopened.persist()).unwrap();
        let legacy: serde_json::Value = serde_json::from_str(legacy).unwrap();
        for (key, value) in legacy.as_object().unwrap() {
            assert_eq!(&restored[key], value, "legacy preference {key} must restore");
        }
    }

    #[test]
    fn file_formatting_keeps_the_schema_and_permissions() {
        let fixture = Fixture::new();
        let path = fixture.0.join("app.ron");
        let original = saved("pdfcraft", r#"{"default_zoom":"100%","javascript":false}"#);
        std::fs::write(&path, &original).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        make_readable(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), readable_text(&original).unwrap());
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn malformed_or_large_files_are_left_alone() {
        let fixture = Fixture::new();
        let path = fixture.0.join("app.ron");
        for original in [b"not RON".to_vec(), saved("pdfcraft", "{broken JSON"), vec![b' '; MAX_INPUT as usize + 1]] {
            std::fs::write(&path, &original).unwrap();
            assert!(make_readable(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
        }
        assert!(make_readable(&fixture.0.join("missing.ron")).is_ok());
    }

    #[test]
    fn a_changed_target_is_not_replaced_and_staging_is_cleaned_up() {
        let fixture = Fixture::new();
        let path = fixture.0.join("app.ron");
        let original = saved("pdfcraft", "{}");
        let changed = saved("pdfcraft", r#"{"author":"another window"}"#);
        std::fs::write(&path, &changed).unwrap();
        let permissions = std::fs::metadata(&path).unwrap().permissions();
        assert!(replace(&path, &original, b"new", permissions).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), changed);
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_readonly_files_are_left_alone() {
        use std::os::unix::{fs::PermissionsExt as _, fs::symlink};
        let fixture = Fixture::new();
        let target = fixture.0.join("original.ron");
        let link = fixture.0.join("app.ron");
        let original = saved("pdfcraft", "{}");
        std::fs::write(&target, &original).unwrap();
        symlink(&target, &link).unwrap();
        assert!(make_readable(&link).is_err());
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(make_readable(&target).is_err());
        assert_eq!(std::fs::read(target).unwrap(), original);
    }

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!("pdfcraft-settings-format-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
