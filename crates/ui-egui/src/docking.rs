//! Application panel identities and persistence around the shared docking renderer.
//! Document contents remain owned by their existing view implementations.

use craft_ui::docking::{Action, DockArea, DockStyle, Layout, Location, Node, PanelLimits, Permissions, Placement, Zone};
use craft_ui::layout::{SplitAxis, SplitSize};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Panel {
    Canvas,
    Tools,
    Inspector,
}

impl Panel {
    pub const ALL: &[Self] = &[Self::Canvas, Self::Tools, Self::Inspector];
    pub fn id(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::Tools => "tools",
            Self::Inspector => "inspector",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Canvas => "Document",
            Self::Tools => "Tools",
            Self::Inspector => "Inspector",
        }
    }
    fn default_side(self) -> Zone {
        match self {
            Self::Canvas => Zone::Right,
            Self::Tools => Zone::Left,
            Self::Inspector => Zone::Right,
        }
    }
    fn parse(id: &str) -> Result<Self, String> {
        Self::ALL.iter().copied().find(|panel| panel.id() == id).ok_or_else(|| format!("Unknown panel: {id}"))
    }
}

fn permissions(panel: &Panel) -> Permissions {
    if *panel == Panel::Canvas { Permissions::PROTECTED } else { Permissions::default() }
}

fn tabs(panel: Panel) -> Node<Panel> {
    Node::Tabs { panels: vec![panel], active: 0 }
}

fn split(axis: SplitAxis, size: SplitSize, first: Node<Panel>, second: Node<Panel>) -> Node<Panel> {
    Node::Split { axis, size, first: Box::new(first), second: Box::new(second) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub layout: Layout<Panel>,
    /// Closed panels retain their most recent group, side, size or floating rectangle.
    pub hidden: Vec<(Panel, Location<Panel>)>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            layout: Layout {
                root: Some(split(
                    SplitAxis::Horizontal,
                    SplitSize::FixedFirst(272.0),
                    tabs(Panel::Tools),
                    split(SplitAxis::Horizontal, SplitSize::FixedSecond(330.0), tabs(Panel::Canvas), tabs(Panel::Inspector)),
                )),
                floating: Vec::new(),
            },
            hidden: Vec::new(),
        }
    }
}

impl Workspace {
    /// Side of the Tools panel, including its saved position while closed.
    pub fn tools_on_right(&self) -> bool {
        let location = self
            .layout
            .location(&Panel::Tools)
            .ok()
            .or_else(|| self.hidden.iter().find(|(panel, _)| *panel == Panel::Tools).map(|(_, location)| location.clone()));
        location.is_some_and(|location| matches!(location.placement, Placement::Split(Zone::Right)))
    }

