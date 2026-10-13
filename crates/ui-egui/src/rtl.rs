//! Mirrored window layout for a right-to-left interface language (Arabic). The scheme is the one
//! WordCraft uses (`wordcraft/crates/ui-egui/src/rtl.rs`).
//!
//! egui has no global direction, but `ui.horizontal` runs right to left wherever the enclosing
//! layout prefers it (`Layout::prefer_right_to_left`): a right-to-left row, or a column whose
//! items are aligned to the right. So direction is inherited from a root:
//!
//! - A panel (which has its width) wraps its body in [`root`]. A container that takes the size of
//!   its content is given the layout itself, or a right-aligned body would stretch it to all the
//!   room it may take: [`modal`], [`area_layout`], [`menu`], [`context_menu`], [`menu_root`].
//! - Columns are opened with [`vertical`] (egui's `ui.vertical` always resets to left-aligned),
//!   and "far side" clusters use [`far_side`]. The rail is an [`end_panel`]; the docked panels swap
//!   sides in `docking`.
//! - What egui lays out from the left inside itself is mirrored in place after it is painted
//!   ([`mirrored_widget`]: check boxes and radio buttons in `widgets`, [`menu_item`], [`combo`]), and so is a widget that paints
//!   its own parts at left-to-right positions ([`mirror_scope`]). Parts move; text, icons and
//!   drawn marks are never flipped.
//! - What egui can only lay out from the left as a whole (a `Grid`, a sideways scroll area) is
//!   drawn in a left-to-right [`island`] set against the right edge, and stays left to right
//!   inside: [`flows`] follows the enclosing layout, not just the language.
//!
//! The document is never mirrored: its direction belongs to the document. In a left-to-right
//! language every function here is the egui call it stands for.

use egui::{Align, InnerResponse, Layout, Rect, Shape, Ui, UiBuilder, pos2};

/// Is the interface language written right to left?
pub fn on() -> bool {
    crate::i18n::current().rtl()
}

/// Is `ui` part of the mirrored interface: a right-to-left language, in an area that lays its
/// content out from the right (so rows opened in it run right to left)? Never in a left-to-right
/// language, whose far-side clusters also fill from the right but are not mirrored.
pub fn flows(ui: &Ui) -> bool {
    on() && ui.layout().prefer_right_to_left()
}

/// Run a container's body (a panel, window, popup or menu, which egui starts left-aligned) in the
/// interface's direction. In a left-to-right language this is just `add`.
pub fn root<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !on() || flows(ui) {
        return add(ui);
    }
    ui.scope_builder(UiBuilder::new().layout(Layout::top_down(Align::Max)), add).inner
}

/// The layout of a floating area's body (`egui::Area::layout`) in the interface's direction. Set
/// on the area itself, so the area still takes the size of its content: a right-aligned body
/// inside a left-aligned one would stretch it to all the room there is.
pub fn area_layout() -> Layout {
    if on() { Layout::top_down(Align::Max) } else { Layout::default() }
}

/// `egui::Modal::new`, with its body in the interface's direction.
pub fn modal(id: egui::Id) -> egui::Modal {
    let modal = egui::Modal::new(id);
    if on() { modal.area(egui::Modal::default_area(id).layout(area_layout())) } else { modal }
}

/// `ui.vertical`, aligned to the side rows start from: a column inside a right-to-left area keeps
/// its content on the right and its rows right to left.
pub fn vertical<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    if flows(ui) { ui.scope_builder(UiBuilder::new().layout(Layout::top_down(Align::Max)), add) } else { ui.vertical(add) }
}

/// The layout for a row's items that belong at its far end (the side rows end on): filled from
/// that end inwards.
pub fn far_side(ui: &Ui, align: Align) -> Layout {
    if flows(ui) { Layout::left_to_right(align) } else { Layout::right_to_left(align) }
}

/// The chevron icon that points back the way reading came from (left; right in a right-to-left
/// interface).
pub fn back_chevron() -> &'static str {
    if on() { "chevron-right" } else { "chevron-left" }
}

/// The chevron icon that points the way reading goes.
pub fn forward_chevron() -> &'static str {
    if on() { "chevron-left" } else { "chevron-right" }
}

/// `inner`, a part of a widget painted in `outer`, where a right-to-left interface draws it: as
/// far from the right edge as it was from the left. Unchanged when not `rtl`.
pub fn mirrored(rtl: bool, outer: Rect, inner: Rect) -> Rect {
    if rtl {
        Rect::from_min_max(pos2(mirrored_x(true, outer, inner.max.x), inner.min.y), pos2(mirrored_x(true, outer, inner.min.x), inner.max.y))
    } else {
        inner
    }
}

