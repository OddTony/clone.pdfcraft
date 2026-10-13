//! One PdfCraft per Windows sign-in: a launch that only names files hands them to the PdfCraft
//! that is already running, where they open as tabs, and exits (#282, #317).
//!
//! Outlook attachments, Explorer double-clicks and Open With all start `pdfcraft.exe "<file>"`, so
//! until now every file got its own process and window. The first PdfCraft listens on a Unix
//! domain socket (Windows 10 1803 and later) in `%LOCALAPPDATA%\PdfCraft`, or in the data folder of
//! a portable copy. The socket is named after the Windows session (`instance-Console.sock`), so
//! two sign-ins of the same user never open files in each other's windows, and it lives in the
//! user's own profile, so other accounts can't create, replace or reach it: nobody else can
//! intercept a launch or open files in this user's window. The exchange is one line each way, a
//! JSON request naming absolute paths and `ok`, and the running app opens the paths exactly as
//! File ▸ Open would.
//!
//! Every failure falls back to the old behaviour: a launch that can't hand its files over within
//! a few seconds opens its own window. `--new-window`, any other option, or no files at all also
//! starts a window of its own, as before. Other platforms keep their current behaviour (macOS
//! already routes Finder opens to the running app through Apple events).
//!
//! The socket comes from `uds_windows`, already in the lockfile through zbus: `std` has no Unix
//! sockets on Windows, and calling Winsock directly would need `unsafe`, which the workspace
//! forbids.

use std::sync::{Arc, Mutex, PoisonError};
#[cfg(windows)]
use std::time::Duration;

use pdfcraft_ui_egui::{OsEvent, OsEventsFn, ShellAction};

/// Longest request read: one line of JSON, far longer than any real list of paths.
#[cfg_attr(not(windows), allow(dead_code))]
const MAX_REQUEST: u64 = 1 << 20;

/// How long either side waits for the other before giving up.
#[cfg(windows)]
const TIMEOUT: Duration = Duration::from_secs(5);

/// What this launch should do.
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Claim {
    /// The running PdfCraft took the files: exit without a window.
    HandedOff,
    /// This is the first PdfCraft: open a window and accept later launches' files.
    Primary(Server),
    /// Open a window of its own and accept nothing (the behaviour before #282).
    Alone,
}

/// Files that later launches handed over, and how to wake the UI for them.
#[derive(Default)]
struct Inbox {
    events: Vec<OsEvent>,
    wake: Option<Box<dyn Fn() + Send>>,
}

/// The first PdfCraft's end of the socket. A background thread accepts launches until the
/// process exits; the UI picks their files up through [`Server::connect`].
pub struct Server {
    inbox: Arc<Mutex<Inbox>>,
}

impl Server {
    #[cfg_attr(not(windows), allow(dead_code))]
    fn new() -> Self {
        Self { inbox: Arc::default() }
    }

    /// Queue files from another launch and wake the UI.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn deliver(&self, event: OsEvent) -> std::io::Result<()> {
        let mut inbox = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        let paths = event_paths(&event);
        let count = inbox.events.iter().map(|e| event_paths(e).len()).sum::<usize>();
        let bytes = inbox.events.iter().flat_map(event_paths).map(String::len).sum::<usize>();
        if count.saturating_add(paths.len()) > 100 || bytes.saturating_add(paths.iter().map(String::len).sum()) > MAX_REQUEST as usize {
            return Err(std::io::Error::other("the running PdfCraft's file queue is full"));
        }
        match (inbox.events.last_mut(), event) {
            (Some(OsEvent::Open(queued)), OsEvent::Open(paths)) => queued.extend(paths),
            (Some(OsEvent::Shell(a, queued)), OsEvent::Shell(b, paths)) if *a == b => queued.extend(paths),
            (_, event) => inbox.events.push(event),
        }
        if let Some(wake) = &inbox.wake {
            wake();
        }
        Ok(())
    }

    /// The queue the UI drains every frame (`PdfCraftApp::os_events`). Files that arrived before
    /// the window existed are in the first batch. A batch also brings the window forward, as the
    /// new window it replaces would have been.
    pub fn connect(&self, ctx: &egui::Context) -> OsEventsFn {
        let wake = ctx.clone();
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner).wake = Some(Box::new(move || wake.request_repaint()));
        let inbox = Arc::clone(&self.inbox);
        let ctx = ctx.clone();
        Box::new(move || {
            let events = std::mem::take(&mut inbox.lock().unwrap_or_else(PoisonError::into_inner).events);
            if events.is_empty() {
                return Vec::new();
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            events
        })
    }
}

