//! macOS: "Combine files in PdfCraft" in Finder's Quick Actions (right-click ▸ Quick Actions),
//! the Mac half of the file manager's "Combine files" entry (#585; Explorer and Linux file
//! managers start `pdfcraft --combine` with the files).
//!
//! Finder hands a service its files through Apple events, not arguments, and a Services
//! provider needs Objective-C classes (`unsafe`, which this workspace forbids); `fmv_macos_events`
//! delivers only Open and Quit. So the app installs an Automator Quick Action in
//! `~/Library/Services` whose shell step writes the selected paths, one a line, to
//! `files.pdfcraft-combine` in a fresh `pdfcraft-combine.*` folder in the user's temporary folder
//! and opens that file with this app (`open -b`). The open arrives as an ordinary Apple event,
//! whether or not the app is running, and `from_open` turns it into the same `Combine` event a
//! Windows or Linux launch makes. Nothing is combined until Combine is pressed.
//!
//! The list is trusted only where the Quick Action writes it: that exact file name, in a
//! `pdfcraft-combine.*` folder directly inside the temporary folder (canonical paths compared), at
//! most a megabyte; it is removed once read. Any other file opens as a document, as before.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};

use pdfcraft_ui_egui::OsEvent;

/// The list file's name, and the start of its folder's name.
const LIST: &str = "files.pdfcraft-combine";
const DIR: &str = "pdfcraft-combine.";
/// The longest list read: far more than any selection's paths.
const MAX_LIST: u64 = 1 << 20;
/// The Quick Action's name in Finder's menu and on disk.
pub const NAME: &str = "Combine files in PdfCraft";

/// An opened Finder list (in the temporary folder) becomes a `Combine` of its files, and the
/// list is removed; anything else opens as before.
pub fn from_open(event: OsEvent) -> Vec<OsEvent> {
    let OsEvent::Open(paths) = event else { return vec![event] };
    let mut out: Vec<OsEvent> = Vec::new();
    for path in paths {
        match listed(Path::new(&path), &std::env::temp_dir()) {
            Some(files) if !files.is_empty() => match out.last_mut() {
                Some(OsEvent::Combine(list)) => list.extend(files),
                _ => out.push(OsEvent::Combine(files)),
            },
            // An empty or unreadable list combines nothing and opens nothing.
            Some(_) => {}
            None => match out.last_mut() {
                Some(OsEvent::Open(list)) => list.push(path),
                _ => out.push(OsEvent::Open(vec![path])),
            },
        }
    }
    out
}

/// The files a Quick Action list names, when `path` is one: called `files.pdfcraft-combine`,
/// in a `pdfcraft-combine.*` folder directly in `temp`. The list and its folder are removed.
/// `None` for any other file (it opens as usual).
fn listed(path: &Path, temp: &Path) -> Option<Vec<String>> {
    if path.file_name()? != LIST {
        return None;
    }
    let dir = path.parent()?;
    if !dir.file_name()?.to_str()?.starts_with(DIR) {
        return None;
    }
    let canon = |p: &Path| std::fs::canonicalize(p).ok();
    if canon(dir.parent()?)? != canon(temp)? {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    let text = if meta.is_file() && meta.len() <= MAX_LIST { std::fs::read_to_string(path).unwrap_or_default() } else { String::new() };
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_dir(dir);
    Some(text.lines().map(str::trim_end).filter(|l| !l.is_empty()).map(str::to_string).collect())
}

/// The Quick Action's folder: `~/Library/Services/<NAME>.workflow`.
fn workflow_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Services").join(format!("{NAME}.workflow")))
}

/// Write the Quick Action (when it is missing or differs) for the app `bundle_id`, and let
/// the services list know. Failures are logged; the app works without it.
pub fn install(bundle_id: &str) {
    let Some(dir) = workflow_dir() else { return };
    let contents = dir.join("Contents");
    let files = [(contents.join("Info.plist"), info_plist()), (contents.join("document.wflow"), document(bundle_id))];
    if files.iter().all(|(p, text)| std::fs::read_to_string(p).ok().as_deref() == Some(text.as_str())) {
        return;
    }
    // Only a first install turns it on in Finder's menu: someone who unticks it in Quick
    // Actions ▸ Customize keeps it off when a later version rewrites the files.
    let first = !contents.join("document.wflow").exists();
    let written = std::fs::create_dir_all(&contents).and_then(|()| files.iter().try_for_each(|(p, text)| std::fs::write(p, text)));
    match written {
        Ok(()) => {
            if first {
                let _ = std::process::Command::new("/usr/bin/defaults").args(shown_args("pbs")).status();
            }
            // Finder's menu picks the new service up on its own too; this only hurries it.
            let _ = std::process::Command::new("/System/Library/CoreServices/pbs").arg("-update").spawn();
            log::info!("Finder Quick Action installed: {}", dir.display());
        }
        Err(e) => log::warn!("Finder Quick Action not installed ({}): {e}", dir.display()),
    }
}