/// The x coordinate `x` of a widget painted in `outer`, mirrored when `rtl`.
pub fn mirrored_x(rtl: bool, outer: Rect, x: f32) -> f32 {
    if rtl { outer.min.x + outer.max.x - x } else { x }
}

/// A side panel on the side reading ends on (right; left in a right-to-left interface).
pub fn end_panel(id: &'static str) -> egui::Panel {
    if on() { egui::Panel::left(id) } else { egui::Panel::right(id) }
}

/// `egui::Popup::menu`, opened under the leading edge of its button, with its items aligned to
/// the side reading starts from.
pub fn menu(response: &egui::Response) -> egui::Popup<'_> {
    let popup = egui::Popup::menu(response);
    if on() { popup.layout(menu_layout()).align(egui::RectAlign::BOTTOM_END) } else { popup }
}

/// A menu's layout: full-width items whose text starts at the leading edge (a button places its
/// content where its layout aligns).
pub fn menu_layout() -> Layout {
    Layout::top_down_justified(if on() { Align::Max } else { Align::Min })
}

/// Run the body of a submenu (which egui lays out from the left, in a popup as wide as its
/// content) in the interface's direction.
///
/// A right-aligned body cannot simply be nested in the left-aligned one: it would start from the
/// far edge of all the room a popup may take and stretch the menu to it. So the body is measured
/// laid out from the left the first time, and from then on laid out from the right in that width.
pub fn menu_root<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !on() || flows(ui) {
        return add(ui);
    }
    let key = ui.id().with("rtl_menu_width");
    let known = ui.data(|d| d.get_temp::<f32>(key)).filter(|width| width.is_finite() && *width > 0.0);
    let room = ui.available_rect_before_wrap();
    let builder = match known {
        Some(width) if room.min.x.is_finite() => {
            UiBuilder::new().max_rect(Rect::from_min_max(room.min, pos2(room.min.x + width, room.max.y))).layout(menu_layout())
        }
        _ => UiBuilder::new(),
    };
    let shown = ui.scope_builder(builder, add);
    let now = shown.response.rect.width();
    if now.is_finite() && known.is_none_or(|width| (width - now).abs() > 0.5) {
        ui.data_mut(|d| d.insert_temp(key, now));
        ui.ctx().request_repaint();
    }
    shown.inner
}

/// A menu item: `text`, with its keyboard shortcut at the far side.
pub fn menu_item(ui: &mut Ui, enabled: bool, text: &str, shortcut: &str) -> egui::Response {
    // egui puts the label at the left of the item and the shortcut at its right: swap them
    // across the whole item (a label alone covers only itself).
    let scope = on().then(|| MirrorScope::begin(ui, None));
    let response = ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(shortcut));
    if let Some(mut scope) = scope {
        scope.outer = Some(response.rect);
    }
    response
}

/// `ui.menu_button` inside a menu: a submenu that opens on the side reading goes to, with its
/// arrow there and pointing that way, and its items in the interface's direction.
pub fn menu_button<R>(ui: &mut Ui, text: &str, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
    use egui::containers::menu::{MenuState, SubMenu};
    if !on() {
        return ui.menu_button(text, add);
    }
    // egui's own submenu button (`SubMenuButton::ui`), with the arrow leading.
    let popup = SubMenu::id_from_widget_id(ui.next_auto_id());
    let open = MenuState::from_ui(ui, |state, _| state.open_item == Some(popup));
    let inactive = ui.style().visuals.widgets.inactive;
    if open {
        ui.style_mut().visuals.widgets.inactive = ui.style().visuals.widgets.open;
    }
    let response = ui.add(egui::Button::new(SUBMENU_ARROW).right_text(text));
    ui.style_mut().visuals.widgets.inactive = inactive;
    // Named by its label alone, as in a left-to-right menu: the arrow is decoration.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), text));

    // egui opens a submenu to the right of the rectangle its button interacts in, and keeps it
    // there when it fits on neither side (a tall submenu in a short window), over the menu it
    // came from. So it is given a rectangle as far to the left of the button as the submenu is
    // wide, when there is that much room: its right-hand side is then the button's left.
    let mut anchor = response.clone();
    let width = egui::AreaState::load(ui.ctx(), popup).and_then(|state| state.size).map_or(0.0, |size| size.x);
    let gap = egui::Frame::menu(ui.style()).total_margin().sum().x / 2.0 + 2.0;
    let left_of_button = response.rect.left() - 2.0 * gap - width;
    if left_of_button.is_finite() && left_of_button >= ui.ctx().content_rect().left() {
        anchor.interact_rect = Rect::from_x_y_ranges(left_of_button..=left_of_button, response.interact_rect.y_range());
    }
    let inner = SubMenu::new().show(ui, &anchor, |ui| menu_root(ui, add));
    InnerResponse::new(inner.map(|shown| shown.inner), response)
}

