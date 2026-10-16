//! Visible page items shared by keyboard navigation and headless discovery.
use crate::Document;

/// A stable address within the current document generation. Indices are zero-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageItem {
    Field { name: String, widget: usize },
    Annotation { index: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageTarget {
    pub item: PageItem,
    /// Normalized rectangle in PDF user space.
    pub rect: [f64; 4],
    pub label: String,
    pub editable: bool,
}

impl Document {
    /// Visible widgets in the PDF's field tab order, followed by top-level comments in
    /// annotation-array order. Focus never changes a value. Replies and hidden items are skipped.
    /// Links and other non-comment annotations are not yet part of this traversal.
    pub fn page_focus_targets(&self, page: usize) -> Vec<PageTarget> {
        if self.info.pages.get(page).is_none() {
            return Vec::new();
        }
        let mut fields: Vec<_> = self
            .form
            .iter()
            .flat_map(|f| {
                f.widgets.iter().enumerate().filter(move |(_, w)| w.page == Some(page) && !w.hidden).map(move |(widget, w)| {
                    (
                        w.tab,
                        PageTarget {
                            item: PageItem::Field { name: f.name.clone(), widget },
                            rect: w.rect,
                            label: f.tooltip.clone().unwrap_or_else(|| f.name.clone()),
                            editable: !f.read_only() && self.allows_form_filling(),
                        },
                    )
                })
            })
            .collect();
        fields.sort_by_key(|(tab, _)| *tab);
        let mut targets: Vec<_> = fields.into_iter().map(|(_, t)| t).collect();
        if !self.comments_hidden() {
            let raw = self.editor.as_ref().and_then(|e| {
                let pages = pdfcraft_annot::page_refs(&e.cos).ok()?;
                let p = pages.get(page)?;
                let list = e.cos.get(*p).as_dict()?.get(b"Annots")?.clone();
                Some(e.cos.resolve(&list))
            });
            for a in self.info.annotations.iter().filter(|a| a.page == page && a.in_reply_to.is_none()) {
                let hidden = self.editor.as_ref().zip(raw.as_ref()).is_some_and(|(e, raw)| {
                    raw.as_array().and_then(|list| list.get(a.index)).is_some_and(|entry| {
                        e.cos
                            .resolve(entry)
                            .as_dict()
                            .and_then(|d| d.get(b"F"))
                            .and_then(|f| e.cos.resolve(f).as_int())
                            .is_some_and(|flags| flags & (1 | 2 | 32) != 0)
                    })
                });
                if !hidden && a.rect.iter().all(|v| v.is_finite()) {
                    targets.push(PageTarget {
                        item: PageItem::Annotation { index: a.index },
                        rect: a.rect.map(f64::from),
                        label: a.contents.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| a.subtype.clone()),
                        // Locked comments still allow editing their text, as on the pointer path.
                        editable: self.allows_annotation(),
                    });
                }
            }
        }
        // Keep coordinates bounded through conversion to frontend f32 values and zoom transforms.
        targets.retain(|t| t.rect.iter().all(|v| v.is_finite() && v.abs() <= 1.0e9) && t.rect[0] <= t.rect[2] && t.rect[1] <= t.rect[3]);
        targets
    }
}

/// Advance through one page, wrapping in either direction. Stale selections restart at an end.
pub fn next_target<'a>(targets: &'a [PageTarget], current: Option<&PageItem>, forward: bool) -> Option<&'a PageTarget> {
    let position = current.and_then(|item| targets.iter().position(|t| &t.item == item));
    let next = match position {
        None if forward => 0,
        None => targets.len().checked_sub(1)?,
        Some(i) if forward => {
            if i.saturating_add(1) < targets.len() {
                i + 1
            } else {
                0
            }
        }
        Some(0) => targets.len().checked_sub(1)?,
        Some(i) => i - 1,
    };
    targets.get(next)
}
