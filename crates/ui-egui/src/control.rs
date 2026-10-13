//! The UI control channel (M3.9): lets an agent see and drive the running app.
//!
//! **Opt-in only.** Nothing here runs unless the app is started with `--control <file>` (or a
//! test attaches a [`ControlClient`]). The transport ([`serve`]) listens on loopback only, on a
//! random port, and every connection must first present the random token written to `<file>`.
//!
//! How it works:
//! - [`ControlPlugin`], an egui plugin, keeps a copy of the AccessKit tree from every frame's
//!   output (the widget tree with ids, roles, labels, rects and states) and injects queued input
//!   events (AccessKit click actions, pointer events, keys, text) at the start of later frames.
//! - Requests reach the app through a channel; `PdfCraftApp` answers them at the start of a
//!   frame, so they see and change exactly what the user would.
//!
//! Methods (JSON in, JSON out):
//! - `ui.state`: open documents, active document, page, zoom, page errors, mode, panels, dialog,
//!   notice and observed window dimensions in logical points.
//! - `ui.inspect {query?, role?, focused?, limit?}`: widgets in tree order with `id` (a string), `role`, `label`,
//!   `value`, `rect` (points), `enabled`, `toggled`, `selected`, `clickable`, `focusable`, `focused`, `depth`.
//! - `ui.click {id}` | `{label}` | `{x, y, button?}`: click a widget (by its AccessKit action) or a
//!   point (`button`: primary, secondary, or middle).
//! - `ui.focus {id}` | `{label}`: focus one enabled widget through its AccessKit action.
//! - `ui.resize {width, height}`: request a native window size (320–8192 logical points per dimension).
//!   The window manager may constrain it; `ui.state.window` reports the observed result.
//! - `ui.quit {}`: normal native exit, or `quitting: false, needs_confirmation: true` and the
//!   existing save prompt when any document has unsaved edits or typing. No discard parameter.
//! - `ui.move {x, y}`: move the pointer without pressing a button (e.g. latched autoscroll).
//! - `ui.drag {from: [x, y], to: [x, y], steps?, modifiers?, button?}`: press, move and release (drawing
//!   comments, selecting text, moving comments). `ui.state` reports `pages_on_screen` to aim at.
//! - `ui.type {text}`, `ui.key {key, modifiers?}`: keyboard input to the focused widget / app.
//! - `ui.command {id}`: run a registry command (as the menu would). `ui.commands` lists them.
//! - `ui.set {key, value}`: view options (`--page`, `--zoom`, …), plus persistent preferences.
//!   `select` accepts physical page numbers and ranges such as `1,12,16-18`; blank clears the
//!   explicit selection. This is the same bounded parser as the Organize Pages field; invalid
//!   input leaves the previous selection unchanged. `ui.state.active.selected_pages` reports it.
//!   `default-mode=all|read|edit|convert|sign` applies to future PDF opens; `mode` overrides it
//!   for this session without changing the saved preference. `ui.state.default_mode` reports it.
//!   `theme=light|dark|system` changes the saved theme preference; `ui.state.theme_preference`
//!   reports the choice and `ui.state.theme` reports the resolved light/dark colours.
//! - `ui.open {path}`: accept a file open; native opens report `pending: true` until they
//!   complete and are listed in `ui.state.pending_opens`.
//! - `ui.screenshot {region?}`: PNG of the window (base64), optionally cropped to a rect.
//!
//! Several windows: `ui.state`, `ui.command`, `ui.set`, `ui.open` and `ui.screenshot` take an
//! optional `window` (a number from `ui.windows`; the window that has the focus when left out; the
//! main window for `ui.screenshot`, which cannot capture the other windows).
//! `ui.inspect`, `ui.click`, `ui.drag`, `ui.move`, `ui.type` and `ui.key` take it too (the main
//! window when left out): each window has its own widget tree and its own input, so ids, labels and
//! coordinates are the window's. (Windows embedded in the main one, as in tests, share its tree.)
//! A number that is no window answers `no window N`.
//! - `ui.windows`: every window with its `id`, `title`, `focused` and `tabs` (`name`, `view_no`, `dirty`).
//! - `ui.window_new_view {window?}`: show the window's document in a new window.
//! - `ui.window_move_tab {window?, doc_index, to?}`: move tab `doc_index` to window `to` (a new one without it).
//! - `ui.window_close {window}`: `closing` the window, `asking` when a save question is shown first (an error when a field refused its typing and the window stays); and `ui.window_focus {window?}`.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui::accesskit::{self, Action, NodeId};
use serde_json::{Value, json};

pub type Reply = Result<Value, String>;

/// Seconds to wait for frames (input to be handled, a screenshot to be taken) before reporting
/// that the window is not being drawn. While a window is hidden, minimized or fully covered,
/// eframe keeps calling `logic` but runs no egui pass, so input and screenshots wait.
const NOT_DRAWN_TIMEOUT: f64 = 5.0;

/// Wall-clock seconds (egui's own clock stops while the window is not drawn).
fn now_secs() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
    }
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() / 1000.0
    }
}

fn not_drawn(what: &str) -> String {
    format!(
        "{what} did not happen within {NOT_DRAWN_TIMEOUT:.0} s: the window is not being drawn (it may be hidden, minimized or covered); bring it to the front and retry"
    )
}

/// One request from a client, answered on `reply`.
pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Reply>,
}

/// Sends requests to the app (from the transport thread or a test).
#[derive(Clone)]
pub struct ControlClient {
    tx: Sender<ControlRequest>,
    ctx: egui::Context,
}

impl ControlClient {
    /// Queue a request; the reply arrives on the returned receiver once the app has handled it.
    pub fn send(&self, method: &str, params: Value) -> Receiver<Reply> {
        let (reply, rx) = channel();
        let req = ControlRequest { method: method.into(), params, reply };
        if let Err(e) = self.tx.send(req) {
            let _ = e.0.reply.send(Err("the app has closed".into()));
        }
        self.ctx.request_repaint();
        rx
    }
}

/// What the plugin keeps of one viewport (one window): each window has its own widget tree,
/// its own input and its own passes.
#[derive(Default)]
struct ViewShared {
    nodes: HashMap<NodeId, accesskit::Node>,
    root: Option<NodeId>,
    focus: Option<NodeId>,
    /// Event batches to inject, one batch per frame of this window.
    inject: VecDeque<Vec<egui::Event>>,
    /// Real egui passes of this window so far (counted in `output_hook`, which `logic`-only
    /// calls never reach).
    passes: u64,
}

/// State shared between the plugin (frame hooks) and the app.
#[derive(Default)]
struct Shared {
    views: HashMap<egui::ViewportId, ViewShared>,
    /// The viewports whose passes are running (a window's pass can run inside the main
    /// window's), innermost last: `output_hook` is not told which pass it ends.
    running: Vec<egui::ViewportId>,
}

impl Shared {
    fn view(&mut self, id: egui::ViewportId) -> &mut ViewShared {
        self.views.entry(id).or_default()
    }
}

/// The egui plugin half of the control channel.
pub struct ControlPlugin {
    shared: Arc<Mutex<Shared>>,
}