/// `defaults` arguments that mark the Quick Action as shown in Finder's right-click menu, the
/// Services menu and Finder's preview, as Automator does for one it saves; macOS lists a new
/// one only under Customize otherwise. `domain` is `pbs` (a file path in tests).
fn shown_args(domain: &str) -> Vec<String> {
    vec![
        "write".into(),
        domain.into(),
        "NSServicesStatus".into(),
        "-dict-add".into(),
        format!("\"(null) - {NAME} - runWorkflowAsService\""),
        "{ presentation_modes = { ContextMenu = 1; FinderPreview = 1; ServicesMenu = 1; TouchBar = 0; }; }".into(),
    ]
}

/// The shell step: the selected paths into a fresh list, opened with the app.
fn script(bundle_id: &str) -> String {
    format!(
        "d=$(mktemp -d \"${{TMPDIR:-/tmp}}/{DIR}XXXXXX\") || exit 1\nfor p in \"$@\"; do printf '%s\\n' \"$p\"; done > \"$d/{LIST}\"\nopen -b {bundle_id} \"$d/{LIST}\"\n"
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn info_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSBackgroundColorName</key>
			<string>background</string>
			<key>NSIconName</key>
			<string>NSActionTemplate</string>
			<key>NSMenuItem</key>
			<dict>
				<key>default</key>
				<string>{}</string>
			</dict>
			<key>NSMessage</key>
			<string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict>
				<key>NSApplicationIdentifier</key>
				<string>com.apple.finder</string>
			</dict>
			<key>NSSendFileTypes</key>
			<array>
				<string>com.adobe.pdf</string>
				<string>public.image</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
"#,
        esc(NAME)
    )
}

