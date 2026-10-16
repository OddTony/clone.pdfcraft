//! `view --pager` inside tmux, end to end: every page on screen must reach the terminal.
//!
//! tmux throws output away when it decides its terminal can't keep up, and images sent through
//! its passthrough go with it for good (it redraws its text, not what passed through). A screen
//! of fully marked placeholder cells was enough to make it do so, so pages went black after a few
//! flips. This runs `view` in a private tmux server (its own socket, killed at the end) whose
//! client sits on a pseudo-terminal from script(1); what tmux writes there is what a terminal
//! would get. Skipped, with a warning, where tmux or a pseudo-terminal from script(1) isn't
//! available, or tmux can't start: CI machines without them pass instead of failing over
//! something that isn't `view`'s.
#![cfg(unix)]

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_pdfcraft-cli");

/// `line` run by a shell on a new pseudo-terminal: BSD script (macOS, FreeBSD) takes the command
/// after the output file; util-linux script (Linux) takes it as one shell line.
fn script(line: &str) -> Command {
    let mut script = Command::new("script");
    if cfg!(target_os = "linux") {
        script.args(["-q", "-e", "-c", line, "/dev/null"]);
    } else {
        script.args(["-q", "/dev/null", "sh", "-c", line]);
    }
    script
}

/// What this machine lacks for the test, if anything: tmux, or a pseudo-terminal of the size
/// asked for from script(1).
fn missing() -> Option<&'static str> {
    if Command::new("tmux").arg("-V").stdin(Stdio::null()).output().is_err() {
        return Some("no tmux");
    }
    let works = (|| {
        let mut child = script("stty rows 5 cols 7 && stty size").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while child.try_wait().ok()?.is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut out = String::new();
        child.stdout.take()?.read_to_string(&mut out).ok()?;
        Some(out.contains("5 7"))
    })();
    (works != Some(true)).then_some("script(1) can't give a command a pseudo-terminal")
}

/// A US Letter document with `pages` pages, in a directory of the test's own.
fn fixture(test: &str, pages: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfcraft-cli-view-tmux-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let one = dir.join("one.pdf");
    let bytes = pdfcraft_engine::Session::new().create_from_text("Tmux test", "Hello from PdfCraft").unwrap();
    std::fs::write(&one, bytes.as_slice()).unwrap();
    let many = dir.join("many.pdf");
    let mut args = vec!["combine".to_string()];
    args.extend(std::iter::repeat_n(one.to_str().unwrap().to_string(), pages));
    args.extend(["--out".to_string(), many.to_str().unwrap().to_string()]);
    assert!(Command::new(BIN).args(&args).status().unwrap().success());
    many
}

/// A private tmux server, and the script(1) processes its clients run under; all go at the end,
/// the server's socket too (tmux leaves it behind when killed).
struct Server {
    socket: String,
    path: Option<PathBuf>,
    clients: Vec<Child>,
}

impl Server {
    fn tmux(&self, args: &[&str]) -> Vec<u8> {
        Command::new("tmux").arg("-L").arg(&self.socket).args(args).output().map(|o| o.stdout).unwrap_or_default()
    }

    /// A client on a new pseudo-terminal, running `line` (a tmux command); what tmux writes to its
    /// terminal collects in the buffer returned.
    fn client(&mut self, line: &str) -> Arc<Mutex<Vec<u8>>> {
        // Any terminal type tmux knows will do: `view` is told the protocol, and tmux passes the
        // escape codes through whatever it thinks the terminal is. xterm-256color is in every
        // terminfo database; kitty's own entry isn't.
        let line = format!("stty rows 58 cols 200 && exec tmux -L {} -f /dev/null {line}", self.socket);
        let mut client = script(&line)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TERM", "xterm-256color")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = client.stdout.take().unwrap();
        self.clients.push(client);
        let output = Arc::new(Mutex::new(Vec::new()));
        let collected = output.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1 << 16];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                collected.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        output
    }

    /// The image ids the pane's placeholder cells name.
    fn on_screen(&self) -> HashSet<u32> {
        on_screen(&self.tmux(&["capture-pane", "-p", "-e", "-t", "view:0"]))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.tmux(&["kill-server"]);
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
        for client in &mut self.clients {
            let _ = client.kill();
            let _ = client.wait();
        }
    }
}