/// The arrow of a right-to-left submenu (egui's own points right).
const SUBMENU_ARROW: &str = "\u{23F4}";

/// `Response::context_menu`, with its items aligned to the side reading starts from.
pub fn context_menu(response: &egui::Response, add: impl FnOnce(&mut Ui)) {
    let popup = egui::Popup::context_menu(response);
    let popup = if on() { popup.layout(menu_layout()) } else { popup };
    popup.show(add);
}

/// Run `add` in a left-to-right island inside a right-to-left area, for content egui can only
/// lay out from the left (a `Grid`, a horizontally scrolling strip). The island is set against the
/// leading (right) edge using the width it had last frame. Outside a right-to-left area this is
/// just `add`.
pub fn island<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    island_with(ui, Layout::top_down(Align::Min), add)
}

/// [`island`] for a strip inside a row: its content runs left to right, centred on the row.
pub fn island_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    island_with(ui, Layout::left_to_right(Align::Center), add)
}

fn island_with<R>(ui: &mut Ui, layout: Layout, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !flows(ui) {
        return add(ui);
    }
    // The island gets all the room there is, so its content takes its natural width; it starts
    // where content of last frame's width would end at the leading edge. A change of width shows
    // one frame late, so that frame asks for another.
    let key = ui.next_auto_id().with("rtl_island_width");
    let known = ui.data(|d| d.get_temp::<f32>(key));
    let room = ui.available_rect_before_wrap();
    let available = if room.width().is_finite() { room.width().max(0.0) } else { 0.0 };
    let width = known.unwrap_or(available);
    let left = room.max.x - width;
    if !left.is_finite() {
        return add(ui);
    }
    let max_rect = Rect::from_min_max(pos2(left, room.min.y), pos2(left + available.max(width), room.max.y));
    let shown = ui.scope_builder(UiBuilder::new().max_rect(max_rect).layout(layout), add);
    let now = shown.response.rect.width();
    if now.is_finite() && known.is_none_or(|w| (w - now).abs() > 0.5) {
        ui.data_mut(|d| d.insert_temp(key, now));
        ui.ctx().request_repaint();
    }
    shown.inner
}

/// `grid.show(ui, add)`. egui's `Grid` cannot be laid out from the right (it places cells from a
/// left edge that a right-to-left parent does not have, and says so with a debug assertion), so
/// inside a right-to-left area it is drawn in an [`island`]. Its columns keep their order.
pub fn grid<R>(ui: &mut Ui, grid: egui::Grid, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    island(ui, |ui| grid.show(ui, add))
}

/// `combo.show_ui(ui, add)`, with the box's arrow on the side reading goes to and the items of
/// its list starting from the right.
pub fn combo<R>(ui: &mut Ui, combo: egui::ComboBox, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
    if !on() {
        return combo.show_ui(ui, add);
    }
    // The list is on its own layer; what is painted here is the box, mirrored within itself.
    let mut scope = MirrorScope::begin(ui, None);
    let shown = combo.show_ui(ui, |ui| menu_root(ui, add));
    scope.outer = Some(shown.response.rect);
    shown
}

/// Add a widget that egui lays out from the left inside itself (a check box and its label, a
/// menu item and its shortcut) and swap the ends its parts are at. It is still one widget, with
/// the same name and the same place to click.
///
/// The parts are mirrored within what they cover, not within the widget's rectangle: a widget
/// stretched across a menu already has its content at the right.
pub fn mirrored_widget(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> egui::Response) -> egui::Response {
    let _mirror = on().then(|| MirrorScope::begin(ui, None));
    add(ui)
}

/// Mirrors a hand-painted widget: see [`mirror_scope`].
#[must_use = "the painting is mirrored when this goes out of scope"]
pub struct MirrorScope {
    ctx: egui::Context,
    layer: egui::LayerId,
    start: usize,
    /// What to mirror inside; `None` for the bounds of what was painted.
    outer: Option<Rect>,
}