impl egui::Plugin for ControlPlugin {
    fn debug_name(&self) -> &'static str {
        "pdfcraft-control"
    }

    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let Ok(mut s) = self.shared.lock() else { return };
        let id = input.viewport_id;
        if s.running.len() >= 16 {
            s.running.remove(0); // bounded, whatever happens
        }
        s.running.push(id);
        let view = s.view(id);
        if let Some(batch) = view.inject.pop_front() {
            input.events.extend(batch);
        }
        if !view.inject.is_empty() {
            ctx.request_repaint_of(id);
        }
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        let Ok(mut s) = self.shared.lock() else { return };
        let id = s.running.pop().unwrap_or(egui::ViewportId::ROOT);
        let view = s.view(id);
        view.passes += 1;
        let Some(update) = &output.platform_output.accesskit_update else { return };
        view.nodes = update.nodes.iter().cloned().collect();
        view.root = update.tree.as_ref().map(|t| t.root);
        view.focus = Some(update.focus);
    }
}

enum Pending {
    /// Answer after `frames` more frames (the injected input has been handled by then).
    Frames {
        /// The window the input went to.
        viewport: egui::ViewportId,
        /// Answer once this many egui passes of that window have run (the injected input has
        /// been handled).
        until_pass: u64,
        since: f64,
        reply: Sender<Reply>,
        value: Value,
    },
    Screenshot {
        tag: u64,
        region: Option<egui::Rect>,
        /// The window's viewport: its screenshot arrives in that viewport's input.
        viewport: egui::ViewportId,
        reply: Sender<Reply>,
        /// When it was requested (egui time, seconds).
        since: f64,
    },
}

/// The app half: the request queue and requests waiting for later frames.
pub struct Control {
    rx: Receiver<ControlRequest>,
    shared: Arc<Mutex<Shared>>,
    pending: Vec<Pending>,
    next_tag: u64,
}

/// Install the control channel on `ctx`: returns the app half and a client.
pub fn attach(ctx: &egui::Context) -> (Control, ControlClient) {
    let shared = Arc::new(Mutex::new(Shared::default()));
    ctx.add_plugin(ControlPlugin { shared: shared.clone() });
    ctx.enable_accesskit();
    let (tx, rx) = channel();
    (Control { rx, shared, pending: Vec::new(), next_tag: 1 }, ControlClient { tx, ctx: ctx.clone() })
}

/// What a request needs from the app (implemented by `PdfCraftApp`).
///
/// `window` is the window a request is for (`None`: the one that has the focus).
pub(crate) trait Host {
    fn state(&mut self, window: Option<u32>) -> Reply;
    fn command(&mut self, window: Option<u32>, id: &str) -> Reply;
    fn dock(&mut self, reset: bool, params: &Value) -> Reply;
    fn commands(&self) -> Value;
    fn set(&mut self, window: Option<u32>, key: &str, value: &str) -> Reply;
    fn open(&mut self, window: Option<u32>, path: &str) -> Reply;
    fn quit(&mut self, ctx: &egui::Context) -> bool;
    fn windows(&mut self) -> Value;
    fn window_new_view(&mut self, window: Option<u32>) -> Reply;
    fn window_move_tab(&mut self, window: Option<u32>, doc_index: usize, to: Option<u32>) -> Reply;
    fn window_close(&mut self, window: Option<u32>) -> Reply;
    fn window_focus(&mut self, window: Option<u32>) -> Reply;
    /// The egui viewport of window `window`, if there is such a window (screenshots).
    fn viewport_of(&mut self, window: Option<u32>) -> Result<egui::ViewportId, String>;
    /// The viewport whose widget tree and input a request for window `window` (`None`: the main
    /// window) works on.
    fn input_viewport(&mut self, window: Option<u32>) -> Result<egui::ViewportId, String>;
}