/// `view` on a 12-page document in a private tmux server, in session `view`, with one client: the
/// server, and what that client's terminal gets. `None`, with a warning, where tmux or script(1)
/// can't run.
fn start(test: &str) -> Option<(Server, Arc<Mutex<Vec<u8>>>)> {
    if let Some(what) = missing() {
        eprintln!("warning: skipped: {what} here");
        return None;
    }
    let pdf = fixture(test, 12);
    let socket = format!("pdfcraft-cli-test-{test}-{}", std::process::id());
    let view = format!("'{BIN}' view '{}' --protocol kitty", pdf.display());
    let mut server = Server { socket, path: None, clients: Vec::new() };
    // A wide window: two pages side by side, and a screenful of placeholder cells per flip.
    let output = server.client(&format!("new-session -s view \"{view}\""));
    // A tmux that can't start here (say, a terminal type it doesn't know) isn't `view`'s failure.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !Command::new("tmux").arg("-L").arg(&server.socket).arg("has-session").output().is_ok_and(|o| o.status.success()) {
        if Instant::now() > deadline {
            eprintln!("warning: skipped: tmux didn't start here");
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let path = PathBuf::from(String::from_utf8_lossy(&server.tmux(&["display-message", "-p", "#{socket_path}"])).trim());
    server.path = path.file_name().is_some_and(|name| *name == *server.socket).then_some(path);
    // The first pages on screen.
    let deadline = Instant::now() + Duration::from_secs(60);
    while arrived(&output.lock().unwrap()).len() < 2 {
        assert!(Instant::now() < deadline, "no page arrived");
        std::thread::sleep(Duration::from_millis(50));
    }
    Some((server, output))
}

/// Wait until every image on the pane's screen has arrived in `output`.
fn all_arrive(server: &Server, output: &Mutex<Vec<u8>>, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let shown = server.on_screen();
        let missing: Vec<u32> = shown.difference(&arrived(&output.lock().unwrap())).copied().collect();
        if !shown.is_empty() && missing.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: images on screen that never reached the terminal: {missing:?} (tmux discarded {} bytes)",
            String::from_utf8_lossy(&server.tmux(&["display", "-p", "#{client_discarded}"])).trim()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The ids of the images whose uploads arrived whole in `out`: from a first chunk (`a=t`) to
/// its last (`m=0`), with nothing else in between.
fn arrived(out: &[u8]) -> HashSet<u32> {
    let text = String::from_utf8_lossy(out);
    let mut ids = HashSet::new();
    let mut upload = None;
    for code in text.split("\x1b_G").skip(1) {
        let control = code.split([';', '\x1b']).next().unwrap_or_default();
        let keys: Vec<(&str, &str)> = control.split(',').filter_map(|kv| kv.split_once('=')).collect();
        let get = |k: &str| keys.iter().find(|(key, _)| *key == k).map(|(_, v)| *v);
        if get("a") == Some("t") {
            upload = get("i").and_then(|i| i.parse::<u32>().ok());
        } else if !keys.iter().all(|(k, _)| matches!(*k, "m" | "q")) {
            upload = None;
        }
        if get("m") == Some("0")
            && let Some(id) = upload.take()
        {
            // As the screen's colours name it: the low 24 bits, or the low 8 in the palette
            // (the high byte travels in a diacritic, not in the colour).
            ids.extend([id & 0xFF_FFFF, id & 0xFF]);
        }
    }
    ids
}

/// The image ids the placeholder cells on screen name, by their 24-bit or palette colour.
fn on_screen(capture: &[u8]) -> HashSet<u32> {
    let text = String::from_utf8_lossy(capture);
    let mut ids = HashSet::new();
    for (at, _) in text.match_indices("\u{10EEEE}") {
        let before = &text[..at];
        let Some(sgr) = before.rfind("\x1b[") else { continue };
        let params: Vec<u32> = before[sgr + 2..].trim_end_matches('m').split(';').filter_map(|p| p.parse().ok()).collect();
        if let Some(i) = params.windows(5).position(|w| w[0] == 38 && w[1] == 2) {
            ids.insert(params[i + 2] << 16 | params[i + 3] << 8 | params[i + 4]);
        } else if let Some(i) = params.windows(3).position(|w| w[0] == 38 && w[1] == 5) {
            ids.insert(params[i + 2]);
        }
    }
    ids
}

#[test]
fn inside_tmux_every_page_on_screen_reaches_the_terminal() {
    let Some((server, output)) = start("flips") else { return };
    for _ in 0..5 {
        server.tmux(&["send-keys", "-t", "view:0", " "]);
        std::thread::sleep(Duration::from_millis(400));
    }
    // Every image the screen shows now must have reached the terminal.
    all_arrive(&server, &output, "after the flips");
    server.tmux(&["send-keys", "-t", "view:0", "q"]);
}

#[test]
fn pages_turned_while_the_pane_is_hidden_arrive_once_it_shows() {
    let Some((server, output)) = start("hidden") else { return };
    // Another window in front: tmux throws away what goes through passthrough from the pane.
    server.tmux(&["new-window", "-t", "view", "sleep 600"]);
    std::thread::sleep(Duration::from_millis(300));
    // To the last pages, which weren't sent ahead: rendered and sent while the pane is hidden.
    server.tmux(&["send-keys", "-t", "view:0", "G"]);
    std::thread::sleep(Duration::from_millis(1500));
    server.tmux(&["select-window", "-t", "view:0"]);
    all_arrive(&server, &output, "after the pane showed again");
    server.tmux(&["send-keys", "-t", "view:0", "q"]);
}

#[test]
fn a_terminal_that_attaches_later_gets_the_pages_too() {
    let Some((mut server, _)) = start("second") else { return };
    // A second client on the same session: tmux sends it what goes through passthrough from now
    // on, and the pages were sent before.
    let second = server.client("attach-session -t view");
    all_arrive(&server, &second, "in the second terminal");
    server.tmux(&["send-keys", "-t", "view:0", "q"]);
}