    /// Place Tools at the outer left or right edge while preserving its open state.
    pub fn set_tools_on_right(&mut self, right: bool) -> Result<(), String> {
        let was_open = self.layout.contains(&Panel::Tools);
        self.set_visible(Panel::Tools, true)?;
        let anchor = if right && self.layout.contains(&Panel::Inspector) { Panel::Inspector } else { Panel::Canvas };
        self.apply(Action::Move { panel: Panel::Tools, anchor, placement: Placement::Split(if right { Zone::Right } else { Zone::Left }) })?;
        if !was_open {
            self.set_visible(Panel::Tools, false)?;
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.layout.validate().map_err(|error| error.to_string())?;
        if !self.layout.contains(&Panel::Canvas) || self.hidden.len() > Panel::ALL.len() {
            return Err("The panel layout must contain its document view and bounded hidden panels".into());
        }
        // Saved settings cannot turn the protected document view into a floating or closable tab.
        if self.layout.floating.iter().any(|group| group.panels.contains(&Panel::Canvas)) {
            return Err("The document view cannot float".into());
        }
        let mut nodes: Vec<_> = self.layout.root.iter().collect();
        while let Some(node) = nodes.pop() {
            match node {
                Node::Split { first, second, .. } => {
                    nodes.push(first);
                    nodes.push(second);
                }
                Node::Tabs { panels, .. } if panels.contains(&Panel::Canvas) && panels.len() != 1 => {
                    return Err("The document view cannot share a tab group".into());
                }
                Node::Stack { entries } if entries.iter().any(|entry| entry.panel == Panel::Canvas) => {
                    return Err("The document view cannot be collapsed".into());
                }
                _ => {}
            }
        }
        let mut seen = std::collections::HashSet::new();
        for (panel, location) in &self.hidden {
            if *panel == Panel::Canvas || self.layout.contains(panel) || !seen.insert(*panel) {
                return Err("A hidden panel has an invalid or duplicate identity".into());
            }
            if location.anchor == Some(Panel::Canvas) && matches!(location.placement, Placement::Tab { .. } | Placement::Split(Zone::Center)) {
                return Err("A hidden panel cannot restore over the document view".into());
            }
            let mut probe = self.layout.clone();
            if let Err(error) = probe.restore(*panel, location)
                && error != craft_ui::docking::DockError::MissingTarget
            {
                return Err(error.to_string());
            }
        }
        Ok(())
    }

    pub fn apply(&mut self, action: Action<Panel>) -> Result<(), String> {
        self.validate()?;
        let mut next = self.clone();
        if let Action::Close { panel } = &action {
            let location = next.layout.location(panel).map_err(|error| error.to_string())?;
            next.hidden.retain(|(id, _)| id != panel);
            next.hidden.push((*panel, location));
        } else if let Action::Open { panel, .. } | Action::OpenAt { panel, .. } = &action {
            next.hidden.retain(|(id, _)| id != panel);
        }
        next.layout.apply_with_permissions(action, permissions).map_err(|error| error.to_string())?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Whether a panel's body is exposed by its selected tab or open stack entry.
    pub(crate) fn exposed(&self, panel: Panel) -> bool {
        if self.layout.floating.iter().any(|group| group.panels.get(group.active) == Some(&panel)) {
            return true;
        }
        let mut pending: Vec<_> = self.layout.root.iter().collect();
        while let Some(node) = pending.pop() {
            match node {
                Node::Split { first, second, .. } => {
                    pending.push(first);
                    pending.push(second);
                }
                Node::Tabs { panels, active } if panels.get(*active) == Some(&panel) => return true,
                Node::Stack { entries } if entries.iter().any(|entry| entry.panel == panel && entry.open) => return true,
                _ => {}
            }
        }
        false
    }

    pub fn set_visible(&mut self, panel: Panel, visible: bool) -> Result<(), String> {
        if self.layout.contains(&panel) == visible {
            return Ok(());
        }
        if !visible {
            return self.apply(Action::Close { panel });
        }
        self.validate()?;
        let mut next = self.clone();
        let mut location = next.hidden.iter().find(|(id, _)| *id == panel).map(|(_, location)| location.clone());
        next.hidden.retain(|(id, _)| *id != panel);
        if let Some(saved) = &mut location
            && saved.floating.is_some()
            && saved.anchor.as_ref().is_some_and(|anchor| !next.layout.floating.iter().any(|group| group.panels.contains(anchor)))
        {
            // The other members of a floating group may have been closed too.
            saved.anchor = None;
            saved.placement = Placement::Tab { before: None };
        }
        let restored = location.as_ref().is_some_and(|location| next.layout.restore(panel, location).is_ok());
        if !restored {
            next.layout
                .apply_with_permissions(
                    Action::OpenAt { panel, anchor: Panel::Canvas, placement: Placement::Split(panel.default_side()) },
                    permissions,
                )
                .map_err(|error| error.to_string())?;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn command(&mut self, params: &Value) -> Result<(), String> {
        self.validate()?;
        if let Some(action) = params.get("action") {
            let action: Action<Panel> = serde_json::from_value(action.clone()).map_err(|error| error.to_string())?;
            return self.apply(action);
        }
        let operation = params.get("operation").and_then(Value::as_str).ok_or("operation is required")?;
        if operation == "resizeSplit" {
            let path: Vec<bool> =
                serde_json::from_value(params.get("path").cloned().ok_or("path is required")?).map_err(|error| error.to_string())?;
            let size: SplitSize =
                serde_json::from_value(params.get("size").cloned().ok_or("size is required")?).map_err(|error| error.to_string())?;
            return self.apply(Action::ResizeSplit { path, size });
        }
        let panel = Panel::parse(params.get("panel").and_then(Value::as_str).ok_or("panel is required")?)?;
        let action = match operation {
            "open" => {
                self.set_visible(panel, true)?;
                return self.apply(Action::Activate { panel });
            }
            "close" => Action::Close { panel },
            "activate" => Action::Activate { panel },
            "setStackOpen" => Action::SetStackOpen { panel, open: params.get("open").and_then(Value::as_bool).ok_or("open must be a boolean")? },
            "resizeStack" => {
                let value = params.get("height").ok_or("height is required")?;
                let height = if value.is_null() {
                    None
                } else {
                    Some(
                        value
                            .as_f64()
                            .filter(|height| height.is_finite() && (0.0..=1_000_000.0).contains(height))
                            .ok_or("height must be finite and bounded")? as f32,
                    )
                };
                Action::ResizeStack { panel, height }
            }
            "float" | "moveFloating" => {
                let rect = params.get("rect").and_then(Value::as_array).filter(|rect| rect.len() == 4).ok_or("rect must be [x,y,width,height]")?;
                let mut values = [0.0f32; 4];
                for (output, value) in values.iter_mut().zip(rect) {
                    *output = value
                        .as_f64()
                        .filter(|number| number.is_finite() && number.abs() <= 1_000_000.0)
                        .ok_or("rect values must be finite and bounded")? as f32;
                }
                if operation == "float" { Action::Float { panel, rect: values } } else { Action::MoveFloating { panel, rect: values } }
            }
            "move" => {
                let anchor = Panel::parse(params.get("target").and_then(Value::as_str).ok_or("target is required")?)?;
                let placement = match params.get("zone").and_then(Value::as_str).unwrap_or("center") {
                    "center" => Placement::Split(Zone::Center),
                    "left" => Placement::Split(Zone::Left),
                    "right" => Placement::Split(Zone::Right),
                    "top" => Placement::Split(Zone::Top),
                    "bottom" => Placement::Split(Zone::Bottom),
                    _ => return Err("zone must be center, left, right, top or bottom".into()),
                };
                let placement = if let Some(before) = params.get("before") {
                    if !matches!(placement, Placement::Split(Zone::Center)) {
                        return Err("before only applies to a center move".into());
                    }
                    Placement::Tab {
                        before: if before.is_null() {
                            None
                        } else {
                            Some(Panel::parse(before.as_str().ok_or("before must be a panel ID or null")?)?)
                        },
                    }
                } else {
                    placement
                };
                Action::Move { panel, anchor, placement }
            }
            _ => return Err("operation must be open, close, activate, float, move, moveFloating, resizeSplit, setStackOpen or resizeStack".into()),
        };
        self.apply(action)
    }
}

fn sync(workspace: &mut Workspace, app: &crate::PdfCraftApp) -> Result<(), String> {
    workspace.set_visible(Panel::Tools, app.left_open)?;
    workspace.set_visible(Panel::Inspector, app.right.is_some())
}

fn drain_reveals(workspace: &mut Workspace, app: &mut crate::PdfCraftApp) {
    for panel in std::mem::take(&mut app.pending_panel_reveals) {
        if workspace.layout.contains(&panel) {
            let _ = workspace.apply(Action::Activate { panel });
        }
    }
}

fn publish_visibility(app: &mut crate::PdfCraftApp, workspace: &Workspace) {
    app.left_open = workspace.layout.contains(&Panel::Tools);
    if !workspace.layout.contains(&Panel::Inspector) {
        if app.right.is_some() {
            app.choose_right_panel(None);
        }
    } else if app.right.is_none() {
        app.choose_right_panel(Some(crate::RightPanel::Comments));
    }
}

pub fn command(app: &mut crate::PdfCraftApp, reset: bool, params: &Value) -> Result<Value, String> {
    let mut workspace = app.docking.clone();
    if reset {
        workspace = Workspace::default();
        workspace.set_visible(Panel::Inspector, app.right.is_some())?;
    } else {
        sync(&mut workspace, app)?;
        workspace.command(params)?;
    }
    publish_visibility(app, &workspace);
    app.docking = workspace;
    Ok(json!({"docking": app.docking}))
}

// Split paths change when a temporarily unavailable pane is pruned. Translate renderer
// paths back to the saved tree before applying geometry changes.
fn saved_split_path(saved: &Layout<Panel>, rendered: &Layout<Panel>, path: &[bool]) -> Option<Vec<bool>> {
    let mut target = rendered.root.as_ref()?;
    for second in path {
        let Node::Split { first, second: other, .. } = target else { return None };
        target = if *second { other } else { first };
    }
    let mut pending = vec![(saved.root.as_ref()?, Vec::new())];
    while let Some((node, original_path)) = pending.pop() {
        if let Node::Split { first, second, .. } = node {
            let mut projection = Layout { root: Some(node.clone()), floating: Vec::new() };
            if projection.contains(&Panel::Inspector) {
                let _ = projection.apply(Action::Close { panel: Panel::Inspector });
            }
            let has_available_panel = |child: &Node<Panel>| {
                Layout { root: Some(child.clone()), floating: Vec::new() }.panels().into_iter().any(|panel| *panel != Panel::Inspector)
            };
            // An ancestor whose Inspector-only side disappeared projects to its surviving
            // child. Its divider is absent, so only the child's split may own this resize.
            if has_available_panel(first) && has_available_panel(second) && projection.root.as_ref() == Some(target) {
                return Some(original_path);
            }
            let mut second_path = original_path.clone();
            second_path.push(true);
            pending.push((second, second_path));
            let mut first_path = original_path;
            first_path.push(false);
            pending.push((first, first_path));
        }
    }
    None
}

pub fn show(app: &mut crate::PdfCraftApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let mut workspace = std::mem::take(&mut app.docking);
    if let Err(error) = workspace.validate().and_then(|()| sync(&mut workspace, app)) {
        app.notify_fmt("Panel layout could not be restored: {error}", &[("error", &error)]);
        workspace = Workspace::default();
        let _ = sync(&mut workspace, app);
    }
    let mut style = DockStyle::from_ui(ui);
    style.min_pane = 96.0;
    style.float_label = crate::i18n::t("Float panel").into();
    style.close_label = crate::i18n::t("Close panel").into();
    style.move_label = crate::i18n::t("Move to group").into();
    style.panels_label = crate::i18n::t("Panels").into();
    style.resize_label = crate::i18n::t("Resize panels").into();
    style.resize_window_label = crate::i18n::t("Resize panel window").into();
    style.background = t.panel;
    style.tab_background = t.chrome;
    style.active_background = t.selected;
    style.text = t.text;
    style.inactive_text = t.text_muted;
    style.border = egui::Stroke::new(1.0, t.divider);
    style.accent = t.accent;
    style.font = crate::theme::medium(12.0);
    drain_reveals(&mut workspace, app);
    let mut rendered = workspace.layout.clone();
    if app.active.is_none() && rendered.contains(&Panel::Inspector) {
        // Availability is a render projection: the saved group, selection and width survive Home.
        let _ = rendered.apply(Action::Close { panel: Panel::Inspector });
    }
    // A right-to-left interface shows the arrangement mirrored: what is saved is the same in
    // every language, so the Tools panel that is on the left in English is on the right here.
    let mirror = crate::rtl::on();
    let shown = if mirror { mirrored_layout(&rendered) } else { rendered.clone() };
    let output = DockArea::new(egui::Id::new("pdfcraft-docking")).show_with_limits(
        ui,
        &shown,
        &style,
        |panel| crate::i18n::t(panel.label()).into(),
        permissions,
        panel_limits,
        |ui, panel| match panel {
            Panel::Canvas => {
                ui.painter().rect_filled(ui.max_rect(), 0.0, t.pasteboard);
                // Home and Combine Files follow the interface; a document never does.
                match app.active {
                    None if app.combine_showing() => crate::rtl::root(ui, |ui| crate::combine_ui::page(app, ui)),
                    None => crate::rtl::root(ui, |ui| crate::home::show(app, ui)),
                    Some(index) => crate::canvas::document_area(app, index, ui),
                }
            }
            Panel::Tools => crate::rtl::root(ui, |ui| crate::panels::left_panel(app, ui)),
            Panel::Inspector => {
                ui.data_mut(|data| data.insert_temp(egui::Id::new("pdfcraft-inspector-body"), ui.max_rect()));
                crate::rtl::root(ui, |ui| crate::panels::right_panel(app, ui));
            }
        },
    );
    let _ = sync(&mut workspace, app);
    drain_reveals(&mut workspace, app);
    publish_visibility(app, &workspace);
    app.docking = workspace;
    for mut action in output.actions {
        if mirror {
            // The gesture was made on the mirrored arrangement.
            let Some(saved) = unmirrored_action(&rendered, action) else { continue };
            action = saved;
        }
        if app.active.is_none()
            && let Action::ResizeSplit { path, .. } = &mut action
        {
            let Some(saved_path) = saved_split_path(&app.docking.layout, &rendered, path) else { continue };
            *path = saved_path;
        }
        if let Err(error) =
            serde_json::to_value(action).map_err(|error| error.to_string()).and_then(|action| command(app, false, &json!({"action": action})))
        {
            app.notify_fmt("Panel move was not applied: {error}", &[("error", &error)]);
        }
    }
}

/// Splits nest; deeper than this the arrangement is shown as saved rather than recursed into.
const MAX_MIRROR_DEPTH: usize = 64;

/// A side-by-side split's size once its children have swapped sides.
fn mirrored_size(size: SplitSize) -> SplitSize {
    match size {
        SplitSize::Ratio(ratio) => SplitSize::Ratio(if ratio.is_finite() { (1.0 - ratio).clamp(0.0, 1.0) } else { ratio }),
        SplitSize::FixedFirst(points) => SplitSize::FixedSecond(points),
        SplitSize::FixedSecond(points) => SplitSize::FixedFirst(points),
    }
}

fn mirrored_node(node: &Node<Panel>, depth: usize) -> Node<Panel> {
    match node {
        Node::Split { axis, size, first, second } if depth < MAX_MIRROR_DEPTH => {
            let (first, second) = (mirrored_node(first, depth.saturating_add(1)), mirrored_node(second, depth.saturating_add(1)));
            match axis {
                SplitAxis::Horizontal => Node::Split { axis: *axis, size: mirrored_size(*size), first: Box::new(second), second: Box::new(first) },
                SplitAxis::Vertical => Node::Split { axis: *axis, size: *size, first: Box::new(first), second: Box::new(second) },
            }
        }
        other => other.clone(),
    }
}

/// `layout` as a right-to-left interface shows it: the children of every side-by-side split
/// swap sides and keep their sizes. Tab order, stacks and floating panels are as saved.
fn mirrored_layout(layout: &Layout<Panel>) -> Layout<Panel> {
    Layout { root: layout.root.as_ref().map(|root| mirrored_node(root, 0)), floating: layout.floating.clone() }
}

/// An action made on [`mirrored_layout`] of `layout`, as it applies to `layout` itself: left and
/// right swap. `None` for a split that `layout` does not have.
fn unmirrored_action(layout: &Layout<Panel>, action: Action<Panel>) -> Option<Action<Panel>> {
    let side = |placement: Placement<Panel>| match placement {
        Placement::Split(Zone::Left) => Placement::Split(Zone::Right),
        Placement::Split(Zone::Right) => Placement::Split(Zone::Left),
        other => other,
    };
    Some(match action {
        Action::Move { panel, anchor, placement } => Action::Move { panel, anchor, placement: side(placement) },
        Action::OpenAt { panel, anchor, placement } => Action::OpenAt { panel, anchor, placement: side(placement) },
        Action::ResizeSplit { path, size } => {
            // Walk the saved tree: at a side-by-side split the mirrored first child is the saved second.
            let mut node = layout.root.as_ref()?;
            let mut saved = Vec::with_capacity(path.len());
            for (depth, shown_second) in path.iter().enumerate() {
                let Node::Split { axis, first, second, .. } = node else { return None };
                let swapped = *axis == SplitAxis::Horizontal && depth < MAX_MIRROR_DEPTH;
                let second_child = *shown_second != swapped;
                saved.push(second_child);
                node = if second_child { second } else { first };
            }
            let Node::Split { axis, .. } = node else { return None };
            let swapped = *axis == SplitAxis::Horizontal && path.len() < MAX_MIRROR_DEPTH;
            Action::ResizeSplit { path: saved, size: if swapped { mirrored_size(size) } else { size } }
        }
        other => other,
    })
}

fn panel_limits(panel: &Panel) -> PanelLimits {
    let (min, max_width) = match panel {
        Panel::Canvas => (egui::vec2(160.0, 160.0), 1_000_000.0),
        Panel::Tools => (egui::vec2(240.0, 120.0), 520.0),
        Panel::Inspector => (egui::vec2(260.0, 120.0), 520.0),
    };
    PanelLimits { min, max: egui::vec2(max_width, 1_000_000.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panels of `node` from left to right, ignoring stacked (top/bottom) order.
    fn left_to_right(node: &Node<Panel>, out: &mut Vec<Panel>) {
        match node {
            Node::Split { first, second, .. } => {
                left_to_right(first, out);
                left_to_right(second, out);
            }
            Node::Tabs { panels, .. } => out.extend(panels.iter().copied()),
            Node::Stack { entries } => out.extend(entries.iter().map(|entry| entry.panel)),
        }
    }

    #[test]
    fn a_right_to_left_interface_shows_the_arrangement_mirrored() {
        let saved = Workspace::default().layout;
        let shown = mirrored_layout(&saved);
        let (mut before, mut after) = (Vec::new(), Vec::new());
        left_to_right(saved.root.as_ref().unwrap(), &mut before);
        left_to_right(shown.root.as_ref().unwrap(), &mut after);
        assert_eq!(before, [Panel::Tools, Panel::Canvas, Panel::Inspector]);
        assert_eq!(after, [Panel::Inspector, Panel::Canvas, Panel::Tools]);
        // Mirroring twice is the saved arrangement, sizes included.
        assert_eq!(mirrored_layout(&shown), saved);
        // A panel keeps its width on the other side.
        assert_eq!(mirrored_size(SplitSize::FixedFirst(272.0)), SplitSize::FixedSecond(272.0));
        assert_eq!(mirrored_size(SplitSize::FixedSecond(330.0)), SplitSize::FixedFirst(330.0));
        assert_eq!(mirrored_size(SplitSize::Ratio(0.25)), SplitSize::Ratio(0.75));
        // A top/bottom split is not mirrored.
        let stacked =
            Layout { root: Some(split(SplitAxis::Vertical, SplitSize::Ratio(0.3), tabs(Panel::Tools), tabs(Panel::Canvas))), floating: Vec::new() };
        assert_eq!(mirrored_layout(&stacked), stacked);
    }

    #[test]
    fn gestures_on_the_mirrored_arrangement_apply_to_the_saved_one() {
        let saved = Workspace::default().layout;
        // Left and right swap; everything else is as made.
        let moved = Action::Move { panel: Panel::Tools, anchor: Panel::Canvas, placement: Placement::Split(Zone::Left) };
        assert_eq!(
            unmirrored_action(&saved, moved),
            Some(Action::Move { panel: Panel::Tools, anchor: Panel::Canvas, placement: Placement::Split(Zone::Right) })
        );
        let top = Action::Move { panel: Panel::Tools, anchor: Panel::Canvas, placement: Placement::Split(Zone::Top) };
        assert_eq!(unmirrored_action(&saved, top.clone()), Some(top));
        let closed = Action::Close { panel: Panel::Inspector };
        assert_eq!(unmirrored_action(&saved, closed.clone()), Some(closed));

        // Resizing a split of the mirrored tree resizes the same split of the saved one: applying
        // the translated action and mirroring gives what applying it to the mirrored tree gives.
        let shown = mirrored_layout(&saved);
        for path in [vec![], vec![false], vec![true]] {
            for size in [SplitSize::FixedFirst(300.0), SplitSize::FixedSecond(280.0), SplitSize::Ratio(0.25)] {
                let on_shown = Action::ResizeSplit { path: path.clone(), size };
                let mut expected = shown.clone();
                let applies = expected.apply(on_shown.clone()).is_ok();
                let translated = unmirrored_action(&saved, on_shown);
                assert_eq!(translated.is_some(), applies, "{path:?}");
                if let Some(action) = translated {
                    let mut actual = saved.clone();
                    actual.apply(action).unwrap();
                    assert_eq!(mirrored_layout(&actual), expected, "{path:?} {size:?}");
                }
            }
        }
        // A split the arrangement does not have is dropped, never a panic.
        assert_eq!(unmirrored_action(&saved, Action::ResizeSplit { path: vec![true; 200], size: SplitSize::Ratio(0.5) }), None);
        assert_eq!(unmirrored_action(&Layout::default(), Action::ResizeSplit { path: Vec::new(), size: SplitSize::Ratio(0.5) }), None);
    }

    #[test]
    fn lithuanian_translates_every_dynamic_panel_title() {
        let lang = crate::i18n::Lang::from_code("lt").expect("Lithuanian is registered");
        for panel in Panel::ALL {
            assert_ne!(crate::i18n::tr(lang, panel.label()), panel.label(), "{} remains untranslated", panel.id());
        }
    }

    #[test]
    fn tools_side_moves_to_both_edges_and_survives_closed_restore() {
        let mut workspace = Workspace::default();
        assert!(!workspace.tools_on_right());
        workspace.set_tools_on_right(true).unwrap();
        assert!(workspace.tools_on_right());
        workspace.set_visible(Panel::Tools, false).unwrap();
        assert!(workspace.tools_on_right());
        workspace.set_tools_on_right(false).unwrap();
        assert!(!workspace.tools_on_right());
        assert!(!workspace.layout.contains(&Panel::Tools));
        workspace.set_visible(Panel::Tools, true).unwrap();
        assert!(!workspace.tools_on_right());
        workspace.validate().unwrap();
    }

    #[test]
    fn exposed_respects_collapsed_stack_and_inactive_floating_tabs() {
        let mut workspace = Workspace {
            layout: Layout {
                root: Some(split(
                    SplitAxis::Horizontal,
                    SplitSize::FixedFirst(272.0),
                    Node::Stack {
                        entries: vec![
                            craft_ui::docking::StackEntry { panel: Panel::Tools, open: true, height: None },
                            craft_ui::docking::StackEntry { panel: Panel::Inspector, open: false, height: None },
                        ],
                    },
                    tabs(Panel::Canvas),
                )),
                floating: Vec::new(),
            },
            hidden: Vec::new(),
        };
        workspace.validate().unwrap();
        assert!(workspace.exposed(Panel::Tools));
        assert!(!workspace.exposed(Panel::Inspector));
        workspace.apply(Action::Activate { panel: Panel::Inspector }).unwrap();
        assert!(workspace.exposed(Panel::Inspector));
        workspace.apply(Action::Float { panel: Panel::Tools, rect: [10.0, 20.0, 300.0, 400.0] }).unwrap();
        workspace.apply(Action::Move { panel: Panel::Inspector, anchor: Panel::Tools, placement: Placement::Split(Zone::Center) }).unwrap();
        assert!(!workspace.exposed(Panel::Tools));
        assert!(workspace.exposed(Panel::Inspector));
    }

    #[test]
    fn unavailable_inspector_split_resize_targets_saved_tools_split() {
        let mut workspace = Workspace::default();
        // Inspector at the outer edge makes the visible Tools split move to the root.
        workspace.apply(Action::Move { panel: Panel::Inspector, anchor: Panel::Tools, placement: Placement::Split(Zone::Left) }).unwrap();
        let mut rendered = workspace.layout.clone();
        rendered.apply(Action::Close { panel: Panel::Inspector }).unwrap();
        let path = saved_split_path(&workspace.layout, &rendered, &[]).unwrap();
        workspace.apply(Action::ResizeSplit { path, size: SplitSize::FixedFirst(290.0) }).unwrap();
        let mut resized = workspace.layout.clone();
        resized.apply(Action::Close { panel: Panel::Inspector }).unwrap();
        assert!(matches!(resized.root, Some(Node::Split { size: SplitSize::FixedFirst(290.0), .. })));
    }

    #[test]
    fn unavailable_outer_inspector_does_not_capture_its_childs_resize() {
        let mut workspace = Workspace {
            layout: Layout {
                root: Some(split(
                    SplitAxis::Horizontal,
                    SplitSize::FixedSecond(330.0),
                    split(SplitAxis::Horizontal, SplitSize::FixedFirst(272.0), tabs(Panel::Tools), tabs(Panel::Canvas)),
                    tabs(Panel::Inspector),
                )),
                floating: Vec::new(),
            },
            hidden: Vec::new(),
        };
        workspace.validate().unwrap();
        let mut rendered = workspace.layout.clone();
        rendered.apply(Action::Close { panel: Panel::Inspector }).unwrap();
        let path = saved_split_path(&workspace.layout, &rendered, &[]).unwrap();
        assert_eq!(path, vec![false]);
        workspace.apply(Action::ResizeSplit { path, size: SplitSize::FixedFirst(290.0) }).unwrap();
        assert!(matches!(workspace.layout.root, Some(Node::Split { size: SplitSize::FixedSecond(330.0), .. })));
        let mut resized = workspace.layout.clone();
        resized.apply(Action::Close { panel: Panel::Inspector }).unwrap();
        assert!(matches!(resized.root, Some(Node::Split { size: SplitSize::FixedFirst(290.0), .. })));
    }
}