impl Control {
    /// Handle new requests and finish waiting ones. Call at the start of every frame.
    pub(crate) fn tick(&mut self, ctx: &egui::Context, host: &mut impl Host) {
        self.finish_pending(ctx);
        while let Ok(req) = self.rx.try_recv() {
            match self.handle(ctx, host, &req.method, &req.params) {
                Handled::Now(r) => {
                    let _ = req.reply.send(r);
                }
                Handled::AfterFrames(viewport, frames, value) => {
                    let until_pass = self.passes(viewport) + u64::from(frames);
                    ctx.request_repaint_of(viewport);
                    self.pending.push(Pending::Frames { viewport, until_pass, since: now_secs(), reply: req.reply, value });
                }
                Handled::Screenshot(region, viewport) => {
                    let tag = self.next_tag;
                    self.next_tag += 1;
                    ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Screenshot(egui::UserData::new(tag)));
                    ctx.request_repaint_of(viewport);
                    self.pending.push(Pending::Screenshot { tag, region, viewport, reply: req.reply, since: now_secs() });
                }
            }
        }
        if !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }

    fn finish_pending(&mut self, ctx: &egui::Context) {
        let mut viewports: Vec<egui::ViewportId> =
            self.pending.iter().filter_map(|p| if let Pending::Screenshot { viewport, .. } = p { Some(*viewport) } else { None }).collect();
        viewports.push(egui::ViewportId::ROOT);
        viewports.sort();
        viewports.dedup();
        let mut shots: Vec<(u64, Arc<egui::ColorImage>)> = Vec::new();
        for viewport in viewports {
            shots.extend(ctx.input_for(viewport, |i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { user_data, image, .. } => {
                            user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).map(|tag| (*tag, image.clone()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            }));
        }
        let ppp = ctx.pixels_per_point();
        let now = now_secs();
        let mut keep = Vec::new();
        let waiting: Vec<Pending> = self.pending.drain(..).collect();
        for p in waiting {
            match p {
                Pending::Frames { viewport, until_pass, reply, value, .. } if self.passes(viewport) >= until_pass => {
                    let _ = reply.send(Ok(value));
                }
                Pending::Frames { viewport, since, reply, .. } if now - since > NOT_DRAWN_TIMEOUT => {
                    // The agent is told it failed, so it must not happen later either.
                    if let Ok(mut sh) = self.shared.lock() {
                        sh.view(viewport).inject.clear();
                    }
                    let _ = reply.send(Err(not_drawn("handling the input")));
                }
                p @ Pending::Frames { .. } => keep.push(p),
                Pending::Screenshot { tag, region, viewport, reply, since } => match shots.iter().find(|(t, _)| *t == tag) {
                    Some((_, image)) => {
                        let _ = reply.send(screenshot_png(image, region, ppp));
                    }
                    // The platform skips painting windows that are hidden, minimized or fully
                    // covered, so no screenshot ever arrives: say so instead of hanging.
                    None if now - since > NOT_DRAWN_TIMEOUT => {
                        let _ = reply.send(Err(not_drawn("the screenshot")));
                    }
                    None => keep.push(Pending::Screenshot { tag, region, viewport, reply, since }),
                },
            }
        }
        self.pending = keep;
    }

    fn passes(&self, viewport: egui::ViewportId) -> u64 {
        self.shared.lock().map(|s| s.views.get(&viewport).map_or(0, |v| v.passes)).unwrap_or(0)
    }

    fn inject(&self, viewport: egui::ViewportId, batches: Vec<Vec<egui::Event>>) -> u32 {
        let n = batches.len() as u32;
        if let Ok(mut s) = self.shared.lock() {
            s.view(viewport).inject.extend(batches);
        }
        // The last batch is handled during the frame it is injected into; answer one frame later.
        n + 1
    }

    fn handle(&mut self, ctx: &egui::Context, host: &mut impl Host, method: &str, p: &Value) -> Handled {
        let str_param = |k: &str| p.get(k).and_then(Value::as_str).ok_or_else(|| format!("{method}: missing string parameter {k}"));
        // Which window the request is for. A number that does not fit is no window.
        let window: Result<Option<u32>, String> = match p.get("window") {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).map(Some).ok_or_else(|| format!("no window {v}")),
        };
        let r: Result<Handled, String> = (|| match method {
            "ui.state" => {
                let window = window?;
                let mut reply = host.state(window);
                if let Ok(state) = reply.as_mut() {
                    // The size of the window the request is for (the main window's input when it
                    // has none of its own, as embedded windows).
                    let viewport = host.input_viewport(window).unwrap_or(egui::ViewportId::ROOT);
                    let size = ctx.input_for(viewport, |i| i.viewport_rect().size());
                    let id = state.get("window").cloned().unwrap_or(Value::Null);
                    state["window"] = json!({ "id": id, "width": size.x, "height": size.y, "pixels_per_point": ctx.pixels_per_point() });
                }
                Ok(Handled::Now(reply))
            }
            "ui.commands" => Ok(Handled::Now(Ok(host.commands()))),
            "ui.windows" => Ok(Handled::Now(Ok(host.windows()))),
            "ui.window_new_view" => Ok(Handled::Now(host.window_new_view(window?))),
            "ui.window_move_tab" => {
                let index = p
                    .get("doc_index")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("ui.window_move_tab: doc_index must be the tab's number")?;
                let to = match p.get("to") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("no window {v}"))?),
                };
                Ok(Handled::Now(host.window_move_tab(window?, index, to)))
            }
            "ui.window_close" => Ok(Handled::Now(host.window_close(window?))),
            "ui.window_focus" => Ok(Handled::Now(host.window_focus(window?))),
            "ui.dock" | "ui.dock.reset" => Ok(Handled::Now(host.dock(method == "ui.dock.reset", p))),
            "ui.command" if str_param("id")? == "ui.dock" => Ok(Handled::Now(host.dock(false, p.get("params").unwrap_or(p)))),
            "ui.command" => Ok(Handled::Now(host.command(window?, str_param("id")?))),
            "ui.set" => {
                let value = match p.get("value") {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => return Err("ui.set: missing parameter value".into()),
                };
                Ok(Handled::Now(host.set(window?, str_param("key")?, &value)))
            }
            "ui.open" => Ok(Handled::Now(host.open(window?, str_param("path")?))),
            "ui.inspect" => {
                let vp = host.input_viewport(window?)?;
                Ok(Handled::Now(Ok(self.inspect(vp, p))))
            }
            "ui.click" => {
                let vp = host.input_viewport(window?)?;
                self.click(vp, p)
            }
            "ui.drag" => {
                let vp = host.input_viewport(window?)?;
                self.drag(vp, p)
            }
            "ui.focus" => {
                let vp = host.input_viewport(window?)?;
                self.focus(vp, p)
            }
            "ui.resize" => {
                only_params(p, method, &["width", "height", "window"])?;
                let dimension = |key: &str| -> Result<f32, String> {
                    p.get(key)
                        .and_then(Value::as_f64)
                        .filter(|n| n.is_finite() && (320.0..=8192.0).contains(n))
                        .map(|n| n as f32)
                        .ok_or_else(|| format!("ui.resize: {key} must be between 320 and 8192 logical points"))
                };
                let size = egui::vec2(dimension("width")?, dimension("height")?);
                if cfg!(target_arch = "wasm32") {
                    return Err("ui.resize needs a native window".into());
                }
                let vp = host.viewport_of(window?)?;
                ctx.send_viewport_cmd_to(vp, egui::ViewportCommand::InnerSize(size));
                ctx.request_repaint();
                Ok(Handled::AfterFrames(vp, 2, json!({ "requested": [size.x, size.y] })))
            }
            "ui.quit" => {
                only_params(p, method, &[])?;
                if cfg!(target_arch = "wasm32") {
                    return Err("ui.quit needs a native window".into());
                }
                let quitting = host.quit(ctx);
                Ok(Handled::Now(Ok(json!({ "quitting": quitting, "needs_confirmation": !quitting }))))
            }
            "ui.move" => {
                let vp = host.input_viewport(window?)?;
                let point = |key: &str| -> Result<f32, String> {
                    p.get(key)
                        .and_then(Value::as_f64)
                        .map(|n| n as f32)
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| format!("ui.move: {key} must be a finite coordinate in points"))
                };
                let pos = egui::pos2(point("x")?, point("y")?);
                Ok(Handled::AfterFrames(vp, self.inject(vp, vec![vec![egui::Event::PointerMoved(pos)]]), json!({ "moved": [pos.x, pos.y] })))
            }
            "ui.type" => {
                let vp = host.input_viewport(window?)?;
                let text = str_param("text")?.to_string();
                Ok(Handled::AfterFrames(vp, self.inject(vp, vec![vec![egui::Event::Text(text)]]), json!({ "typed": true })))
            }
            "ui.key" => {
                let vp = host.input_viewport(window?)?;
                let name = str_param("key")?;
                let key = egui::Key::from_name(name)
                    .ok_or_else(|| format!("ui.key: unknown key {name:?} (egui key names: A, Enter, Escape, ArrowDown, F5, …)"))?;
                let modifiers = modifiers(p.get("modifiers"))?;
                let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
                Ok(Handled::AfterFrames(vp, self.inject(vp, vec![vec![ev(true)], vec![ev(false)]]), json!({ "key": name })))
            }
            "ui.screenshot" => {
                let region = match p.get("region") {
                    None | Some(Value::Null) => None,
                    Some(r) => Some(rect_param(r)?),
                };
                ctx.request_repaint();
                Ok(Handled::Screenshot(region, host.viewport_of(window?)?))
            }
            other => Err(format!(
                "unknown method {other:?} (ui.state, ui.inspect, ui.click, ui.focus, ui.resize, ui.quit, ui.move, ui.drag, ui.type, ui.key, ui.command, ui.commands, ui.set, ui.open, ui.screenshot, ui.windows, ui.window_new_view, ui.window_move_tab, ui.window_close, ui.window_focus)"
            )),
        })();
        r.unwrap_or_else(|e| Handled::Now(Err(e)))
    }

    fn inspect(&self, viewport: egui::ViewportId, p: &Value) -> Value {
        let query = p.get("query").and_then(Value::as_str).map(str::to_lowercase);
        let role = p.get("role").and_then(Value::as_str).map(str::to_lowercase);
        let focused = p.get("focused").and_then(Value::as_bool);
        let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
        let Ok(shared) = self.shared.lock() else { return json!({ "widgets": [] }) };
        let Some(s) = shared.views.get(&viewport) else { return json!({ "widgets": [], "count": 0, "truncated": false }) };
        let mut out = Vec::new();
        let mut total = 0usize;
        let mut stack: Vec<(NodeId, usize)> = s.root.map(|r| vec![(r, 0)]).unwrap_or_default();
        while let Some((id, depth)) = stack.pop() {
            let Some(node) = s.nodes.get(&id) else { continue };
            for c in node.children().iter().rev() {
                stack.push((*c, depth + 1));
            }
            let w = widget(id, node, depth, s.focus == Some(id));
            let text =
                format!("{} {} {}", w["label"].as_str().unwrap_or(""), w["value"].as_str().unwrap_or(""), w["description"].as_str().unwrap_or(""))
                    .to_lowercase();
            let role_ok = role.as_ref().is_none_or(|r| w["role"].as_str().is_some_and(|x| x.to_lowercase() == *r));
            let query_ok = query.as_ref().is_none_or(|q| text.contains(q.as_str()));
            let focus_ok = focused.is_none_or(|want| (s.focus == Some(id)) == want);
            if role_ok && query_ok && focus_ok && id != s.root.unwrap_or(id) {
                total += 1;
                if out.len() < limit {
                    out.push(w);
                }
            }
        }
        json!({ "widgets": out, "count": total, "truncated": total > out.len() })
    }

    /// Press at `from`, move to `to` in `steps` frames, release (drawing, selecting text, moving).
    fn drag(&mut self, viewport: egui::ViewportId, p: &Value) -> Result<Handled, String> {
        let point = |k: &str| -> Result<egui::Pos2, String> {
            match p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).as_deref() {
                Some([x, y]) => Some(egui::pos2(*x as f32, *y as f32))
                    .filter(|pos| pos.is_finite())
                    .ok_or_else(|| format!("ui.drag: {k} must be [x, y], finite coordinates in points")),
                _ => Err(format!("ui.drag: {k} must be [x, y] in points")),
            }
        };
        let (from, to) = (point("from")?, point("to")?);
        if !(to - from).is_finite() {
            return Err("ui.drag: from and to must be a finite distance apart, in points".into());
        }
        let steps = p.get("steps").and_then(Value::as_u64).unwrap_or(8).clamp(1, 200) as usize;
        let modifiers = modifiers(p.get("modifiers"))?;
        let which = pointer_button(p, "ui.drag")?;
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: which, pressed, modifiers };
        let mut frames = vec![vec![egui::Event::PointerMoved(from)], vec![button(from, true)]];
        for k in 1..=steps {
            frames.push(vec![egui::Event::PointerMoved(from + (to - from) * (k as f32 / steps as f32))]);
        }
        frames.push(vec![button(to, false)]);
        let n = self.inject(viewport, frames);
        Ok(Handled::AfterFrames(viewport, n, json!({ "dragged": [[from.x, from.y], [to.x, to.y]] })))
    }

    fn click(&mut self, viewport: egui::ViewportId, p: &Value) -> Result<Handled, String> {
        if let (Some(x), Some(y)) = (p.get("x").and_then(Value::as_f64), p.get("y").and_then(Value::as_f64)) {
            let point = |key: &str, n: f64| {
                Some(n as f32).filter(|n| n.is_finite()).ok_or_else(|| format!("ui.click: {key} must be a finite coordinate in points"))
            };
            let pos = egui::pos2(point("x", x)?, point("y", y)?);
            let which = pointer_button(p, "ui.click")?;
            let button = |pressed| egui::Event::PointerButton { pos, button: which, pressed, modifiers: egui::Modifiers::NONE };
            let frames = self.inject(viewport, vec![vec![egui::Event::PointerMoved(pos)], vec![button(true)], vec![button(false)]]);
            return Ok(Handled::AfterFrames(viewport, frames, json!({ "clicked": [x, y] })));
        }
        let shared = self.shared.lock().map_err(|_| "control state poisoned")?;
        let empty = ViewShared::default();
        let s = shared.views.get(&viewport).unwrap_or(&empty);
        let id = match (p.get("id"), p.get("label").and_then(Value::as_str)) {
            (Some(id), _) => {
                let id: u64 =
                    id.as_u64().or_else(|| id.as_str().and_then(|s| s.parse().ok())).ok_or("ui.click: id must be a widget id from ui.inspect")?;
                let id = NodeId(id);
                if !s.nodes.contains_key(&id) {
                    return Err(format!("ui.click: no widget with id {} on screen (run ui.inspect again)", id.0));
                }
                id
            }
            (None, Some(label)) => {
                let want = label.to_lowercase();
                let matches: Vec<NodeId> = s
                    .nodes
                    .iter()
                    .filter(|(_, n)| n.supports_action(Action::Click) && !n.is_disabled() && n.label().is_some_and(|l| l.to_lowercase() == want))
                    .map(|(id, _)| *id)
                    .collect();
                match matches.as_slice() {
                    [one] => *one,
                    [] => return Err(format!("ui.click: no enabled clickable widget labelled {label:?} (try ui.inspect with query)")),
                    many => return Err(format!("ui.click: {} widgets are labelled {label:?}; click one by id", many.len())),
                }
            }
            (None, None) => return Err("ui.click: pass id, label, or x and y".into()),
        };
        let node = &s.nodes[&id];
        if node.is_disabled() {
            return Err(format!("ui.click: widget {} ({}) is disabled", id.0, node.label().unwrap_or("unlabelled")));
        }
        let label = node.label().map(str::to_owned);
        let action = accesskit::ActionRequest { action: Action::Click, target_tree: accesskit::TreeId::ROOT, target_node: id, data: None };
        drop(shared);
        let frames = self.inject(viewport, vec![vec![egui::Event::AccessKitActionRequest(action)]]);
        Ok(Handled::AfterFrames(viewport, frames, json!({ "clicked": id.0.to_string(), "label": label })))
    }

    fn focus(&mut self, viewport: egui::ViewportId, p: &Value) -> Result<Handled, String> {
        only_params(p, "ui.focus", &["id", "label", "window"])?;
        let shared = self.shared.lock().map_err(|_| "control state poisoned")?;
        let empty = ViewShared::default();
        let s = shared.views.get(&viewport).unwrap_or(&empty);
        let id = match (p.get("id"), p.get("label")) {
            (Some(id), None) => {
                NodeId(id.as_u64().or_else(|| id.as_str().and_then(|s| s.parse().ok())).ok_or("ui.focus: id must be a widget id from ui.inspect")?)
            }
            (None, Some(Value::String(label))) => {
                let matches: Vec<NodeId> = s
                    .nodes
                    .iter()
                    .filter(|(_, n)| n.supports_action(Action::Focus) && !n.is_disabled() && n.label().is_some_and(|l| l.eq_ignore_ascii_case(label)))
                    .map(|(id, _)| *id)
                    .collect();
                match matches.as_slice() {
                    [one] => *one,
                    [] => return Err(format!("ui.focus: no enabled focusable widget labelled {label:?}")),
                    many => return Err(format!("ui.focus: {} widgets are labelled {label:?}; use a widget id", many.len())),
                }
            }
            _ => return Err("ui.focus: pass exactly one widget id or label".into()),
        };
        let node = s.nodes.get(&id).ok_or_else(|| format!("ui.focus: no widget with id {} on screen (run ui.inspect again)", id.0))?;
        if node.is_disabled() || !node.supports_action(Action::Focus) {
            return Err(format!("ui.focus: widget {} is disabled or not focusable", id.0));
        }
        let action = accesskit::ActionRequest { action: Action::Focus, target_tree: accesskit::TreeId::ROOT, target_node: id, data: None };
        drop(shared);
        let frames = self.inject(viewport, vec![vec![egui::Event::AccessKitActionRequest(action)]]);
        Ok(Handled::AfterFrames(viewport, frames, json!({ "focus_requested": id.0.to_string() })))
    }
}

