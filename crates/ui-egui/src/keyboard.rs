//! Region focus and page-item traversal. Text editing and document edits keep their own paths.
use egui::{Id, Rect};
use pdfcraft_engine::keyboard::{PageItem, next_target};

use crate::{
    PdfCraftApp, QuickTool,
    comments::{Composer, ComposerKind},
};

pub(crate) fn document_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.has_focus(Region::Document.id()))
}

pub(crate) fn paint_page_item(
    ui: &egui::Ui,
    painter: &egui::Painter,
    xf: &crate::canvas::PageXform,
    page: usize,
    info: &pdfcraft_render::DocInfo,
    form: &[pdfcraft_engine::FormField],
    view: &crate::DocView,
) {
    let Some((p, item)) = &view.keyboard_item else { return };
    if *p != page || (!document_focused(ui.ctx()) && view.forms.focus.is_none()) {
        return;
    }
    let rect = match item {
        PageItem::Field { name, widget } => {
            form.iter().find(|f| &f.name == name).and_then(|f| f.widgets.get(*widget)).map(|w| w.rect.map(|v| v as f32))
        }
        PageItem::Annotation { index } => info.annotations.iter().find(|a| a.page == page && a.index == *index).map(|a| a.rect),
    };
    if let Some(rect) = rect.filter(|r| r.iter().all(|v| v.is_finite())) {
        painter.rect_stroke(
            xf.user_rect(info, page, rect).expand(2.0),
            2.0,
            egui::Stroke::new(2.0, crate::theme::Tokens::get(ui.ctx()).accent),
            egui::StrokeKind::Outside,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Region {
    Document,
    Toolbar,
    Tools,
    Inspector,
}

impl Region {
    fn label(self) -> &'static str {
        match self {
            Self::Document => "Document",
            Self::Toolbar => "Toolbar",
            Self::Tools => "Tools",
            Self::Inspector => "Side panel",
        }
    }
    fn id(self) -> Id {
        Id::new(("keyboard-region", self))
    }
}

struct Entry {
    region: Region,
    rect: Rect,
    layer: egui::LayerId,
}

#[derive(Default)]
pub(crate) struct Navigation {
    regions: Vec<Entry>,
    pending_focus: Option<Id>,
}

impl Navigation {
    /// egui schedules reverse Tab focus for the following pass. Reassert the requested focus target
    /// after that transfer, so a departed text editor cannot hand focus to a scroll area.
    pub(crate) fn apply_pending_focus(&mut self, ctx: &egui::Context, blocked: bool) {
        if let Some(id) = self.pending_focus.take()
            && !blocked
        {
            ctx.memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(id);
            });
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.regions.clear();
    }

    pub(crate) fn region(&mut self, ui: &egui::Ui, region: Region, label: Option<&str>) {
        let rect = ui.max_rect().intersect(ui.clip_rect());
        if !rect.is_finite() || !rect.is_positive() {
            return;
        }
        let response = ui.interact(rect, region.id(), egui::Sense::focusable_noninteractive());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), crate::i18n::t(label.unwrap_or(region.label()))));
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(match region {
                Region::Document => egui::accesskit::Role::Document,
                Region::Toolbar => egui::accesskit::Role::Toolbar,
                Region::Tools | Region::Inspector => egui::accesskit::Role::Group,
            })
        });
        if response.has_focus() && region == Region::Document {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(response.id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true })
            });
        }
        self.regions.push(Entry { region, rect, layer: ui.layer_id() });
    }

    pub(crate) fn document_focused(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(Region::Document.id()))
    }

    pub(crate) fn cycle(&mut self, ctx: &egui::Context, forward: bool) {
        self.pending_focus = None;
        let focused = ctx.memory(|m| m.focused());
        let current = focused
            .and_then(|id| self.regions.iter().find(|r| r.region.id() == id).map(|r| r.region))
            .or_else(|| {
                focused
                    .and_then(|id| ctx.read_response(id))
                    .and_then(|response| self.regions.iter().find(|r| r.rect.contains(response.rect.center())).map(|r| r.region))
            })
            .unwrap_or(Region::Document);
        let mut regions: Vec<_> = self.regions.iter().map(|r| r.region).collect();
        regions.sort_unstable();
        regions.dedup();
        let target = if forward {
            regions.iter().copied().find(|r| *r > current).or_else(|| regions.first().copied())
        } else {
            regions.iter().rev().copied().find(|r| *r < current).or_else(|| regions.last().copied())
        };
        if let Some(target) = target {
            egui::Popup::close_all(ctx);
            ctx.memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(target.id());
            });
        }
    }

    pub(crate) fn paint(&self, ctx: &egui::Context) {
        for entry in &self.regions {
            if ctx.memory(|m| m.has_focus(entry.region.id())) {
                ctx.layer_painter(entry.layer).rect_stroke(
                    entry.rect.shrink(2.0),
                    2.0,
                    egui::Stroke::new(2.0, crate::theme::Tokens::get(ctx).accent),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

impl PdfCraftApp {
    pub(crate) fn focus_region(&mut self, forward: bool) {
        if self.modal_open() || self.palette_open || !self.commit_form_typing() {
            return;
        }
        if let Some(view) = self.active.and_then(|i| self.views.get_mut(i)) {
            view.forms.focus = None;
        }
        if let Some(ctx) = self.ctx.clone() {
            self.keyboard.cycle(&ctx, forward);
        }
    }

    pub(crate) fn step_page_item(&mut self, forward: bool) -> bool {
        if let Some(view) = self.active.and_then(|i| self.views.get_mut(i))
            && let Some(f) = &view.forms.focus
        {
            let page = self
                .session
                .get(view.id)
                .and_then(|d| d.form.iter().find(|field| field.name == f.name))
                .and_then(|field| field.widgets.get(f.widget))
                .and_then(|w| w.page);
            if let Some(page) = page {
                view.keyboard_item = Some((page, PageItem::Field { name: f.name.clone(), widget: f.widget }));
            }
        }
        if self.modal_open() || self.palette_open || self.is_preparing() || !self.commit_form_typing() {
            return false;
        }
        let Some((i, id)) = self.active_ids() else { return false };
        let Some(doc) = self.session.get(id) else { return false };
        let Some(view) = self.views.get_mut(i).filter(|v| !v.organize) else { return false };
        let page = view.current;
        let targets = doc.page_focus_targets(page);
        let current = view
            .forms
            .focus
            .as_ref()
            .map(|f| PageItem::Field { name: f.name.clone(), widget: f.widget })
            .or_else(|| view.keyboard_item.as_ref().filter(|(p, _)| *p == page).map(|(_, item)| item.clone()))
            .or_else(|| view.comments.selected.filter(|(p, _)| *p == page).map(|(_, index)| PageItem::Annotation { index }));
        let Some(target) = next_target(&targets, current.as_ref(), forward) else { return false };
        if view.page_xform(page).is_some_and(|xf| !view.viewport_rect().contains_rect(xf.user_rect(&doc.info, page, target.rect.map(|v| v as f32)))) {
            view.go_to_dest(
                page,
                pdfcraft_render::DestView::Xyz { left: Some(target.rect[0] as f32), top: Some(target.rect[3] as f32), zoom: None },
                &doc.info,
            );
        }
        view.forms.focus = None;
        view.comments.selected = None;
        view.keyboard_item = Some((page, target.item.clone()));
        self.quick_tool = QuickTool::Select;
        match &target.item {
            PageItem::Field { name, widget } => {
                if let Some(field) = doc.form.iter().find(|f| &f.name == name)
                    && target.editable
                    && matches!(
                        field.kind,
                        pdfcraft_engine::FormFieldKind::Text | pdfcraft_engine::FormFieldKind::Combo | pdfcraft_engine::FormFieldKind::List
                    )
                {
                    view.forms.focus = Some(crate::forms_ui::keyboard_focus(field, *widget));
                }
            }
            PageItem::Annotation { index } => {
                view.comments.selected = Some((page, *index));
                view.comments.reveal = true;
            }
        }
        if let Some(ctx) = self.ctx.clone() {
            ctx.memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.request_focus(Region::Document.id());
            });
            self.keyboard.pending_focus = Some(
                match view
                    .forms
                    .focus
                    .as_ref()
                    .filter(|f| doc.form.iter().any(|field| field.name == f.name && field.kind == pdfcraft_engine::FormFieldKind::Text))
                {
                    Some(f) => Id::new(("form-field", view.id.0, &f.name, f.widget)),
                    None => Region::Document.id(),
                },
            );
        }
        true
    }

    /// After the form overlay has seen typing and validation has accepted its queued edit.
    pub(crate) fn finish_page_tab(&mut self, accepted: bool) {
        let request = self.active.and_then(|i| self.views.get_mut(i)).and_then(|v| v.forms.tab_request.take());
        if accepted && let Some(forward) = request {
            self.step_page_item(forward);
        }
    }

    pub(crate) fn page_item_keys(&mut self, ctx: &egui::Context) {
        if self.modal_open() || self.palette_open || self.is_preparing() || !self.keyboard.document_focused(ctx) || ctx.text_edit_focused() {
            return;
        }
        let Some((i, id)) = self.active_ids() else { return };
        if self.views.get(i).is_none_or(|v| v.organize || v.forms.focus.is_some()) {
            return;
        }
        let tab = ctx.input_mut(|input| {
            let mut tab = None;
            input.events.retain(|e| {
                if let egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. } = e
                    && (modifiers.is_none() || modifiers.shift_only())
                {
                    tab = Some(!modifiers.shift);
                    return false;
                }
                true
            });
            tab
        });
        if let Some(forward) = tab {
            self.step_page_item(forward);
            return;
        }
        if self.views.get(i).is_some_and(|v| v.keyboard_item.is_some())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            if let Some(view) = self.views.get_mut(i) {
                view.keyboard_item = None;
                view.comments.selected = None;
            }
            return;
        }
        if self.views.get(i).is_none_or(|v| v.keyboard_item.is_none()) {
            return;
        }
        let activate = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter) || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
        if !activate {
            return;
        }
        let Some(doc) = self.session.get(id) else { return };
        let Some(view) = self.views.get_mut(i) else { return };
        let Some((page, item)) = view.keyboard_item.clone() else { return };
        // Re-resolve against the current generation: an edit may have removed this item.
        let Some(target) = doc.page_focus_targets(page).into_iter().find(|t| t.item == item) else {
            view.keyboard_item = None;
            return;
        };
        match item {
            PageItem::Field { name, widget } => {
                if let Some(f) = doc.form.iter().find(|f| f.name == name) {
                    crate::forms_ui::activate_widget(view, f, widget, doc.allows_form_filling());
                }
            }
            PageItem::Annotation { index } if target.editable => {
                let text =
                    doc.info.annotations.iter().find(|a| a.page == page && a.index == index).and_then(|a| a.contents.clone()).unwrap_or_default();
                view.comments.composer =
                    Some(Composer { page, at: [target.rect[2], target.rect[3]], kind: ComposerKind::Edit(index), text, focus: true });
            }
            _ => {}
        }
    }
}
