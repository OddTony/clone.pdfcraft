//! Isolated, opt-in Linux data-control clipboard integration.
#[cfg(target_os = "linux")]
pub use linux::run;

#[cfg(not(target_os = "linux"))]
pub fn run(_: &[String]) -> anyhow::Result<()> {
    anyhow::bail!("native-wayland-clipboard requires Linux 5.3+ and a data-control compositor")
}

#[cfg(target_os = "linux")]
mod linux {
    //! Opt-in native clipboard check. No peer can connect to the user's compositor.
    use anyhow::{Context, Result, bail, ensure};
    use rustix::{
        event::{PollFd, PollFlags, Timespec, poll},
        fd::OwnedFd,
        fs::{OFlags, fcntl_getfl, fcntl_setfl},
        process::{Pid, PidfdFlags, Signal, kill_process_group, pidfd_open},
    };
    use std::{
        collections::BTreeSet,
        fs::{self, File},
        io::{Read, Write},
        os::unix::{fs::DirBuilderExt, net::UnixStream, process::CommandExt},
        path::PathBuf,
        process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio},
        time::{Duration, Instant},
    };

    const TEST: &str = "create_ui::native_wayland_clipboard_tests::native_wayland_image_clipboard_creates_pdf_and_exports_png";
    const MARKER: &[u8] = b"ARTCRAFT_PDF_EXPORT_WAITING\n";
    const INPUT: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255];
    const EXPORT: [u8; 16] = [0, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255];
    const CAPTURE: usize = 4 * 1024 * 1024;
    const MAX_PEERS: usize = 8;

    #[derive(Default)]
    struct Selection {
        pending: Vec<u8>,
        sent: bool,
        offers: BTreeSet<u32>,
        selected: Option<u32>,
    }
    fn offer_id(text: &str) -> Option<u32> {
        let (_, suffix) = text.split_once("data_control_offer_v1")?;
        let digits = suffix.strip_prefix('#').or_else(|| suffix.strip_prefix('@'))?;
        digits.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
    }
    impl Selection {
        fn feed(&mut self, bytes: &[u8]) -> Result<()> {
            ensure!(self.pending.len().checked_add(bytes.len()).is_some_and(|n| n <= CAPTURE), "oversized clipboard protocol line");
            self.pending.extend_from_slice(bytes);
            let mut consumed = 0;
            while let Some(offset) = self.pending.get(consumed..).context("protocol cursor")?.iter().position(|b| *b == b'\n') {
                let end = consumed.checked_add(offset).context("protocol offset overflow")?;
                let line = String::from_utf8_lossy(self.pending.get(consumed..end).context("protocol boundary")?);
                if line.contains("data_control_device_v1") && line.contains(".set_selection(") {
                    self.sent = true;
                    self.offers.clear();
                    self.selected = None;
                } else if self.sent {
                    if line.contains(".offer(\"image/png\")")
                        && let Some(id) = offer_id(&line)
                    {
                        ensure!(self.offers.len() < 256, "too many clipboard offers");
                        self.offers.insert(id);
                    }
                    if line.contains("data_control_device_v1") && line.contains(".selection(") {
                        self.selected = offer_id(&line);
                    }
                }
                consumed = end.checked_add(1).context("protocol cursor overflow")?;
            }
            self.pending.drain(..consumed);
            Ok(())
        }
        fn ready(&self) -> bool {
            self.sent && self.selected.is_some_and(|id| self.offers.contains(&id))
        }
    }

    fn socket_notification(bytes: &[u8]) -> Option<String> {
        let text = std::str::from_utf8(bytes).ok()?;
        let (_, rest) = text.split_once("Running compositor on wayland display '")?;
        let name = rest.split_once('\'')?.0;
        (name.starts_with("wayland-") && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).then(|| name.to_owned())
    }
    fn exact_listing(bytes: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(bytes)?;
        ensure!(text.lines().filter(|line| *line == format!("{TEST}: test")).count() == 1, "native clipboard test is absent or ambiguous");
        Ok(())
    }
    fn exact_success(bytes: &[u8]) -> bool {
        let text = String::from_utf8_lossy(bytes);
        text.lines().any(|s| s == "running 1 test")
            && text.contains(&format!("test {TEST} ..."))
            && text.contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
    }
    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|p| p == needle)
    }

    struct Peer {
        child: Child,
        pid: Pid,
        notification: Option<OwnedFd>,
        stdout: Option<ChildStdout>,
        stderr: Option<ChildStderr>,
        out: Vec<u8>,
        err: Vec<u8>,
        status: Option<ExitStatus>,
        selection: Selection,
    }
    fn drain<R: Read>(pipe: &mut Option<R>, output: &mut Vec<u8>, other: usize) -> Result<Vec<u8>> {
        let mut added = Vec::new();
        let mut buffer = [0; 8192];
        while let Some(reader) = pipe.as_mut() {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    *pipe = None;
                    break;
                }
                Ok(n) => {
                    ensure!(
                        output.len().checked_add(other).and_then(|m| m.checked_add(n)).is_some_and(|m| m <= CAPTURE),
                        "clipboard peer exceeded capture limit"
                    );
                    let bytes = buffer.get(..n).context("capture boundary")?;
                    output.extend_from_slice(bytes);
                    added.extend_from_slice(bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(added)
    }
    impl Peer {
        fn signal_group(&self) {
            // The direct child remains unreaped until its owned group has been signalled.
            let _ = kill_process_group(self.pid, Signal::KILL);
        }
        fn complete(&self) -> bool {
            self.status.is_some() && self.stdout.is_none() && self.stderr.is_none()
        }
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            if self.status.is_some() {
                return;
            }
            self.signal_group();
            let _ = self.child.kill();
            if let Some(notification) = &self.notification {
                let end = Instant::now() + Duration::from_secs(5);
                loop {
                    let Some(remaining) = end.checked_duration_since(Instant::now()) else {
                        eprintln!("clipboard peer did not exit after forced termination");
                        return;
                    };
                    let Ok(timeout) = Timespec::try_from(remaining) else { return };
                    let mut fds = [PollFd::new(notification, PollFlags::IN)];
                    match poll(&mut fds, Some(&timeout)) {
                        Ok(1) if fds[0].revents().contains(PollFlags::IN) && !fds[0].revents().intersects(PollFlags::ERR | PollFlags::NVAL) => {
                            let _ = self.child.wait();
                            return;
                        }
                        Err(rustix::io::Errno::INTR) => continue,
                        other => {
                            eprintln!("cannot confirm clipboard peer exit: {other:?}");
                            return;
                        }
                    }
                }
            }
            // Only spawn/setup failures can lack a pidfd. Keep their WNOHANG cleanup finite.
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("cannot reap clipboard peer: {e}");
                        return;
                    }
                }
                if Instant::now() >= end {
                    eprintln!("clipboard peer did not exit after forced termination");
                    return;
                }
                let timeout = Timespec::try_from(Duration::from_millis(10));
                if let Ok(timeout) = timeout {
                    let _ = poll(&mut [], Some(&timeout));
                }
            }
        }
    }

    // This dedicated terminal command exits immediately after registration is removed.
    // signal-hook unregisters actions, but does not restore previous/default dispositions.
    struct Cancellation {
        reader: UnixStream,
        registrations: Vec<signal_hook::SigId>,
    }
    impl Cancellation {
        fn new() -> Result<Self> {
            let (reader, writer) = UnixStream::pair()?;
            reader.set_nonblocking(true)?;
            writer.set_nonblocking(true)?;
            let mut cancellation = Self { reader, registrations: Vec::new() };
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                cancellation.registrations.push(signal_hook::low_level::pipe::register(signal, writer.try_clone()?)?);
            }
            Ok(cancellation)
        }
        fn check(&mut self) -> Result<()> {
            let mut byte = [0];
            loop {
                match self.reader.read(&mut byte) {
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                    _ => bail!("native clipboard check cancelled by SIGINT/SIGTERM"),
                }
            }
        }
    }
    impl Drop for Cancellation {
        fn drop(&mut self) {
            for id in self.registrations.drain(..) {
                signal_hook::low_level::unregister(id);
            }
        }
    }

    struct Run {
        peers: Vec<Peer>,
        deadline: Instant,
        cancellation: Option<Cancellation>,
    }
    impl Run {
        fn new(seconds: u64) -> Self {
            Self { peers: Vec::new(), deadline: Instant::now() + Duration::from_secs(seconds), cancellation: None }
        }
        fn spawn(&mut self, command: &mut Command, stdin: Stdio) -> Result<usize> {
            if let Some(cancellation) = &mut self.cancellation {
                cancellation.check()?;
            }
            ensure!(self.peers.len() < MAX_PEERS, "too many native clipboard peers");
            command.process_group(0).stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = command.spawn().with_context(|| format!("starting {command:?}"))?;
            let pid = Pid::from_child(&child);
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let mut peer = Peer {
                child,
                pid,
                notification: None,
                stdout,
                stderr,
                out: Vec::new(),
                err: Vec::new(),
                status: None,
                selection: Selection::default(),
            };
            peer.notification =
                Some(pidfd_open(pid, PidfdFlags::empty()).context("native clipboard check requires pidfd support, Linux 5.3 or later")?);
            if let Some(fd) = &peer.stdout {
                fcntl_setfl(fd, fcntl_getfl(fd)? | OFlags::NONBLOCK)?;
            }
            if let Some(fd) = &peer.stderr {
                fcntl_setfl(fd, fcntl_getfl(fd)? | OFlags::NONBLOCK)?;
            }
            self.peers.push(peer);
            Ok(self.peers.len() - 1)
        }
        fn cleanup(&mut self) -> Result<()> {
            // Signal every group before positively waiting for any unreaped leader.
            for peer in &mut self.peers {
                if peer.status.is_none() {
                    peer.signal_group();
                    let _ = peer.child.kill();
                }
                peer.stdout = None;
                peer.stderr = None;
                peer.child.stdin = None;
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.peers.iter().any(|peer| peer.status.is_none()) {
                let remaining = deadline.checked_duration_since(Instant::now()).context("clipboard peer did not exit during cleanup")?;
                let timeout = Timespec::try_from(remaining)?;
                let mut slots = Vec::new();
                let mut fds = Vec::new();
                for (index, peer) in self.peers.iter().enumerate() {
                    if let Some(fd) = &peer.notification {
                        slots.push(index);
                        fds.push(PollFd::new(fd, PollFlags::IN));
                    }
                }
                ensure!(!fds.is_empty(), "missing owned-peer exit notification");
                match poll(&mut fds, Some(&timeout)) {
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(error) => return Err(error.into()),
                    Ok(0) => bail!("clipboard cleanup deadline exceeded"),
                    Ok(_) => {}
                }
                let events: Vec<_> =
                    fds.iter().zip(slots).filter_map(|(fd, index)| (!fd.revents().is_empty()).then_some((index, fd.revents()))).collect();
                drop(fds);
                for (index, flags) in events {
                    ensure!(
                        flags.contains(PollFlags::IN) && !flags.intersects(PollFlags::ERR | PollFlags::NVAL),
                        "unexpected pidfd event during cleanup: {flags:?}"
                    );
                    let peer = self.peers.get_mut(index).context("missing cleanup peer")?;
                    peer.status = Some(peer.child.wait()?);
                    peer.notification = None;
                }
            }
            Ok(())
        }
        fn get(&self, index: usize) -> Result<&Peer> {
            self.peers.get(index).context("missing clipboard peer")
        }
        fn tick(&mut self, deadline: Instant) -> Result<()> {
            let remaining = deadline.checked_duration_since(Instant::now()).context("native clipboard deadline exceeded")?;
            let timeout = Timespec::try_from(remaining)?;
            let mut fds = Vec::new();
            let mut slots = Vec::new();
            for (index, peer) in self.peers.iter().enumerate() {
                if let Some(fd) = &peer.notification {
                    fds.push(PollFd::new(fd, PollFlags::IN));
                    slots.push((index, 0));
                }
                if let Some(fd) = &peer.stdout {
                    fds.push(PollFd::new(fd, PollFlags::IN));
                    slots.push((index, 1));
                }
                if let Some(fd) = &peer.stderr {
                    fds.push(PollFd::new(fd, PollFlags::IN));
                    slots.push((index, 2));
                }
            }
            if let Some(cancellation) = &self.cancellation {
                fds.push(PollFd::new(&cancellation.reader, PollFlags::IN));
                slots.push((usize::MAX, 3));
            }
            ensure!(!fds.is_empty(), "all clipboard peers exited before the expected notification");
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => bail!("native clipboard notification deadline exceeded"),
                Err(rustix::io::Errno::INTR) => return Ok(()),
                Err(e) => return Err(e.into()),
                Ok(_) => {}
            }
            let events: Vec<_> = fds.iter().zip(slots).filter_map(|(fd, slot)| (!fd.revents().is_empty()).then_some((slot, fd.revents()))).collect();
            drop(fds);
            for ((index, kind), flags) in events {
                if kind == 3 {
                    bail!("native clipboard check cancelled by SIGINT/SIGTERM");
                }
                let peer = self.peers.get_mut(index).context("missing event peer")?;
                match kind {
                    0 => {
                        ensure!(
                            flags.contains(PollFlags::IN) && !flags.intersects(PollFlags::ERR | PollFlags::NVAL),
                            "unexpected pidfd event: {flags:?}"
                        );
                        peer.signal_group();
                        peer.status = Some(peer.child.wait()?); // pidfd positively reported exit.
                        peer.notification = None;
                    }
                    1 => {
                        drain(&mut peer.stdout, &mut peer.out, peer.err.len())?;
                    }
                    _ => {
                        let bytes = drain(&mut peer.stderr, &mut peer.err, peer.out.len())?;
                        peer.selection.feed(&bytes)?;
                    }
                }
            }
            Ok(())
        }
        fn until(&mut self, seconds: u64, required: &[usize], ready: impl Fn(&Self) -> Result<bool>) -> Result<()> {
            let deadline = self.deadline.min(Instant::now() + Duration::from_secs(seconds));
            loop {
                for &index in required {
                    let peer = self.get(index)?;
                    ensure!(
                        peer.status.is_none(),
                        "required clipboard peer exited before readiness: {}",
                        String::from_utf8_lossy(peer.err.get(peer.err.len().saturating_sub(8192)..).unwrap_or_default())
                    );
                }
                if ready(self)? {
                    return Ok(());
                }
                self.tick(deadline)?;
            }
        }
        fn output(&mut self, index: usize, seconds: u64, required: &[usize]) -> Result<Vec<u8>> {
            self.until(seconds, required, |run| Ok(run.get(index)?.complete()))?;
            let peer = self.get(index)?;
            ensure!(peer.status.is_some_and(|s| s.success()), "clipboard peer failed: {}", String::from_utf8_lossy(&peer.err));
            Ok(peer.out.clone())
        }
    }

    impl Drop for Run {
        fn drop(&mut self) {
            if let Err(error) = self.cleanup() {
                eprintln!("native clipboard cleanup failed: {error:#}");
            }
        }
    }

    struct PrivateTree(PathBuf);
    impl PrivateTree {
        fn new() -> Result<Self> {
            for attempt in 0..128 {
                let path = std::env::temp_dir().join(format!("pdfcraft-native-clipboard-{}-{attempt}", std::process::id()));
                match fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            bail!("cannot allocate a private clipboard directory")
        }
        fn configure(&self, command: &mut Command, display: Option<&str>) -> Result<()> {
            for key in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "SWAYSOCK", "I3SOCK", "DBUS_SESSION_BUS_ADDRESS"] {
                command.env_remove(key);
            }
            for (key, name) in [
                ("HOME", "home"),
                ("XDG_RUNTIME_DIR", "runtime"),
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_STATE_HOME", "state"),
            ] {
                let path = self.0.join(name);
                match fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
                command.env(key, path);
            }
            command
                .env("WLR_BACKENDS", "headless")
                .env("WLR_HEADLESS_OUTPUTS", "1")
                .env("WLR_RENDERER", "pixman")
                .env("XDG_SESSION_TYPE", "wayland")
                .env("WAYLAND_DEBUG", "client");
            command.env("PDFCRAFT_NATIVE_CLIPBOARD_RUNTIME", self.0.join("runtime"));
            if let Some(display) = display {
                command.env("WAYLAND_DISPLAY", display);
            }
            Ok(())
        }
    }
    impl Drop for PrivateTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture_png() -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&INPUT)?;
        writer.finish()?;
        Ok(out)
    }
    fn exported_png(bytes: &[u8], expected: &[u8; 16]) -> Result<()> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_limits(png::Limits { bytes: 1024 * 1024 });
        let mut reader = decoder.read_info()?;
        ensure!((reader.info().width, reader.info().height) == (2, 2), "external clipboard image dimensions changed");
        let mut buffer = vec![0; reader.output_buffer_size().context("PNG output size overflow")?.min(1024)];
        let info = reader.next_frame(&mut buffer)?;
        ensure!(
            info.color_type == png::ColorType::Rgba
                && info.bit_depth == png::BitDepth::Eight
                && buffer.get(..info.buffer_size()) == Some(expected.as_slice()),
            "external clipboard pixels/channel order differ"
        );
        Ok(())
    }

    fn cargo_artifact(bytes: &[u8]) -> Result<PathBuf> {
        let mut found = BTreeSet::new();
        for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let message: serde_json::Value = serde_json::from_slice(line).context("invalid Cargo artifact record")?;
            if message.get("reason").and_then(|v| v.as_str()) == Some("compiler-artifact")
                && message.pointer("/target/name").and_then(|v| v.as_str()) == Some("pdfcraft_ui_egui")
                && message.pointer("/profile/test").and_then(|v| v.as_bool()) == Some(true)
                && let Some(path) = message.get("executable").and_then(|v| v.as_str())
            {
                found.insert(PathBuf::from(path));
            }
        }
        ensure!(found.len() == 1, "Cargo must produce one UI libtest executable, found {}", found.len());
        found.into_iter().next().context("missing UI libtest executable")
    }

    pub fn run(args: &[String]) -> Result<()> {
        let mut executable = None;
        let mut tools = ["sway".to_owned(), "wl-copy".to_owned(), "wl-paste".to_owned(), "wayland-info".to_owned()];
        let mut iter = args.iter();
        while let Some(key) = iter.next() {
            if key == "--help" {
                println!(
                    "native-wayland-clipboard [--test-executable FILE] [--sway FILE] [--wl-copy FILE] [--wl-paste FILE] [--wayland-info FILE]\nLinux 5.3+, Sway headless/pixman and data-control required; no user clipboard is used."
                );
                return Ok(());
            }
            let value = iter.next().with_context(|| format!("missing value for {key}"))?;
            match key.as_str() {
                "--test-executable" => executable = Some(fs::canonicalize(value).context("locating UI test executable")?),
                "--sway" => *tools.get_mut(0).context("sway tool")? = value.clone(),
                "--wl-copy" => *tools.get_mut(1).context("wl-copy tool")? = value.clone(),
                "--wl-paste" => *tools.get_mut(2).context("wl-paste tool")? = value.clone(),
                "--wayland-info" => *tools.get_mut(3).context("wayland-info tool")? = value.clone(),
                _ => bail!("unknown native clipboard option {key}"),
            }
        }
        let _preflight =
            pidfd_open(rustix::process::getpid(), PidfdFlags::empty()).context("native clipboard tool requires pidfd, Linux 5.3 or later")?;
        let tree = PrivateTree::new()?;
        let mut run = Run::new(1200);
        run.cancellation = Some(Cancellation::new()?);
        let result = (|| -> Result<()> {
            let executable = match executable {
                Some(path) => path,
                None => {
                    // Compile before starting any compositor; the explicit override avoids compilation.
                    let build = run.spawn(
                        Command::new(env!("CARGO")).current_dir(crate::gates::root()).args([
                            "test",
                            "-p",
                            "pdfcraft-ui-egui",
                            "--lib",
                            "--no-run",
                            "--locked",
                            "--message-format=json-render-diagnostics",
                        ]),
                        Stdio::null(),
                    )?;
                    cargo_artifact(&run.output(build, 1200, &[])?)?
                }
            };
            run.deadline = Instant::now() + Duration::from_secs(90);
            let mut list = Command::new(&executable);
            tree.configure(&mut list, None)?;
            let list = run.spawn(list.args(["--ignored", "--exact", TEST, "--list"]), Stdio::null())?;
            exact_listing(&run.output(list, 15, &[])?)?;
            let config = tree.0.join("sway.conf");
            fs::write(&config, "xwayland disable\noutput * mode 1280x720\nseat seat0 fallback true\n")?;
            let fixture = tree.0.join("input.png");
            let input = fixture_png()?;
            fs::write(&fixture, &input)?;
            let mut sway = Command::new(tools.first().context("sway tool")?);
            tree.configure(&mut sway, None)?;
            let compositor = run.spawn(sway.args(["--verbose", "--unsupported-gpu", "--config"]).arg(&config), Stdio::null())?;
            run.until(15, &[compositor], |run| Ok(socket_notification(&run.get(compositor)?.err).is_some()))
                .context("waiting for isolated Sway startup")?;
            let display = socket_notification(&run.get(compositor)?.err).context("missing running-display notification")?;
            let mut globals = Command::new(tools.get(3).context("wayland-info tool")?);
            tree.configure(&mut globals, Some(&display))?;
            let globals = run.spawn(&mut globals, Stdio::null())?;
            let globals = run.output(globals, 10, &[compositor])?;
            let globals = String::from_utf8_lossy(&globals);
            ensure!(
                (globals.contains("interface: 'ext_data_control_manager_v1'") || globals.contains("interface: 'zwlr_data_control_manager_v1'"))
                    && globals.contains("interface: 'wl_seat'"),
                "isolated compositor lacks data-control manager/seat"
            );
            let mut owner = Command::new(tools.get(1).context("wl-copy tool")?);
            tree.configure(&mut owner, Some(&display))?;
            let owner = run.spawn(owner.args(["--foreground", "--type", "image/png"]), Stdio::from(File::open(&fixture)?))?;
            run.until(15, &[compositor, owner], |run| Ok(run.get(owner)?.selection.ready()))
                .context("waiting for external PNG ownership acknowledgement")?;
            let mut read = Command::new(tools.get(2).context("wl-paste tool")?);
            tree.configure(&mut read, Some(&display))?;
            let read = run.spawn(read.args(["--type", "image/png", "--no-newline"]), Stdio::null())?;
            let received = run.output(read, 10, &[compositor, owner])?;
            exported_png(&received, &INPUT)?;
            let mut test = Command::new(&executable);
            tree.configure(&mut test, Some(&display))?;
            let test = run.spawn(test.args(["--ignored", "--exact", TEST, "--nocapture"]), Stdio::piped())?;
            run.until(30, &[compositor, test], |run| {
                let peer = run.get(test)?;
                Ok(contains(&peer.out, MARKER) && peer.selection.ready())
            })
            .context("waiting for native test export selection")?;
            let mut read = Command::new(tools.get(2).context("wl-paste tool")?);
            tree.configure(&mut read, Some(&display))?;
            let read = run.spawn(read.args(["--type", "image/png", "--no-newline"]), Stdio::null())?;
            exported_png(&run.output(read, 10, &[compositor, test])?, &EXPORT)?;
            let mut ack = run.peers.get_mut(test).and_then(|p| p.child.stdin.take()).context("native test has no handshake pipe")?;
            ack.write_all(b"export-ready\n")?;
            drop(ack);
            let output = run.output(test, 15, &[compositor])?;
            ensure!(exact_success(&output), "UI executable did not run and pass the exact native regression");
            Ok(())
        })();
        let cleanup = run.cleanup();
        match (result, cleanup) {
            (Err(error), Err(cleanup)) => return Err(error.context(format!("cleanup also failed: {cleanup:#}"))),
            (Err(error), _) | (_, Err(error)) => return Err(error),
            _ => {}
        }
        println!("native Wayland PNG import, Create-from-Clipboard and export passed");
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn exact_native_test_is_required() {
            assert!(exact_listing(format!("{TEST}: test\n1 test, 0 benchmarks\n").as_bytes()).is_ok());
            assert!(exact_listing(b"0 tests, 0 benchmarks\n").is_err());
            assert!(exact_listing(format!("{TEST}: test\n{TEST}: test\n").as_bytes()).is_err());
            assert!(!exact_success(b"test result: ok. 0 passed; 0 failed; 0 ignored;"));
            assert!(exact_success(
                format!("running 1 test\ntest {TEST} ... PDFCRAFT_NATIVE_EXPORT\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n").as_bytes()
            ));
        }
        #[test]
        fn ownership_needs_request_png_offer_and_matching_selection() {
            let mut selection = Selection::default();
            selection
                .feed(b"zwlr_data_control_offer_v1#5.offer(\"image/png\")\nzwlr_data_control_device_v1#2.selection(zwlr_data_control_offer_v1#5)\n")
                .unwrap();
            assert!(!selection.ready(), "stale offer before our request");
            selection.feed(b"zwlr_data_control_device_v1#2.set_selection(zwlr_data_control_source_v1#3)\n").unwrap();
            selection
                .feed(b"zwlr_data_control_offer_v1#8.offer(\"image/png\")\nzwlr_data_control_device_v1#2.selection(zwlr_data_control_offer_v1#9)\n")
                .unwrap();
            assert!(!selection.ready(), "different offer ID");
            selection.feed(b"zwlr_data_control_device_v1#2.selection(zwlr_data_control_offer_v1#8)").unwrap();
            assert!(!selection.ready(), "partial notification");
            selection.feed(b"\n").unwrap();
            assert!(selection.ready());
        }
        #[test]
        fn display_requires_running_notification() {
            assert!(socket_notification(b"created socket wayland-1").is_none());
            assert_eq!(socket_notification(b"Running compositor on wayland display 'wayland-1'"), Some("wayland-1".into()));
            assert!(socket_notification(b"Running compositor on wayland display '../../host'").is_none());
        }
        #[test]
        fn generated_png_checks_asymmetry() {
            let bytes = fixture_png().unwrap();
            exported_png(&bytes, &INPUT).unwrap();
            assert!(exported_png(&bytes, &EXPORT).is_err());
        }
        #[test]
        fn cargo_discovery_requires_one_ui_libtest() {
            let record =
                br#"{"reason":"compiler-artifact","target":{"name":"pdfcraft_ui_egui"},"profile":{"test":true},"executable":"/tmp/ui-tests"}"#;
            assert_eq!(cargo_artifact(record).unwrap(), PathBuf::from("/tmp/ui-tests"));
            assert!(cargo_artifact(b"").is_err());
            assert!(
                cargo_artifact(br#"{"reason":"compiler-artifact","target":{"name":"other"},"profile":{"test":true},"executable":"/tmp/ui-tests"}"#)
                    .is_err()
            );
        }
        fn helper_command(mode: &str) -> Command {
            let name = format!("{}::process_helper", module_path!().split_once("::").unwrap().1);
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", &name, "--nocapture"]).env("PDFCRAFT_PROCESS_HELPER", mode);
            command
        }
        #[test]
        fn process_helper() {
            let Some(mode) = std::env::var_os("PDFCRAFT_PROCESS_HELPER") else { return };
            match mode.to_str().unwrap() {
                "flood" => {
                    let block = [b'x'; 8192];
                    for _ in 0..600 {
                        std::io::stdout().write_all(&block).unwrap();
                    }
                }
                "wait" => {
                    println!("PEER_READY");
                    std::io::stdout().flush().unwrap();
                    let timeout = Timespec::try_from(Duration::from_secs(60)).unwrap();
                    let _ = poll(&mut [], Some(&timeout));
                }
                "cancel" => {
                    let mut run = Run::new(15);
                    run.cancellation = Some(Cancellation::new().unwrap());
                    let peer = run.spawn(&mut helper_command("wait"), Stdio::null()).unwrap();
                    run.until(5, &[peer], |run| Ok(contains(&run.get(peer)?.out, b"PEER_READY"))).unwrap();
                    println!("CANCEL_READY={}", run.get(peer).unwrap().child.id());
                    std::io::stdout().flush().unwrap();
                    let error = run.output(peer, 10, &[]).unwrap_err();
                    assert!(error.to_string().contains("cancelled"));
                    run.cleanup().unwrap();
                    println!("CANCEL_CLEAN");
                }
                "inherit" => {
                    let mut command = helper_command("wait");
                    command.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
                    let child = command.spawn().unwrap();
                    println!("GRANDCHILD={}", child.id());
                    std::io::stdout().flush().unwrap();
                    let mut ack = String::new();
                    std::io::stdin().read_line(&mut ack).unwrap();
                    assert_eq!(ack, "exit\n");
                    // The runner must kill this same-group descendant before reaping our leader.
                    drop(child);
                }
                _ => panic!("unknown process helper mode"),
            }
        }
        #[test]
        fn actual_successful_zero_test_listing_is_rejected() {
            let mut run = Run::new(10);
            let mut command = Command::new(std::env::current_exe().unwrap());
            let child = run.spawn(command.args(["--exact", "absent_test", "--list"]), Stdio::null()).unwrap();
            let output = run.output(child, 5, &[]).unwrap();
            assert!(exact_listing(&output).is_err());
            run.cleanup().unwrap();
        }
        #[test]
        fn output_limit_and_deadline_reap_owned_peers() {
            let mut run = Run::new(15);
            let child = run.spawn(&mut helper_command("flood"), Stdio::null()).unwrap();
            assert!(run.output(child, 5, &[]).is_err());
            run.cleanup().unwrap();
            assert!(run.peers.iter().all(|p| p.status.is_some()));
            let mut run = Run::new(15);
            let child = run.spawn(&mut helper_command("wait"), Stdio::null()).unwrap();
            run.until(5, &[child], |run| Ok(contains(&run.get(child)?.out, b"PEER_READY"))).unwrap();
            assert!(run.until(1, &[child], |_| Ok(false)).is_err());
            run.cleanup().unwrap();
            assert!(run.peers.iter().all(|p| p.status.is_some()));
        }
        #[test]
        fn interrupt_and_terminate_clean_up_separate_owned_groups() {
            struct OwnedPeer(OwnedFd);
            impl Drop for OwnedPeer {
                fn drop(&mut self) {
                    let _ = rustix::process::pidfd_send_signal(&self.0, Signal::KILL);
                }
            }
            for signal in [Signal::INT, Signal::TERM] {
                let mut run = Run::new(15);
                let child = run.spawn(&mut helper_command("cancel"), Stdio::null()).unwrap();
                run.until(5, &[child], |run| Ok(contains(&run.get(child)?.out, b"CANCEL_READY="))).unwrap();
                let text = String::from_utf8_lossy(&run.get(child).unwrap().out);
                let raw =
                    text.split_once("CANCEL_READY=").unwrap().1.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<i32>().unwrap();
                let peer = OwnedPeer(pidfd_open(Pid::from_raw(raw).unwrap(), PidfdFlags::empty()).unwrap());
                rustix::process::kill_process(run.get(child).unwrap().pid, signal).unwrap();
                let output = run.output(child, 5, &[]).unwrap();
                assert!(contains(&output, b"CANCEL_CLEAN"));
                run.cleanup().unwrap();
                let timeout = Timespec::try_from(Duration::from_secs(2)).unwrap();
                let mut fds = [PollFd::new(&peer.0, PollFlags::IN)];
                assert_eq!(poll(&mut fds, Some(&timeout)).unwrap(), 1, "cancelled peer survived");
            }
        }
        #[test]
        fn inherited_pipe_descendant_is_killed_before_leader_reap() {
            struct Descendant(OwnedFd);
            impl Drop for Descendant {
                fn drop(&mut self) {
                    let _ = rustix::process::pidfd_send_signal(&self.0, Signal::KILL);
                }
            }
            let mut run = Run::new(15);
            let child = run.spawn(&mut helper_command("inherit"), Stdio::piped()).unwrap();
            run.until(5, &[child], |run| Ok(contains(&run.get(child)?.out, b"GRANDCHILD="))).unwrap();
            let text = String::from_utf8_lossy(&run.get(child).unwrap().out);
            let raw = text.split_once("GRANDCHILD=").unwrap().1.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<i32>().unwrap();
            let descendant = Descendant(pidfd_open(Pid::from_raw(raw).unwrap(), PidfdFlags::empty()).unwrap());
            let mut ack = run.peers.get_mut(child).unwrap().child.stdin.take().unwrap();
            ack.write_all(b"exit\n").unwrap();
            drop(ack);
            run.output(child, 5, &[]).unwrap();
            run.cleanup().unwrap();
            let timeout = Timespec::try_from(Duration::from_secs(2)).unwrap();
            let mut fds = [PollFd::new(&descendant.0, PollFlags::IN)];
            assert_eq!(poll(&mut fds, Some(&timeout)).unwrap(), 1, "descendant still alive");
        }
    }
}