enum Handled {
    Now(Reply),
    AfterFrames(egui::ViewportId, u32, Value),
    Screenshot(Option<egui::Rect>, egui::ViewportId),
}

fn widget(id: NodeId, n: &accesskit::Node, depth: usize, focused: bool) -> Value {
    let rect = n.bounds().map(|b| [b.x0, b.y0, b.x1, b.y1]);
    let mut w = json!({
        // A string: 64-bit ids don't survive JSON parsers that use doubles.
        "id": id.0.to_string(),
        "role": format!("{:?}", n.role()),
        "depth": depth,
        "enabled": !n.is_disabled(),
        "clickable": n.supports_action(Action::Click),
        "focusable": n.supports_action(Action::Focus),
    });
    let mut more = serde_json::Map::new();
    let obj = &mut more;
    if let Some(l) = n.label() {
        obj.insert("label".into(), json!(l));
    }
    if let Some(v) = n.value() {
        obj.insert("value".into(), json!(v));
    }
    if let Some(d) = n.description() {
        obj.insert("description".into(), json!(d));
    }
    if let Some(r) = rect {
        obj.insert("rect".into(), json!(r));
    }
    if let Some(t) = n.toggled() {
        obj.insert("toggled".into(), json!(format!("{t:?}").to_lowercase()));
    }
    if let Some(s) = n.is_selected() {
        obj.insert("selected".into(), json!(s));
    }
    if focused {
        obj.insert("focused".into(), json!(true));
    }
    if let Value::Object(m) = &mut w {
        m.extend(more);
    }
    w
}