/// Mirror, inside `outer`, everything painted on `ui`'s layer from now until the returned guard
/// goes out of scope. A widget that paints its parts at positions worked out for a left-to-right
/// interface (an icon, then a label, a badge at the far end) holds one and reads from the right
/// instead. Parts move, they are not flipped: text, images and drawn marks keep their own
/// orientation; a rounded rectangle swaps its corners. `None`, and nothing happens, in a
/// left-to-right language.
///
/// Only painting moves. A part with its own `ui.interact` rectangle paints at the left-to-right
/// place and interacts at the [`mirrored`] one.
pub fn mirror_scope(ui: &Ui, outer: Rect) -> Option<MirrorScope> {
    if !on() {
        return None;
    }
    Some(MirrorScope::begin(ui, Some(outer)))
}

impl MirrorScope {
    fn begin(ui: &Ui, outer: Option<Rect>) -> Self {
        let (ctx, layer) = (ui.ctx().clone(), ui.layer_id());
        let start = ctx.graphics_mut(|layers| layers.entry(layer).next_idx().0);
        MirrorScope { ctx, layer, start, outer }
    }
}

impl Drop for MirrorScope {
    fn drop(&mut self) {
        let start = self.start;
        self.ctx.graphics_mut(|layers| {
            let list = layers.entry(self.layer);
            // Without a rectangle to mirror in, the parts swap ends within what they cover.
            let outer = self.outer.unwrap_or_else(|| {
                let mut bounds = Rect::NOTHING;
                for clipped in list.all_entries().skip(start) {
                    let part = clipped.shape.visual_bounding_rect();
                    if part.is_finite() && part.is_positive() {
                        bounds |= part;
                    }
                }
                bounds
            });
            if !(outer.is_finite() && outer.is_positive()) {
                return;
            }
            for index in start..list.next_idx().0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    // A clip set for one part (a label cut to its column) moves with the part;
                    // the panel's own clip stays.
                    if outer.expand(2.0).contains_rect(clipped.clip_rect) {
                        clipped.clip_rect = mirrored(true, outer, clipped.clip_rect);
                    }
                    mirror_shape(&mut clipped.shape, outer, 0);
                });
            }
        });
    }
}

/// Shapes nest (`Shape::Vec`); deeper than this is left alone rather than recursed into.
const MAX_SHAPE_DEPTH: usize = 16;