/// Automator's document: one Run Shell Script step taking the files as arguments, a Quick
/// Action for files in Finder.
fn document(bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key>
	<string>534</string>
	<key>AMApplicationVersion</key>
	<string>2.10</string>
	<key>AMDocumentVersion</key>
	<string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Optional</key>
					<true/>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>AMActionVersion</key>
				<string>2.0.3</string>
				<key>AMApplication</key>
				<array>
					<string>Automator</string>
				</array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key>
					<dict/>
					<key>CheckedForUserDefaultShell</key>
					<dict/>
					<key>inputMethod</key>
					<dict/>
					<key>shell</key>
					<dict/>
					<key>source</key>
					<dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key>
					<string>List</string>
					<key>Types</key>
					<array>
						<string>com.apple.cocoa.string</string>
					</array>
				</dict>
				<key>ActionBundlePath</key>
				<string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key>
				<string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key>
					<string>{}</string>
					<key>CheckedForUserDefaultShell</key>
					<true/>
					<key>inputMethod</key>
					<integer>1</integer>
					<key>shell</key>
					<string>/bin/zsh</string>
					<key>source</key>
					<string></string>
				</dict>
				<key>BundleIdentifier</key>
				<string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key>
				<string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key>
				<false/>
				<key>CanShowWhenRun</key>
				<true/>
				<key>Category</key>
				<array>
					<string>AMCategoryUtilities</string>
				</array>
				<key>Class Name</key>
				<string>RunShellScriptAction</string>
				<key>InputUUID</key>
				<string>5B1E3C1A-7F0E-4E4B-9C3B-2D6A7F1C0A01</string>
				<key>OutputUUID</key>
				<string>5B1E3C1A-7F0E-4E4B-9C3B-2D6A7F1C0A02</string>
				<key>UUID</key>
				<string>5B1E3C1A-7F0E-4E4B-9C3B-2D6A7F1C0A03</string>
				<key>UnlocalizedApplications</key>
				<array>
					<string>Automator</string>
				</array>
				<key>isViewVisible</key>
				<integer>1</integer>
			</dict>
			<key>isViewVisible</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>connectors</key>
	<dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleID</key>
		<string>com.apple.finder</string>
		<key>applicationPath</key>
		<string>/System/Library/CoreServices/Finder.app</string>
		<key>inputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>presentationMode</key>
		<integer>15</integer>
		<key>processesInput</key>
		<false/>
		<key>serviceApplicationBundleID</key>
		<string>com.apple.finder</string>
		<key>serviceApplicationPath</key>
		<string>/System/Library/CoreServices/Finder.app</string>
		<key>serviceInputTypeIdentifier</key>
		<string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key>
		<string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key>
		<false/>
		<key>systemImageName</key>
		<string>NSActionTemplate</string>
		<key>useAutomaticInputType</key>
		<false/>
		<key>workflowTypeIdentifier</key>
		<string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#,
        esc(&script(bundle_id))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_in(temp: &Path, dir: &str, name: &str, text: &str) -> PathBuf {
        let d = temp.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join(name);
        std::fs::write(&f, text).unwrap();
        f
    }

    #[test]
    fn a_finder_list_in_the_temporary_folder_names_the_files_to_combine_and_is_removed() {
        let temp = std::env::temp_dir().join(format!("pdfcraft-finder-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let f = list_in(&temp, "pdfcraft-combine.AbC123", LIST, "/a/b.pdf\n/c/photo.png\n\n/d/a b.pdf\n");
        assert_eq!(listed(&f, &temp), Some(vec!["/a/b.pdf".into(), "/c/photo.png".into(), "/d/a b.pdf".into()]));
        assert!(!f.exists() && !f.parent().unwrap().exists(), "the list and its folder are removed");
        // Anything else is an ordinary file to open, and is left alone.
        for (dir, name) in [("pdfcraft-combine.x", "other.pdf"), ("elsewhere", LIST)] {
            let g = list_in(&temp, dir, name, "/a.pdf\n");
            assert_eq!(listed(&g, &temp), None, "{dir}/{name}");
            assert!(g.exists());
        }
        // Outside the temporary folder: not a list.
        let other = temp.join("not-temp");
        let h = list_in(&other, "pdfcraft-combine.x", LIST, "/a.pdf\n");
        assert_eq!(listed(&h, &temp), None);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn opened_lists_become_one_combine_and_other_files_still_open() {
        let temp = std::env::temp_dir();
        let d = tempdir_in(&temp);
        let list = d.join(LIST);
        std::fs::write(&list, "/x/1.pdf\n/x/2.jpg\n").unwrap();
        let events = from_open(OsEvent::Open(vec!["/y/report.pdf".into(), list.to_string_lossy().into_owned()]));
        assert_eq!(events, vec![OsEvent::Open(vec!["/y/report.pdf".into()]), OsEvent::Combine(vec!["/x/1.pdf".into(), "/x/2.jpg".into()])]);
        assert_eq!(from_open(OsEvent::Quit), vec![OsEvent::Quit]);
    }

    fn tempdir_in(temp: &Path) -> PathBuf {
        let d = temp.join(format!("{DIR}{}-{}", std::process::id(), line!()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The "shown" setting is one `defaults` can write, in the form macOS keeps for a Quick
    /// Action ticked in Customize (written to a scratch file here, never the real `pbs`).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_quick_action_is_marked_as_shown_in_finders_menu() {
        let file = std::env::temp_dir().join(format!("pdfcraft-pbs-test-{}.plist", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let path = file.to_string_lossy().into_owned();
        let ok = std::process::Command::new("/usr/bin/defaults").args(shown_args(&path)).status().unwrap();
        assert!(ok.success());
        let read = std::process::Command::new("/usr/bin/defaults").args(["read", &path, "NSServicesStatus"]).output().unwrap();
        let text = String::from_utf8_lossy(&read.stdout);
        assert!(text.contains("(null) - Combine files in PdfCraft - runWorkflowAsService") && text.contains("ContextMenu = 1"), "{text}");
        let _ = std::fs::remove_file(&file);
    }

    /// The Quick Action's two files are property lists macOS reads, and its step writes a
    /// list `from_open` takes (run with zsh, as Automator does).
    #[cfg(target_os = "macos")]
    #[test]
    fn the_quick_action_is_a_valid_workflow_whose_step_writes_a_list_this_app_reads() {
        let dir = std::env::temp_dir().join(format!("pdfcraft-workflow-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in [("Info.plist", info_plist()), ("document.wflow", document("com.example.app"))] {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            let lint = std::process::Command::new("plutil").arg("-lint").arg(&p).output().unwrap();
            assert!(lint.status.success(), "{name}: {}", String::from_utf8_lossy(&lint.stdout));
        }
        let info = std::fs::read_to_string(dir.join("Info.plist")).unwrap();
        assert!(info.contains("<string>Combine files in PdfCraft</string>") && info.contains("com.adobe.pdf") && info.contains("public.image"));
        // The step, with `open` replaced by printing the list's path.
        let step = script("com.example.app").replace("open -b com.example.app", "printf %s");
        let out = std::process::Command::new("/bin/zsh").arg("-c").arg(&step).arg("zsh").arg("/p/one.pdf").arg("/p/two words.png").output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let list = PathBuf::from(String::from_utf8(out.stdout).unwrap());
        assert_eq!(listed(&list, &std::env::temp_dir()), Some(vec!["/p/one.pdf".into(), "/p/two words.png".into()]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