fn only_params(p: &Value, method: &str, allowed: &[&str]) -> Result<(), String> {
    // The JSON-RPC transport represents omitted params as null.
    if p.is_null() && allowed.is_empty() {
        return Ok(());
    }
    let object = p.as_object().ok_or_else(|| format!("{method}: params must be an object"))?;
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{method}: unknown parameter {key:?}"));
    }
    Ok(())
}

fn pointer_button(p: &Value, method: &str) -> Result<egui::PointerButton, String> {
    match p.get("button").and_then(Value::as_str).unwrap_or("primary") {
        "primary" | "left" => Ok(egui::PointerButton::Primary),
        "secondary" | "right" => Ok(egui::PointerButton::Secondary),
        "middle" => Ok(egui::PointerButton::Middle),
        other => Err(format!("{method}: unknown button {other:?} (primary, secondary, middle)")),
    }
}

fn modifiers(v: Option<&Value>) -> Result<egui::Modifiers, String> {
    let mut m = egui::Modifiers::NONE;
    for name in v.and_then(Value::as_array).into_iter().flatten() {
        match name.as_str().unwrap_or("") {
            "command" | "cmd" => m.command = true,
            "shift" => m.shift = true,
            "alt" | "option" => m.alt = true,
            "ctrl" | "control" => m.ctrl = true,
            "mac_cmd" => m.mac_cmd = true,
            other => return Err(format!("ui.key: unknown modifier {other:?} (command, shift, alt, ctrl)")),
        }
    }
    // "command" is ⌘ on macOS and Ctrl elsewhere, as egui expects.
    if m.command {
        if cfg!(target_os = "macos") {
            m.mac_cmd = true;
        } else {
            m.ctrl = true;
        }
    }
    Ok(m)
}

fn rect_param(v: &Value) -> Result<egui::Rect, String> {
    let a: Vec<f32> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|f| f as f32).collect()).unwrap_or_default();
    match a.as_slice() {
        [x0, y0, x1, y1] if x1 > x0 && y1 > y0 => Ok(egui::Rect::from_min_max(egui::pos2(*x0, *y0), egui::pos2(*x1, *y1))),
        _ => Err("region must be [x0, y0, x1, y1] in points".into()),
    }
}

/// Encode (a region of) a screenshot as PNG; the reply carries base64 data and the size.
fn screenshot_png(image: &egui::ColorImage, region: Option<egui::Rect>, ppp: f32) -> Reply {
    let [w, h] = image.size;
    let (x0, y0, x1, y1) = match region {
        Some(r) => {
            let px = |v: f32, max: usize| ((v * ppp).round().max(0.0) as usize).min(max);
            (px(r.min.x, w), px(r.min.y, h), px(r.max.x, w), px(r.max.y, h))
        }
        None => (0, 0, w, h),
    };
    if x1 <= x0 || y1 <= y0 {
        return Err("the region is outside the window".into());
    }
    let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        for px in &image.pixels[y * w + x0..y * w + x1] {
            rgba.extend_from_slice(&px.to_srgba_unmultiplied());
        }
    }
    let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, cw, ch);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    use base64::Engine as _;
    Ok(json!({ "png_base64": base64::engine::general_purpose::STANDARD.encode(out), "width": cw, "height": ch, "pixels_per_point": ppp }))
}

impl crate::PdfCraftApp {
    /// Run `f` with the window a request names (default: the focused one) loaded.
    fn in_window(&mut self, window: Option<u32>, f: impl FnOnce(&mut Self) -> Reply) -> Reply {
        let id = window.map_or(self.focused_window, crate::WindowId);
        self.with_window(id, f).unwrap_or_else(|| Err(format!("no window {}", id.0)))
    }