/// The request a launch sends: `{"open":[…]}` and a newline.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn request(paths: &[String]) -> String {
    let mut line = serde_json::json!({ "open": paths }).to_string();
    line.push('\n');
    line
}

fn event_paths(event: &OsEvent) -> &[String] {
    match event {
        OsEvent::Open(paths) | OsEvent::Shell(_, paths) => paths,
        OsEvent::Quit => &[],
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn shell_request(paths: &[String], action: Option<ShellAction>) -> String {
    match action {
        None => request(paths),
        // Old running versions don't recognize this key and refuse the handoff; the new process
        // opens its own window instead of silently treating a Combine/Email request as Open.
        Some(action) => format!("{}\n", serde_json::json!({"shell": action.code(), "paths": paths})),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn parse_event(line: &str) -> Option<OsEvent> {
    if line.len() > MAX_REQUEST as usize {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line.trim_end()).ok()?;
    let object = value.as_object()?;
    let (key, action) = if object.len() == 1 && object.contains_key("open") {
        ("open", None)
    } else if object.len() == 2 && object.contains_key("paths") {
        ("paths", Some(ShellAction::parse(value.get("shell")?.as_str()?)?))
    } else {
        return None;
    };
    let items = value.get(key)?.as_array()?;
    if items.is_empty() || items.len() > 100 {
        return None;
    }
    let paths = items
        .iter()
        .map(|p| p.as_str().filter(|s| !s.is_empty() && s.len() <= 32768 && !s.contains('\0')).map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    Some(match action {
        None => OsEvent::Open(paths),
        Some(action) => OsEvent::Shell(action, paths),
    })
}

/// The paths a request names, or `None` when it isn't a request: anything but one JSON object
/// whose `open` is a non-empty list of non-empty strings.
#[cfg(test)]
pub fn parse_request(line: &str) -> Option<Vec<String>> {
    match parse_event(line)? {
        OsEvent::Open(paths) => Some(paths),
        _ => None,
    }
}

/// File arguments made absolute against this launch's working folder, which the running app
/// doesn't share. A path that can't be made absolute is sent as it is.
pub fn absolute(paths: &[String]) -> Vec<String> {
    paths.iter().map(|p| std::path::absolute(p).map_or_else(|_| p.clone(), |a| a.to_string_lossy().into_owned())).collect()
}

/// Decide what this launch does. `files` are absolute ([`absolute`]); `may_hand_off` is false
/// when the launch asked for anything besides opening files, so it gets its own window.
pub fn claim(files: &[String], may_hand_off: bool, action: Option<ShellAction>) -> Claim {
    #[cfg(windows)]
    {
        match socket_path() {
            Some(path) => claim_action_at(&path, files, may_hand_off, action),
            None => Claim::Alone,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (files, may_hand_off, action);
        Claim::Alone
    }
}

/// The socket in `%LOCALAPPDATA%\PdfCraft` (beside the crash-recovery folder); none when
/// `%LOCALAPPDATA%` is unset.
#[cfg(windows)]
fn socket_path() -> Option<std::path::PathBuf> {
    let name = socket_name(std::env::var("SESSIONNAME").ok().as_deref());
    // %LOCALAPPDATA% is private to the user's sign-in, so other accounts can't receive this
    // user's file paths or send it files to open. A portable copy uses it too (#889), under its
    // own name so it never hands files to an installed copy (which keeps its settings elsewhere).
    // Never the portable folder itself: it may sit where other accounts can write (`C:\Tools`).
    let name = if pdfcraft_ui_egui::portable::data_dir().is_some() { format!("portable-{name}") } else { name };
    std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|d| std::path::PathBuf::from(d).join("PdfCraft").join(name))
}

/// The socket's file name for a Windows session (`SESSIONNAME`: `Console`, `RDP-Tcp#3`…). The
/// same user signed in twice (at the console and over Remote Desktop) gets one socket per session,
/// so a file never opens in a window on the other screen. Only letters, digits and `-` are kept.
#[cfg_attr(not(windows), allow(dead_code))]
fn socket_name(session: Option<&str>) -> String {
    let session: String = session.unwrap_or_default().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(32).collect();
    if session.is_empty() { "instance.sock".to_string() } else { format!("instance-{session}.sock") }
}

/// Held while a launch finds out whether it is the first, so launches that start together (several
/// files selected in Explorer and opened at once) take turns: the first one is listening before
/// the next one looks. Without it a launch could find the socket bound but not yet listening,
/// fail to clear it ("Access is denied") and open a window of its own. The lock is on a file
/// beside the socket, which is never deleted; Windows releases it when the process ends, however
/// it ends. Best effort: without the lock the launch goes on as before.
#[cfg(windows)]
fn election(socket: &std::path::Path) -> Option<std::fs::File> {
    if let Some(dir) = socket.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(socket.with_extension("lock")).ok()?;
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                log::info!("single instance: no election lock ({e})");
                return None;
            }
        }
    }
}

/// [`claim`] with the socket at `path`.
#[cfg(all(windows, test))]
fn claim_at(path: &std::path::Path, files: &[String], may_hand_off: bool) -> Claim {
    claim_action_at(path, files, may_hand_off, None)
}

#[cfg(windows)]
fn claim_action_at(path: &std::path::Path, files: &[String], may_hand_off: bool, action: Option<ShellAction>) -> Claim {
    use uds_windows::{UnixListener, UnixStream};
    // Released when this function returns: by then this launch listens, or has handed over.
    let _election = election(path);
    // Two launches can start together: the one that loses the race to bind hands over to the
    // one that won. A third round is for a socket that vanishes in between.
    for _ in 0..3 {
        match UnixStream::connect(path) {
            Ok(stream) => {
                if !may_hand_off {
                    return Claim::Alone;
                }
                return match hand_off(&stream, files, action) {
                    Ok(()) => Claim::HandedOff,
                    Err(e) => {
                        log::warn!("couldn't hand the files to the running PdfCraft ({e}); opening a new window");
                        Claim::Alone
                    }
                };
            }
            // Nobody is listening: no socket yet, or one left by a PdfCraft that didn't exit
            // cleanly (connecting to it is refused). Clear it and become the first. Removed
            // without asking `path.exists()` first: on Windows that follows the socket's reparse
            // point, which can fail, and a leftover socket would then block every later start.
            Err(_) => match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                // Another launch has bound the socket and is about to listen: try it again.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(e) => {
                    log::info!("single instance off: can't clear {} ({e})", path.display());
                    return Claim::Alone;
                }
            },
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match UnixListener::bind(path) {
            Ok(listener) => return serve(listener),
            // Another launch bound it first: hand over to that one on the next round.
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
            // Older Windows, a path too long for a socket address, a file system without socket
            // support (some network or removable drives): this launch works as before.
            Err(e) => {
                log::info!("single instance off: can't listen at {} ({e})", path.display());
                return Claim::Alone;
            }
        }
    }
    Claim::Alone
}

/// Send `files` over `stream` and wait for the running app's `ok`.
#[cfg(windows)]
fn hand_off(stream: &uds_windows::UnixStream, files: &[String], action: Option<ShellAction>) -> std::io::Result<()> {
    use std::io::{BufRead, Read, Write};
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut writer = stream;
    writer.write_all(shell_request(files, action).as_bytes())?;
    let mut reply = String::new();
    std::io::BufReader::new(stream.take(16)).read_line(&mut reply)?;
    if reply == "ok\n" { Ok(()) } else { Err(std::io::Error::other("the running PdfCraft didn't confirm")) }
}

/// Accept launches on a background thread for as long as the process runs.
#[cfg(windows)]
fn serve(listener: uds_windows::UnixListener) -> Claim {
    let server = Server::new();
    let inbox = Server { inbox: Arc::clone(&server.inbox) };
    let started = std::thread::Builder::new().name("single-instance".into()).spawn(move || {
        for stream in listener.incoming() {
            let result = stream.and_then(|s| receive(&s, &inbox));
            if let Err(e) = result {
                log::warn!("single instance: a launch's request failed ({e})");
                // Don't spin if accepting keeps failing.
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    match started {
        Ok(_) => Claim::Primary(server),
        Err(e) => {
            log::warn!("single instance off: no listener thread ({e})");
            Claim::Alone
        }
    }
}

/// Read one launch's request, queue its files and confirm.
#[cfg(windows)]
fn receive(stream: &uds_windows::UnixStream, server: &Server) -> std::io::Result<()> {
    use std::io::{BufRead, Read, Write};
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut line = String::new();
    std::io::BufReader::new(stream.take(MAX_REQUEST)).read_line(&mut line)?;
    // A launch that wanted its own window only checked that this one is running.
    if line.is_empty() {
        return Ok(());
    }
    let event = parse_event(&line).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "not a file request"))?;
    server.deliver(event)?;
    let mut writer = stream;
    writer.write_all(b"ok\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_names_the_paths_and_round_trips() {
        let paths = vec![r"C:\Users\me\Documents\Quote.pdf".to_string(), r"D:\Scans\Plan „A“ 1.pdf".to_string()];
        let line = request(&paths);
        assert!(line.ends_with('\n') && !line.trim_end().contains('\n'), "one line");
        assert_eq!(parse_request(&line), Some(paths));
    }

    #[test]
    fn anything_but_a_list_of_paths_is_refused() {
        for line in ["", "ok\n", "{}", r#"{"open":[]}"#, r#"{"open":[""]}"#, r#"{"open":["a.pdf",3]}"#, r#"{"open":"a.pdf"}"#, "[\"a.pdf\"]"] {
            assert_eq!(parse_request(line), None, "{line:?}");
        }
    }

    #[test]
    fn typed_requests_are_bounded_and_never_become_plain_opens() {
        let paths = vec![r"C:\A & B\quotation.txt".into(), r"D:\日本語\two.png".into()];
        for action in [ShellAction::Convert, ShellAction::Email, ShellAction::Combine] {
            let line = shell_request(&paths, Some(action));
            assert_eq!(parse_event(&line), Some(OsEvent::Shell(action, paths.clone())));
            assert_eq!(parse_request(&line), None, "old open-only protocol must refuse it");
        }
        assert!(parse_event(&shell_request(&vec!["a".into(); 101], Some(ShellAction::Email))).is_none());
        assert!(parse_event(r#"{"shell":"send","paths":["a"]}"#).is_none());
        assert!(parse_event(r#"{"shell":"email","paths":["a"],"open":["b"]}"#).is_none());
        assert!(parse_event(r#"{"open":["a\u0000b"]}"#).is_none());
    }

    #[test]
    fn queue_preserves_action_boundaries_and_refuses_overflow_before_acknowledging() {
        let server = Server::new();
        let mut poll = server.connect(&egui::Context::default());
        server.deliver(OsEvent::Shell(ShellAction::Combine, vec!["one.pdf".into()])).unwrap();
        server.deliver(OsEvent::Shell(ShellAction::Combine, vec!["two.pdf".into()])).unwrap();
        server.deliver(OsEvent::Open(vec!["view.pdf".into()])).unwrap();
        server.deliver(OsEvent::Shell(ShellAction::Email, vec!["mail.txt".into()])).unwrap();
        let events = poll();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], OsEvent::Shell(ShellAction::Combine, p) if p.len()==2));
        assert!(matches!(&events[1], OsEvent::Open(_)));
        assert!(matches!(&events[2], OsEvent::Shell(ShellAction::Email, _)));
        server.deliver(OsEvent::Open(vec!["x.pdf".into(); 100])).unwrap();
        assert!(server.deliver(OsEvent::Open(vec!["overflow.pdf".into()])).is_err());
        assert_eq!(event_paths(&poll()[0]).len(), 100);
    }

    #[cfg(windows)]
    #[test]
    fn simultaneous_explorer_combines_arrive_in_one_review_list() {
        let path = socket("shell-combine");
        let Claim::Primary(server) = claim_action_at(&path, &[], true, Some(ShellAction::Combine)) else { panic!("listen") };
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let launches: Vec<_> = (0..8)
            .map(|i| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    assert!(matches!(claim_action_at(&path, &[format!("C:\\{i}.txt")], true, Some(ShellAction::Combine)), Claim::HandedOff));
                })
            })
            .collect();
        for launch in launches {
            launch.join().unwrap();
        }
        let mut poll = server.connect(&egui::Context::default());
        let events = poll();
        let [OsEvent::Shell(ShellAction::Combine, paths)] = events.as_slice() else { panic!("one combine batch") };
        assert_eq!(paths.len(), 8);
        assert_eq!(paths.iter().collect::<std::collections::HashSet<_>>().len(), 8);
    }

    #[test]
    fn relative_paths_are_made_absolute_against_this_launch() {
        let out = absolute(&["Quote.pdf".to_string()]);
        assert_eq!(out, vec![std::env::current_dir().unwrap().join("Quote.pdf").to_string_lossy().into_owned()]);
        let already = std::env::current_dir().unwrap().join("x.pdf").to_string_lossy().into_owned();
        assert_eq!(absolute(std::slice::from_ref(&already)), vec![already]);
    }

    #[test]
    fn each_windows_session_gets_its_own_socket_name() {
        assert_eq!(socket_name(Some("Console")), "instance-Console.sock");
        assert_eq!(socket_name(Some("RDP-Tcp#3")), "instance-RDP-Tcp3.sock");
        assert_ne!(socket_name(Some("RDP-Tcp#3")), socket_name(Some("RDP-Tcp#4")));
        assert_eq!(socket_name(Some(r"..\x/y:z")), "instance-xyz.sock", "never a path");
        assert_eq!(socket_name(None), "instance.sock");
        assert_eq!(socket_name(Some("")), "instance.sock");
    }

    #[test]
    fn delivered_files_reach_the_ui_once_wake_it_and_bring_the_window_forward() {
        let server = Server::new();
        // Before the window exists: queued, nothing to wake yet.
        server.deliver(OsEvent::Open(vec!["a.pdf".into()])).unwrap();
        let ctx = egui::Context::default();
        let mut poll = server.connect(&ctx);
        server.deliver(OsEvent::Open(vec!["b.pdf".into()])).unwrap();
        let events = poll();
        assert!(matches!(events.as_slice(), [OsEvent::Open(p)] if p == &["a.pdf", "b.pdf"]));
        assert!(poll().is_empty(), "each file is delivered once");
    }

    #[cfg(windows)]
    fn socket(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfcraft-instance-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(socket_name(None))
    }

    #[cfg(windows)]
    #[test]
    fn a_second_launch_hands_its_files_to_the_first() {
        let path = socket("hand-off");
        let Claim::Primary(first) = claim_at(&path, &[], true) else { panic!("the first launch listens") };
        let mut poll = first.connect(&egui::Context::default());
        assert!(matches!(claim_at(&path, &["C:\\a.pdf".into()], true), Claim::HandedOff));
        assert!(matches!(poll().as_slice(), [OsEvent::Open(p)] if p == &["C:\\a.pdf"]));
        // A launch with options keeps its own window and doesn't take over the socket.
        assert!(matches!(claim_at(&path, &["C:\\b.pdf".into()], false), Claim::Alone));
        assert!(poll().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn launches_started_together_share_one_window() {
        // Several files selected in Explorer and opened with Enter: one process per file, all at once.
        let path = socket("together");
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let launches: Vec<_> = (0..8)
            .map(|i| {
                let (path, barrier) = (path.clone(), Arc::clone(&barrier));
                std::thread::spawn(move || {
                    barrier.wait();
                    claim_at(&path, &[format!("C:\\{i}.pdf")], true)
                })
            })
            .collect();
        // The first launch's listener lives until every other launch has handed over.
        let claims: Vec<Claim> = launches.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(claims.iter().filter(|c| matches!(c, Claim::Primary(_))).count(), 1, "one window");
        assert_eq!(claims.iter().filter(|c| matches!(c, Claim::HandedOff)).count(), 7, "the rest hand over");
    }

    #[cfg(windows)]
    #[test]
    fn a_socket_left_by_a_crashed_instance_is_replaced() {
        let path = socket("stale");
        // Bind and drop: the file stays, nobody listens.
        drop(uds_windows::UnixListener::bind({
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            &path
        }));
        // `symlink_metadata`, not `exists`, which follows the socket's reparse point and can fail.
        assert!(std::fs::symlink_metadata(&path).is_ok(), "the socket file stays behind");
        assert!(matches!(claim_at(&path, &["C:\\a.pdf".into()], true), Claim::Primary(_)));
    }
}