fn mirror_shape(shape: &mut Shape, outer: Rect, depth: usize) {
    // How far something occupying `block` moves.
    let shift = |block: Rect| if block.is_finite() { mirrored(true, outer, block).min.x - block.min.x } else { 0.0 };
    match shape {
        Shape::Noop => {}
        Shape::Vec(shapes) => {
            if depth < MAX_SHAPE_DEPTH {
                for shape in shapes {
                    mirror_shape(shape, outer, depth.saturating_add(1));
                }
            }
        }
        Shape::Rect(rect) => {
            rect.rect = mirrored(true, outer, rect.rect);
            let radius = &mut rect.corner_radius;
            std::mem::swap(&mut radius.nw, &mut radius.ne);
            std::mem::swap(&mut radius.sw, &mut radius.se);
        }
        // The text keeps its own direction; the block it occupies moves.
        Shape::Text(text) => {
            text.pos.x += shift(text.galley.rect.translate(text.pos.to_vec2()));
            // A wrapped paragraph that started each line at the left ends each at the right.
            if text.galley.rows.len() > 1 && text.galley.job.halign == Align::LEFT {
                let galley = std::sync::Arc::make_mut(&mut text.galley);
                let block = galley.rect;
                for row in &mut galley.rows {
                    let line = row.rect();
                    if line.is_finite() && block.is_finite() {
                        row.pos.x += mirrored(true, block, line).min.x - line.min.x;
                    }
                }
                // Lines moved within the block, so it bounds them all.
                galley.mesh_bounds = galley.mesh_bounds.union(block);
            }
        }
        // A picture or a drawn mark (a tick, an arrow): moved as it is.
        Shape::Mesh(mesh) => {
            let by = shift(mesh.calc_bounds());
            for vertex in &mut std::sync::Arc::make_mut(mesh).vertices {
                vertex.pos.x += by;
            }
        }
        Shape::Callback(callback) => callback.rect = mirrored(true, outer, callback.rect),
        other => {
            let by = shift(other.visual_bounding_rect());
            if by != 0.0 {
                other.translate(egui::vec2(by, 0.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Color32, CornerRadius, FontId, Sense, vec2};

    use super::*;
    use crate::i18n::{self, Lang};

    fn outer() -> Rect {
        Rect::from_min_max(pos2(100.0, 10.0), pos2(300.0, 40.0))
    }

    fn arabic() -> Lang {
        Lang::from_code("ar").expect("Arabic is a registered language")
    }

    /// What a widget painting an icon square, a wrapped label and a picture, all from the left of
    /// `outer`, puts on screen in `lang`.
    fn painted(lang: Lang) -> Vec<Shape> {
        i18n::set_current(lang);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _mirror = mirror_scope(ui, outer());
            let corners = CornerRadius { nw: 6, ne: 0, sw: 6, se: 0 };
            ui.painter().rect_filled(Rect::from_min_size(outer().min + vec2(10.0, 0.0), vec2(20.0, 20.0)), corners, Color32::RED);
            let galley = ui.painter().layout("one two three four five".to_owned(), FontId::proportional(12.0), Color32::WHITE, 60.0);
            ui.painter().galley(outer().min + vec2(40.0, 0.0), galley, Color32::WHITE);
            let mut mesh = egui::Mesh::with_texture(egui::TextureId::User(7));
            mesh.add_rect_with_uv(
                Rect::from_min_size(outer().min + vec2(120.0, 0.0), vec2(30.0, 20.0)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.painter().add(Shape::mesh(mesh));
        });
        i18n::set_current(Lang::EN);
        let shapes = output.shapes.iter().map(|clipped| clipped.shape.clone()).filter(|shape| !matches!(shape, Shape::Noop)).collect();
        output.drop_without_applying_deltas();
        shapes
    }

    #[test]
    fn a_part_is_as_far_from_the_right_as_it_was_from_the_left() {
        let part = Rect::from_min_size(outer().min + vec2(10.0, 5.0), vec2(20.0, 10.0));
        let moved = mirrored(true, outer(), part);
        assert_eq!(moved, Rect::from_min_max(pos2(270.0, 15.0), pos2(290.0, 25.0)));
        assert_eq!(mirrored(true, outer(), moved), part, "mirroring twice puts it back");
        assert_eq!(mirrored(false, outer(), part), part);
        assert_eq!(mirrored_x(true, outer(), 100.0), 300.0);
        assert_eq!(mirrored_x(false, outer(), 100.0), 100.0);
    }

    #[test]
    fn a_left_to_right_language_paints_where_the_widget_said() {
        let shapes = painted(Lang::EN);
        let [Shape::Rect(square), Shape::Text(text), Shape::Mesh(picture)] = shapes.as_slice() else { panic!("{shapes:?}") };
        assert_eq!(square.rect.left(), 110.0);
        assert_eq!(text.pos.x, 140.0);
        assert_eq!(picture.calc_bounds().left(), 220.0);
    }

    #[test]
    fn a_right_to_left_language_mirrors_what_a_widget_painted() {
        let (ltr, rtl) = (painted(Lang::EN), painted(arabic()));
        let [Shape::Rect(square), Shape::Text(text), Shape::Mesh(picture)] = rtl.as_slice() else { panic!("{rtl:?}") };
        let [_, Shape::Text(ltr_text), Shape::Mesh(ltr_picture)] = ltr.as_slice() else { panic!("{ltr:?}") };

        // The square is as far from the right edge, with its rounded corners still outermost.
        assert_eq!(square.rect, Rect::from_min_size(pos2(270.0, 10.0), vec2(20.0, 20.0)));
        assert_eq!(square.corner_radius, CornerRadius { nw: 0, ne: 6, sw: 0, se: 6 });

        // The paragraph's block moved; its lines, which started together at the left, now end
        // together at the right. Each line is still drawn left to right.
        let block = text.galley.rect.translate(text.pos.to_vec2());
        assert!((block.right() - 260.0).abs() < 0.01, "{block:?}");
        assert!((block.width() - ltr_text.galley.rect.width()).abs() < 0.01);
        assert!(text.galley.rows.len() > 1, "the label wraps");
        for (row, ltr_row) in text.galley.rows.iter().zip(&ltr_text.galley.rows) {
            assert!((row.rect().right() - text.galley.rect.right()).abs() < 1.0, "{:?} in {:?}", row.rect(), text.galley.rect);
            assert!((ltr_row.rect().left() - ltr_text.galley.rect.left()).abs() < 1.0);
            assert_eq!(row.row.glyphs.len(), ltr_row.row.glyphs.len());
        }

        // The picture moved without being flipped: its left edge still shows the texture's left.
        let bounds = picture.calc_bounds();
        assert_eq!((bounds.left(), bounds.right()), (150.0, 180.0));
        for (vertex, ltr_vertex) in picture.vertices.iter().zip(&ltr_picture.vertices) {
            assert_eq!(vertex.uv, ltr_vertex.uv);
            assert_eq!(vertex.pos.x - bounds.left(), ltr_vertex.pos.x - ltr_picture.calc_bounds().left());
        }
    }

    #[test]
    fn odd_shapes_never_panic() {
        let nan = Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(f32::INFINITY, f32::NEG_INFINITY));
        let mut nested = Shape::Noop;
        for _ in 0..200 {
            nested = Shape::Vec(vec![nested, Shape::circle_filled(pos2(f32::NAN, 1.0), 2.0, Color32::RED)]);
        }
        let mut shapes = vec![
            nested,
            Shape::Vec(Vec::new()),
            Shape::rect_filled(nan, 3, Color32::RED),
            Shape::line(Vec::new(), egui::Stroke::new(1.0, Color32::RED)),
            Shape::mesh(egui::Mesh::default()),
            Shape::mesh(egui::Mesh::with_texture(egui::TextureId::User(1))),
        ];
        for outer in [outer(), nan, Rect::NOTHING, Rect::EVERYTHING] {
            for shape in &mut shapes {
                mirror_shape(shape, outer, 0);
            }
        }
    }

    #[test]
    fn rows_run_right_to_left_only_under_a_right_to_left_root() {
        for (lang, mirrored) in [(Lang::EN, false), (arabic(), true)] {
            i18n::set_current(lang);
            let ctx = egui::Context::default();
            let mut xs = (0.0, 0.0);
            let mut in_island = (0.0, 0.0);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                assert!(!flows(ui), "a panel starts left-aligned");
                root(ui, |ui| {
                    assert_eq!(flows(ui), mirrored);
                    ui.horizontal(|ui| {
                        let first = ui.allocate_exact_size(vec2(20.0, 10.0), Sense::hover()).0;
                        let second = ui.allocate_exact_size(vec2(20.0, 10.0), Sense::hover()).0;
                        xs = (first.center().x, second.center().x);
                        // The far end of a row is where reading ends.
                        ui.with_layout(far_side(ui, Align::Center), |ui| {
                            let last = ui.allocate_exact_size(vec2(20.0, 10.0), Sense::hover()).0;
                            assert_eq!(last.center().x < first.center().x, mirrored);
                        });
                    });
                    vertical(ui, |ui| assert_eq!(flows(ui), mirrored, "a column keeps the direction"));
                    // What egui lays out from the left only stays left to right inside.
                    island(ui, |ui| {
                        assert!(!flows(ui));
                        ui.horizontal(|ui| {
                            let first = ui.allocate_exact_size(vec2(20.0, 10.0), Sense::hover()).0;
                            let second = ui.allocate_exact_size(vec2(20.0, 10.0), Sense::hover()).0;
                            in_island = (first.center().x, second.center().x);
                        });
                    });
                    grid(ui, egui::Grid::new("g").num_columns(2), |ui| {
                        ui.label("a");
                        ui.label("b");
                        ui.end_row();
                    });
                });
            });
            output.drop_without_applying_deltas();
            assert_eq!(xs.0 > xs.1, mirrored, "{lang:?}: {xs:?}");
            assert!(in_island.0 < in_island.1, "{lang:?}: {in_island:?}");
        }
        i18n::set_current(Lang::EN);
    }

    #[test]
    fn panels_swap_sides_and_chevrons_turn() {
        i18n::set_current(Lang::EN);
        assert_eq!((back_chevron(), forward_chevron()), ("chevron-left", "chevron-right"));
        assert_eq!(area_layout(), Layout::default());
        i18n::set_current(arabic());
        assert_eq!((back_chevron(), forward_chevron()), ("chevron-right", "chevron-left"));
        assert!(area_layout().prefer_right_to_left());
        assert!(menu_layout().prefer_right_to_left());
        i18n::set_current(Lang::EN);
        assert!(!menu_layout().prefer_right_to_left());
    }
}