    /// The state of the loaded window (`ui.state`).
    fn loaded_state(&self) -> Value {
        let active = self.active.and_then(|i| self.views.get(i));
        let docs: Vec<Value> = self
            .views
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                let d = self.session.get(v.id)?;
                Some(json!({ "tab": i, "doc": v.id.0, "name": d.name, "path": d.path, "pages": d.info.pages.len(), "dirty": d.dirty, "active": self.active == Some(i) }))
            })
            .collect();
        #[cfg(not(target_arch = "wasm32"))]
        let pending_opens: Vec<Value> =
            self.pending_opens.iter().map(|pending| json!({ "token": pending.token, "name": pending.name, "path": pending.path })).collect();
        #[cfg(target_arch = "wasm32")]
        let pending_opens: Vec<Value> = Vec::new();
        json!({
            "docking": self.docking,
            "documents": docs,
            "pending_opens": pending_opens,
            "active": active.map(|v| json!({
                "doc": v.id.0,
                "page": v.current + 1,
                "zoom_percent": (v.zoom * 100.0).round(),
                "fit": format!("{:?}", v.fit),
                "layout": format!("{:?}", v.layout),
                "organize": v.organize,
                "grid_zoom": v.grid_zoom(),
                // Pages picked in the organize grid or the Pages panel (empty: the current page).
                "selected_pages": v.selected.iter().map(|p| p + 1).collect::<Vec<_>>(),
                "auto_scrolling": v.auto_scrolling(),
                "raster_memory": v.raster_memory(),
                "viewport": [v.viewport_rect().min.x, v.viewport_rect().min.y, v.viewport_rect().max.x, v.viewport_rect().max.y],
                "find_open": v.find.is_some(),
                "page_errors": v.page_errors().iter().map(|(p, e)| json!({ "page": p + 1, "error": e })).collect::<Vec<_>>(),
                // Where pages are on screen (points), to aim ui.click / ui.drag at page content.
                "pages_on_screen": v.visible_page_rects().iter().map(|(p, r)| json!({ "page": p + 1, "rect": [r.min.x, r.min.y, r.max.x, r.max.y] })).collect::<Vec<_>>(),
                "selected_comment": v.comments.selected.map(|(p, i)| json!({ "page": p + 1, "index": i + 1 })),
                "comment_composer_open": v.comments.composer.is_some(),
            })),
            // The Combine files tab: how its files show, and each grid thumbnail's state.
            "combine": {
                "showing": self.combine_showing(),
                "view": self.combine_view.as_str(),
                "zoom": self.combine_zoom,
                "files": self.combine_draft.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                "selected": self.combine_selection(),
                "thumbnails": self.combine_thumbnails().iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
                "expanded": self.combine_expanded().iter().filter_map(|i| self.combine_draft.get(*i).map(|f| f.name.clone())).collect::<Vec<_>>(),
                "preview": self.combine_preview().map(|(file, page, render)| json!({ "file": file, "page": page + 1, "render": format!("{render:?}") })),
            },
            "quick_tool": match self.quick_tool {
                crate::QuickTool::Measure(t) => format!("measure-{}", t.name()),
                crate::QuickTool::Select => "select".to_string(),
                crate::QuickTool::Hand => "hand".to_string(),
                crate::QuickTool::Crop => "crop".to_string(),
                crate::QuickTool::EnhanceCamera => "enhance-camera".to_string(),
                crate::QuickTool::Redact => "redact".to_string(),
                crate::QuickTool::AddText => "add-text".to_string(),
                crate::QuickTool::EditText => "edit-text".to_string(),
                crate::QuickTool::Link => "link".to_string(),
                crate::QuickTool::SignArea { certify: false } => "sign".to_string(),
                crate::QuickTool::SignArea { certify: true } => "certify".to_string(),
                crate::QuickTool::MarqueeZoom => "marquee-zoom".to_string(),
                crate::QuickTool::Snapshot => "snapshot".to_string(),
                crate::QuickTool::ColumnSelect => "column-select".to_string(),
                crate::QuickTool::Stamp(k) => format!("stamp-{}", k.name().trim_start_matches("PC").to_ascii_lowercase()),
                crate::QuickTool::CustomStamp(i) => format!("custom-stamp-{i}"),
                crate::QuickTool::Fill(f) => format!("fill-{}", f.command().trim_start_matches("sign.fill.")),
                crate::QuickTool::Field(f) => format!("field-{}", f.command().trim_start_matches("form.add.")),
                crate::QuickTool::Comment(t) => t.command().trim_start_matches("comment.").to_string(),
            },
            "home": self.active.is_none(),
            "mode": format!("{:?}", self.mode),
            "default_mode": self.default_mode,
            "left_panel": if self.left_open { json!(format!("{:?}", self.left)) } else { Value::Null },
            "tools_side": if self.docking.tools_on_right() { "right" } else { "left" },
            "right_panel": self.right.map(|r| format!("{r:?}")),
            "dialog": self.dialog.map(|d| format!("{d:?}")),
            "palette_open": self.palette_open,
            "theme": format!("{:?}", self.theme),
            "theme_preference": self.theme_preference,
            "language": self.language,
            "notice": self.toast.as_ref().map(|t| t.0.clone()),
            "progress": self.progress_notice.as_ref().map(|p| json!({ "label": p.label, "fraction": p.fraction })),
            "password_prompt": self.password_prompt.is_some(),
            "close_prompt": self.close_request.is_some(),
        })
    }

    fn command_here(&mut self, id: &str) -> Reply {
        let spec = pdfcraft_engine::commands::command(id).ok_or_else(|| format!("unknown command {id:?} (see ui.commands)"))?;
        if !self.command_enabled(spec) {
            return Err(format!("{id} is disabled right now"));
        }
        self.execute(id);
        Ok(json!({ "ran": id }))
    }
}

impl Host for crate::PdfCraftApp {
    fn dock(&mut self, reset: bool, params: &Value) -> Reply {
        crate::docking::command(self, reset, params)
    }

    fn quit(&mut self, ctx: &egui::Context) -> bool {
        self.control_quit(ctx)
    }

    fn state(&mut self, window: Option<u32>) -> Reply {
        self.in_window(window, |a| {
            let mut state = a.loaded_state();
            if let Some(map) = state.as_object_mut() {
                map.insert("window".into(), json!(a.current_window().0));
                map.insert("window_count".into(), json!(a.window_count()));
            }
            Ok(state)
        })
    }

    fn command(&mut self, window: Option<u32>, id: &str) -> Reply {
        self.in_window(window, |a| a.command_here(id))
    }

    fn commands(&self) -> Value {
        let mut list: Vec<Value> = pdfcraft_engine::commands::COMMANDS
            .iter()
            .map(|c| json!({ "id": c.id, "label": c.label, "menu": c.menu, "enabled": self.command_enabled(c), "shortcut": c.shortcut.map(|s| s.label(cfg!(target_os = "macos"))) }))
            .collect();
        list.push(json!({"id": "ui.dock", "label": "Arrange Panel", "enabled": true, "params": "action, or operation/panel; move: target/zone/before; float/moveFloating: rect; resizeSplit: path/size; setStackOpen: open; resizeStack: height"}));
        json!({ "commands": list })
    }

