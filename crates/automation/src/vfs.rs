//! The disk the automation tools read from and write to.
//!
//! On `wasm32-unknown-unknown` (a browser) there is no disk: `std::fs` compiles and then fails on
//! every call. The ~40 reads and writes in the tool table (`doc_open`, `doc_save`,
//! `page_insert_file`, the export outputs…) go through this module instead, which picks one of
//! two backings:
//!
//! * the real disk (`std::fs`, unchanged) — every native build, by default;
//! * an in-memory disk (path → bytes), which a host fills before calling a tool and empties
//!   after. It is on for `wasm32`, and, so the layer can be tested natively, behind the `memfs`
//!   feature.
//!
//! That way the whole tool table works in a browser without touching tool by tool, and nothing
//! changes for a native build.

/// `true` when the disk is the in-memory one. Tools that need a real folder (`doc_batch`, the
/// `--root` confinement) switch themselves off on it, with a message that says why.
pub const IN_MEMORY: bool = cfg!(any(target_arch = "wasm32", feature = "memfs"));

#[cfg(not(any(target_arch = "wasm32", feature = "memfs")))]
mod active {
    use std::io;
    use std::path::Path;

    pub type Reader = std::fs::File;

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }
    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(p, bytes)
    }
    pub fn open(p: &Path) -> io::Result<Reader> {
        std::fs::File::open(p)
    }
    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        std::fs::create_dir_all(p)
    }
    pub fn exists(p: &Path) -> bool {
        p.exists()
    }
    pub fn is_dir(p: &Path) -> bool {
        p.is_dir()
    }
}

#[cfg(any(target_arch = "wasm32", feature = "memfs"))]
mod active {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::{self, Cursor, ErrorKind};
    use std::path::{Component, Path, PathBuf};

    pub type Reader = Cursor<Vec<u8>>;

    #[derive(Default)]
    struct Disk {
        files: BTreeMap<PathBuf, Vec<u8>>,
        folders: BTreeSet<PathBuf>,
    }

    thread_local! {
        // A WebAssembly instance is one thread, and each worker has its own disk.
        static DISK: RefCell<Disk> = RefCell::new(Disk::default());
    }

    /// The path without `.` and `..`, by name only: there is nothing to look up, so nothing is.
    fn normalize(p: &Path) -> PathBuf {
        let mut parts: Vec<Component> = Vec::new();
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    if matches!(parts.last(), Some(Component::Normal(_))) {
                        parts.pop();
                    }
                }
                other => parts.push(other),
            }
        }
        parts.iter().collect()
    }

    fn not_found(p: &Path) -> io::Error {
        io::Error::new(ErrorKind::NotFound, format!("{}: not on the in-memory disk", p.display()))
    }

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        let p = normalize(p);
        DISK.with(|d| d.borrow().files.get(&p).cloned().ok_or_else(|| not_found(&p)))
    }
    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        let p = normalize(p);
        if is_dir(&p) {
            return Err(io::Error::new(ErrorKind::IsADirectory, format!("{}: is a folder", p.display())));
        }
        DISK.with(|d| d.borrow_mut().files.insert(p, bytes.to_vec()));
        Ok(())
    }
    pub fn open(p: &Path) -> io::Result<Reader> {
        read(p).map(Cursor::new)
    }
    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        let p = normalize(p);
        DISK.with(|d| d.borrow_mut().folders.insert(p));
        Ok(())
    }
    pub fn exists(p: &Path) -> bool {
        let p = normalize(p);
        DISK.with(|d| d.borrow().files.contains_key(&p)) || is_dir(&p)
    }
    pub fn is_dir(p: &Path) -> bool {
        let p = normalize(p);
        if p.parent().is_none() {
            return true; // the root
        }
        DISK.with(|d| {
            let d = d.borrow();
            d.folders.contains(&p) || d.files.keys().any(|k| k.starts_with(&p) && *k != p)
        })
    }

    /// Takes a file off the disk (a host clears its input after opening it).
    pub fn remove_file(p: &Path) -> io::Result<()> {
        let p = normalize(p);
        DISK.with(|d| d.borrow_mut().files.remove(&p).map(|_| ()).ok_or_else(|| not_found(&p)))
    }
}

pub use active::{Reader, create_dir_all, exists, is_dir, open, read, write};

#[cfg(any(target_arch = "wasm32", feature = "memfs"))]
pub use active::remove_file;

#[cfg(all(test, feature = "memfs"))]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::Path;

    #[test]
    fn the_memory_disk_keeps_names_and_bytes() {
        write(Path::new("/a/b/x.pdf"), b"one").unwrap();
        assert_eq!(read(Path::new("/a/b/x.pdf")).unwrap(), b"one");
        assert_eq!(read(Path::new("/a/./b/../b/x.pdf")).unwrap(), b"one", ". and .. resolve by name");
        let mut text = String::new();
        open(Path::new("/a/b/x.pdf")).unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "one");
        assert!(exists(Path::new("/a/b/x.pdf")));
        assert!(is_dir(Path::new("/a/b")), "a folder exists because a file is inside it");
        assert!(is_dir(Path::new("/")));
        assert!(!is_dir(Path::new("/a/b/x.pdf")));
        assert!(!exists(Path::new("/a/b/y.pdf")));
        assert_eq!(read(Path::new("/a/b/y.pdf")).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn a_folder_is_not_overwritten_by_a_file_and_a_file_can_be_removed() {
        write(Path::new("/p/q/file"), b"1").unwrap();
        assert!(write(Path::new("/p/q"), b"2").is_err());
        create_dir_all(Path::new("/empty")).unwrap();
        assert!(is_dir(Path::new("/empty")) && exists(Path::new("/empty")));
        remove_file(Path::new("/p/q/file")).unwrap();
        assert!(!exists(Path::new("/p/q/file")));
        assert!(remove_file(Path::new("/p/q/file")).is_err());
    }

    #[test]
    fn dot_dot_cannot_climb_above_the_root() {
        write(Path::new("/x/../../../top.pdf"), b"t").unwrap();
        assert_eq!(read(Path::new("/top.pdf")).unwrap(), b"t", "`..` at the root stays at the root, by name");
    }
}
