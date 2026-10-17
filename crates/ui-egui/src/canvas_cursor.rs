//! Cursor guard for the PDF canvas.
//!
//! Overlays drawn above the canvas (the mode bar, tab strip, left quick toolbar, right rail,
//! palette, dialog, popup, …) set the cursor themselves when the pointer hovers a clickable.
//! The canvas also runs its hover detection every frame and calls `set_cursor_icon` based on the
//! pointer position. The last `set_cursor_icon` in the frame wins, so the canvas would otherwise
//! leak its cursor through the overlays: a text I-beam when the pointer is over a text box on the
//! page below the bar, a resize arrow on a box edge in Edit mode, and so on.
//!
//! [`set`] only writes the cursor when the pointer is over the canvas and no non-background
//! area sits on top. The visible-viewport check (`ui.clip_rect()`) catches the mode bar, tab
//! strip, side panels and toolbars; the layer check catches popups and dialogs.

use egui::{Context, CursorIcon, Order, Pos2, Rect};

/// Set the cursor icon only if the canvas owns the pointer (see module docs).
///
/// `canvas_rect` is the canvas's drawable area — `ui.clip_rect()` at the call site. Pass
/// `ui.clip_rect()` so the guard catches the mode bar and other panels without each call site
/// having to know the panel layout.
pub fn set(ui: &egui::Ui, icon: CursorIcon) {
    let ctx = ui.ctx();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    if !canvas_owns_pointer(ctx, ui.clip_rect(), pos) {
        return;
    }
    ctx.set_cursor_icon(icon);
}

/// Return `true` when the canvas owns the pointer: the pointer is inside `canvas_rect` and no
/// popup, dialog, window or other non-background area sits on top.
pub fn canvas_owns_pointer(ctx: &Context, canvas_rect: Rect, pos: Pos2) -> bool {
    if !canvas_rect.contains(pos) {
        return false;
    }
    // The mode bar, tab strip, side panels and central panel all live in the background layer.
    // A popup, dialog, window or tooltip lives in a higher layer; if it covers the pointer, the
    // overlay's cursor wins. `ctx.layer_id_at` is the topmost interactive area at `pos`.
    if let Some(layer) = ctx.layer_id_at(pos) {
        return layer.order == Order::Background;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::canvas_owns_pointer;
    use egui::{Context, Pos2, Rect, vec2};

    fn ctx() -> Context {
        Context::default()
    }

    #[test]
    fn pointer_inside_canvas_with_no_overlay_is_owned() {
        let ctx = ctx();
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        let pos = Pos2::new(500.0, 400.0);
        assert!(canvas_owns_pointer(&ctx, canvas, pos));
    }

    #[test]
    fn pointer_outside_canvas_is_not_owned() {
        let ctx = ctx();
        let canvas = Rect::from_min_size(Pos2::new(0.0, 50.0), vec2(1000.0, 750.0));
        // 10 px below the top of the window — inside the (above) mode bar's band.
        let pos = Pos2::new(500.0, 10.0);
        assert!(!canvas_owns_pointer(&ctx, canvas, pos));
    }

    #[test]
    fn pointer_on_canvas_edge_is_owned() {
        let ctx = ctx();
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        assert!(canvas_owns_pointer(&ctx, canvas, Pos2::new(0.0, 0.0)));
        assert!(canvas_owns_pointer(&ctx, canvas, Pos2::new(1000.0, 800.0)));
    }

    #[test]
    fn pointer_just_outside_canvas_is_not_owned() {
        let ctx = ctx();
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        assert!(!canvas_owns_pointer(&ctx, canvas, Pos2::new(-1.0, 400.0)));
        assert!(!canvas_owns_pointer(&ctx, canvas, Pos2::new(1000.5, 400.0)));
    }

    /// A background layer over the pointer does not block the canvas: `layer_id_at` returns
    /// `None` for a fresh `Context`, which the guard treats as "no overlay".
    #[test]
    fn fresh_context_has_no_layer_at_pointer() {
        let ctx = ctx();
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        let pos = Pos2::new(500.0, 400.0);
        assert!(ctx.layer_id_at(pos).is_none());
        assert!(canvas_owns_pointer(&ctx, canvas, pos));
    }
}