    fn set(&mut self, window: Option<u32>, key: &str, value: &str) -> Reply {
        self.in_window(window, |a| a.set_option(key, value).map(|()| json!({ "set": key, "value": value })))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open(&mut self, window: Option<u32>, path: &str) -> Reply {
        self.in_window(window, |a| {
            let before = a.views.len();
            a.open_path(path);
            if a.views.len() > before || a.password_prompt.is_some() {
                Ok(json!({ "opened": path, "password_prompt": a.password_prompt.is_some() }))
            } else if a.pending_opens.iter().any(|pending| pending.path.as_deref() == Some(path)) {
                Ok(json!({ "accepted": path, "pending": true }))
            } else {
                Err(a.toast.as_ref().map(|t| t.0.clone()).unwrap_or_else(|| format!("couldn't open {path}")))
            }
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn open(&mut self, _window: Option<u32>, _path: &str) -> Reply {
        Err("ui.open needs a file system; on the web, drop the file on the page".into())
    }

    fn windows(&mut self) -> Value {
        let focused = self.focused_window;
        let list: Vec<Value> = self
            .window_ids()
            .into_iter()
            .filter_map(|id| {
                self.with_window(id, |a| {
                    let tabs: Vec<Value> = a
                        .views
                        .iter()
                        .filter_map(|v| {
                            let d = a.session.get(v.id)?;
                            Some(json!({ "name": d.name, "view_no": v.view_no, "dirty": d.dirty }))
                        })
                        .collect();
                    json!({ "id": id.0, "title": a.window_title, "focused": id == focused, "tabs": tabs })
                })
            })
            .collect();
        json!({ "windows": list })
    }

    fn window_new_view(&mut self, window: Option<u32>) -> Reply {
        self.in_window(window, |a| a.command_here("window.new_view"))
    }

    fn window_move_tab(&mut self, window: Option<u32>, doc_index: usize, to: Option<u32>) -> Reply {
        let to = match to {
            Some(n) if !self.has_window(crate::WindowId(n)) => return Err(format!("no window {n}")),
            other => other.map(crate::WindowId),
        };
        self.in_window(window, |a| {
            let doc = a.views.get(doc_index).map(|v| v.id).ok_or_else(|| format!("no tab {doc_index}"))?;
            let from = a.current_window();
            a.pending_window_ops.push(crate::WindowOp::MoveTab { doc, from, to });
            Ok(json!({ "moved": doc_index }))
        })
    }

    fn window_close(&mut self, window: Option<u32>) -> Reply {
        self.in_window(window, |a| {
            let id = a.current_window();
            if id == crate::WindowId::ROOT {
                return Err("the main window closes with the app (use the file ▸ close commands for its tabs)".into());
            }
            a.request_close_window(id);
            if a.close_request.is_some() {
                Ok(json!({ "asking": id.0 }))
            } else if a.pending_window_ops.contains(&crate::WindowOp::Close(id)) {
                Ok(json!({ "closing": id.0 }))
            } else {
                Err("the window stays open: a field refused what was typed into it".into())
            }
        })
    }

    fn window_focus(&mut self, window: Option<u32>) -> Reply {
        self.in_window(window, |a| {
            let id = a.current_window();
            a.pending_window_ops.push(crate::WindowOp::Focus(id));
            Ok(json!({ "focused": id.0 }))
        })
    }

    fn input_viewport(&mut self, window: Option<u32>) -> Result<egui::ViewportId, String> {
        let id = crate::WindowId(window.unwrap_or(0));
        if !self.has_window(id) {
            return Err(format!("no window {}", id.0));
        }
        // Embedded windows (no windowing system of their own, as in tests) are drawn inside the
        // main window's pass and share its widget tree and input.
        let embedded = self.ctx.as_ref().is_some_and(|c| c.embed_viewports());
        Ok(if embedded { crate::WindowId::ROOT.viewport() } else { id.viewport() })
    }

    fn viewport_of(&mut self, window: Option<u32>) -> Result<egui::ViewportId, String> {
        let id = crate::WindowId(window.unwrap_or(0));
        if !self.has_window(id) {
            return Err(format!("no window {}", id.0));
        }
        if id != crate::WindowId::ROOT {
            // eframe paints windows beyond the main one as immediate viewports and ignores a
            // screenshot request for them.
            return Err(format!(
                "no screenshot of window {}: only the main window (0) can be captured; use the system's screenshot tool for the others",
                id.0
            ));
        }
        Ok(id.viewport())
    }
}

// ---- transport (native only) -----------------------------------------------------------------

/// Where a running app's control channel can be reached (written to the `--control` file).
#[cfg(not(target_arch = "wasm32"))]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
}

/// Listen on `127.0.0.1` (random port) and forward authenticated requests to `client`.
///
/// Protocol: newline-delimited JSON-RPC 2.0. The first request on a connection must be
/// `{"method": "auth", "params": {"token": …}}`; anything else closes the connection, and so does
/// a line longer than [`MAX_AUTH_LINE`] before that.
#[cfg(not(target_arch = "wasm32"))]
pub fn serve(client: ControlClient) -> std::io::Result<Endpoint> {
    use std::io::{BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = random_token()?;
    let expected = token.clone();
    std::thread::Builder::new().name("pdfcraft-control".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let client = client.clone();
            let expected = expected.clone();
            let _ = std::thread::Builder::new().name("pdfcraft-control-conn".into()).spawn(move || {
                let Ok(read) = stream.try_clone() else { return };
                let mut write = stream;
                let mut authed = false;
                let mut reader = BufReader::new(read);
                let mut buf = Vec::new();
                loop {
                    // Until `auth` succeeds, lines are capped at MAX_AUTH_LINE; afterwards they
                    // are read whole, as before.
                    let max = if authed { usize::MAX } else { MAX_AUTH_LINE };
                    match read_line_limited(&mut reader, &mut buf, max) {
                        Ok(LineRead::Line) => {}
                        // End of stream or a failed read ends the connection, as before.
                        Ok(LineRead::End) | Err(_) => break,
                        Ok(LineRead::TooLong) => {
                            let e = rpc_error(Value::Null, -32600, &format!("request line longer than {max} bytes before auth"));
                            let _ = writeln!(write, "{e}").and_then(|_| write.flush());
                            break;
                        }
                    }
                    // Bytes that aren't UTF-8 end the connection, as `BufRead::lines` did.
                    let Ok(line) = std::str::from_utf8(&buf) else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let (reply, close) = match serde_json::from_str::<Value>(line) {
                        // Before `auth`, like any other first request, this closes the connection.
                        Err(e) => (rpc_error(Value::Null, -32700, &format!("parse error: {e}")), !authed),
                        Ok(msg) => {
                            let id = msg.get("id").cloned().unwrap_or(Value::Null);
                            let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
                            let params = msg.get("params").cloned().unwrap_or(Value::Null);
                            if !authed {
                                let ok =
                                    method == "auth" && params.get("token").and_then(Value::as_str).is_some_and(|t| constant_time_eq(t, &expected));
                                authed = ok;
                                if ok {
                                    (json!({ "jsonrpc": "2.0", "id": id, "result": { "ok": true } }), false)
                                } else {
                                    (rpc_error(id, -32001, "authenticate first: auth {token}"), true)
                                }
                            } else {
                                let rx = client.send(method, params);
                                match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                                    Ok(Ok(v)) => (json!({ "jsonrpc": "2.0", "id": id, "result": v }), false),
                                    Ok(Err(e)) => (rpc_error(id, -32000, &e), false),
                                    Err(_) => (rpc_error(id, -32002, "the app did not answer within 30 s"), false),
                                }
                            }
                        }
                    };
                    if writeln!(write, "{reply}").and_then(|_| write.flush()).is_err() || close {
                        break;
                    }
                }
            });
        }
    })?;
    Ok(Endpoint { port, token })
}

/// Longest line read before a connection has authenticated. The only acceptable request then is
/// `auth`, about a hundred bytes; a longer line is refused without buffering the rest of it.
#[cfg(not(target_arch = "wasm32"))]
const MAX_AUTH_LINE: usize = 4 * 1024;

/// What [`read_line_limited`] found.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
enum LineRead {
    /// A line, now in the buffer (newline and a trailing `\r` stripped).
    Line,
    /// End of stream.
    End,
    /// The line is longer than the limit; at most limit + 1 bytes of it were read.
    TooLong,
}

/// Read one `\n`-terminated line of at most `max` bytes into `buf`.
#[cfg(not(target_arch = "wasm32"))]
fn read_line_limited(reader: &mut impl std::io::BufRead, buf: &mut Vec<u8>, max: usize) -> std::io::Result<LineRead> {
    use std::io::{BufRead, Read};
    buf.clear();
    // `take` bounds what is buffered for one line: never more than `max` + 1 bytes.
    let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
    if reader.take(limit).read_until(b'\n', buf)? == 0 {
        return Ok(LineRead::End);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        return Ok(LineRead::Line);
    }
    // No newline: either the limit was passed, or this is a last line before end of stream.
    Ok(if buf.len() > max { LineRead::TooLong } else { LineRead::Line })
}

#[cfg(not(target_arch = "wasm32"))]
fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(not(target_arch = "wasm32"))]
fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 128 random bits from the OS, hex-encoded.
#[cfg(not(target_arch = "wasm32"))]
fn random_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod transport_tests {
    /// Before authenticating, a line that isn't JSON gets a parse error and closes the
    /// connection, as [`super::serve`] documents for any first request other than `auth`.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn malformed_input_before_auth_closes_the_connection() {
        use std::io::{BufRead, BufReader, ErrorKind, Write};
        let (_control, client) = super::attach(&egui::Context::default());
        let ep = super::serve(client).unwrap();
        let s = std::net::TcpStream::connect(("127.0.0.1", ep.port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        writeln!(s.try_clone().unwrap(), "not json").unwrap();
        let mut r = BufReader::new(s);
        let mut first = String::new();
        r.read_line(&mut first).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(reply["error"]["code"], -32700, "{first}");
        // Closed: end of stream (or a reset), not a connection left waiting for more input.
        let mut next = String::new();
        let closed = match r.read_line(&mut next) {
            Ok(n) => n == 0,
            Err(e) => matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted),
        };
        assert!(closed, "the connection stayed open: {next:?}");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn lines_are_read_up_to_the_limit() {
        use super::{LineRead, read_line_limited};
        let mut buf = Vec::new();
        let mut input: &[u8] = b"aaaaaaaaaa\nbb\r\ncc";
        assert_eq!(read_line_limited(&mut input, &mut buf, 10).unwrap(), LineRead::Line);
        assert_eq!(buf, b"aaaaaaaaaa", "a line exactly at the limit is accepted");
        assert_eq!(read_line_limited(&mut input, &mut buf, 10).unwrap(), LineRead::Line);
        assert_eq!(buf, b"bb", "CRLF is stripped");
        assert_eq!(read_line_limited(&mut input, &mut buf, 10).unwrap(), LineRead::Line);
        assert_eq!(buf, b"cc", "a last line without a newline is still read");
        assert_eq!(read_line_limited(&mut input, &mut buf, 10).unwrap(), LineRead::End, "then end of stream");
        let long = [b'x'; 100];
        let mut input: &[u8] = &long;
        assert_eq!(read_line_limited(&mut input, &mut buf, 10).unwrap(), LineRead::TooLong);
        assert_eq!(input.len(), 100 - 11, "at most limit + 1 bytes are read; the rest is left");
    }

    /// Before authenticating, a line longer than [`super::MAX_AUTH_LINE`] is refused as soon as
    /// the limit is passed (no newline needed) and the connection is closed.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_overlong_line_before_auth_is_refused() {
        use std::io::{BufRead, BufReader, ErrorKind, Write};
        let (_control, client) = super::attach(&egui::Context::default());
        let ep = super::serve(client).unwrap();
        let s = std::net::TcpStream::connect(("127.0.0.1", ep.port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        // Exactly one byte over the limit, without a newline: the server reads it all, so the
        // close below is a clean end of stream.
        s.try_clone().unwrap().write_all(&vec![b'x'; super::MAX_AUTH_LINE + 1]).unwrap();
        let mut r = BufReader::new(s);
        let mut first = String::new();
        r.read_line(&mut first).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(reply["error"]["code"], -32600, "{first}");
        let mut next = String::new();
        let closed = match r.read_line(&mut next) {
            Ok(n) => n == 0,
            Err(e) => matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted),
        };
        assert!(closed, "the connection stayed open: {next:?}");
    }

    /// After `auth` the limit no longer applies: a long line is read as before (here it isn't
    /// JSON, so it gets a parse error and the connection stays open).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn long_lines_after_auth_are_still_read() {
        use std::io::{BufRead, BufReader, Write};
        let (_control, client) = super::attach(&egui::Context::default());
        let ep = super::serve(client).unwrap();
        let s = std::net::TcpStream::connect(("127.0.0.1", ep.port)).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut w = s.try_clone().unwrap();
        let mut r = BufReader::new(s);
        let reply = |r: &mut BufReader<std::net::TcpStream>| {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            serde_json::from_str::<serde_json::Value>(&line).unwrap()
        };
        let auth = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "auth", "params": { "token": ep.token } });
        writeln!(w, "{auth}").unwrap();
        assert_eq!(reply(&mut r)["result"]["ok"], true);
        writeln!(w, "{}", "x".repeat(4 * super::MAX_AUTH_LINE)).unwrap();
        assert_eq!(reply(&mut r)["error"]["code"], -32700, "a long line after auth is parsed, not refused");
        writeln!(w, "y").unwrap();
        assert_eq!(reply(&mut r)["error"]["code"], -32700, "and the connection is still open");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn input(viewport: egui::ViewportId) -> egui::RawInput {
        let viewports = [egui::ViewportId::ROOT, child()].into_iter().map(|id| (id, egui::ViewportInfo::default())).collect();
        egui::RawInput { viewport_id: viewport, viewports, ..Default::default() }
    }

    /// Run a pass of `viewport` (the textures it made are not needed).
    fn run(ctx: &egui::Context, viewport: egui::ViewportId, ui_fn: impl FnMut(&mut egui::Ui)) {
        ctx.run_ui(input(viewport), ui_fn).textures_delta.clear();
    }

    /// One pass of `viewport` showing a button called `label`.
    fn pass(ctx: &egui::Context, viewport: egui::ViewportId, label: &str) {
        run(ctx, viewport, |ui| {
            let _ = ui.button(label.to_owned());
        });
    }

    fn labels(control: &Control, viewport: egui::ViewportId) -> Vec<String> {
        let v = control.inspect(viewport, &json!({ "role": "button" }));
        v["widgets"].as_array().unwrap().iter().filter_map(|w| w["label"].as_str().map(str::to_owned)).collect()
    }

    fn child() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("child window")
    }

    #[test]
    fn each_window_has_its_own_widget_tree_and_input() {
        let ctx = egui::Context::default();
        let (control, _client) = attach(&ctx);
        // The child's pass runs inside the main window's, as eframe draws immediate viewports.
        run(&ctx, egui::ViewportId::ROOT, |ui| {
            let _ = ui.button("In main");
            pass(ui.ctx(), child(), "In child");
        });
        assert_eq!(labels(&control, egui::ViewportId::ROOT), ["In main"]);
        assert_eq!(labels(&control, child()), ["In child"]);
        // Input for the child reaches the child's next pass only.
        control.inject(child(), vec![vec![egui::Event::Text("x".into())]]);
        let mut main_events = 0;
        let mut child_events = 0;
        run(&ctx, egui::ViewportId::ROOT, |ui| {
            main_events = ui.input(|i| i.events.len());
            run(ui.ctx(), child(), |ui| {
                child_events = ui.input(|i| i.events.iter().filter(|e| matches!(e, egui::Event::Text(_))).count());
            });
        });
        assert_eq!((main_events, child_events), (0, 1));
        assert_eq!(control.passes(child()), 2);
    }

    #[test]
    fn a_click_by_label_goes_to_the_window_that_has_the_button() {
        let ctx = egui::Context::default();
        let (mut control, _client) = attach(&ctx);
        pass(&ctx, child(), "Only here");
        let handled = control.click(child(), &json!({ "label": "Only here" })).unwrap();
        assert!(matches!(handled, Handled::AfterFrames(v, _, _) if v == child()));
        // The main window has no such button.
        assert!(control.click(egui::ViewportId::ROOT, &json!({ "label": "Only here" })).is_err());
    }

    #[test]
    fn a_window_that_was_never_drawn_has_nothing_to_inspect() {
        let ctx = egui::Context::default();
        let (control, _client) = attach(&ctx);
        let v = control.inspect(child(), &json!({}));
        assert_eq!(v["count"], 0);
    }
}
