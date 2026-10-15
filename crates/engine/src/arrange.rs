//! Arrange selected objects: align and distribute (Element Studio).
//!
//! Geometry comes from the object inventory; the moves themselves reuse the
//! proven translation path, one [`Edit::MoveObjects`] per moved object inside
//! a single [`Edit::Batch`]: one undo step, atomic (a refusal changes
//! nothing). Sampling is bounded by the inventory caps.

use crate::{DocId, Edit, EditError, Session};

pub use pdfcraft_edit::{AlignOp, ObjectTarget};

/// What `Session::align_objects` moved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlignReport {
    /// Objects actually moved (already-placed ones are left out).
    pub moved: usize,
    /// Objects left in place.
    pub placed: usize,
    /// Per-target user-space offsets that were applied, for remapping selections.
    pub deltas: Vec<(ObjectTarget, [f64; 2])>,
}

impl Session {
    /// Align or distribute open-document objects on one page from a
    /// [`pdfcraft_edit::AlignOp`]: `targets` are inventory references (see
    /// `object_list`), `page` is 0-based, and `to_page` aligns within the
    /// page box instead of the selection box.
    pub fn align_objects(
        &mut self,
        id: DocId,
        page: usize,
        targets: Vec<ObjectTarget>,
        op: AlignOp,
        to_page: bool,
    ) -> Result<AlignReport, EditError> {
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let info = doc.info.pages.get(page).ok_or_else(|| EditError::Invalid("the page no longer exists".into()))?;
        let objects = doc.editable_objects(page).map_err(EditError::Invalid)?;
        let scope = if to_page {
            let [x0, y0, x1, y1] = info.crop.map(f64::from);
            [x0, y0, x1, y1]
        } else {
            // The selection box; align_offsets validates it through the inventory.
            let inventory: std::collections::BTreeMap<_, _> = objects.iter().map(|o| (o.target, o)).collect();
            let mut los = [f64::INFINITY; 4];
            let mut his = [f64::NEG_INFINITY; 4];
            for t in &targets {
                let o = inventory.get(t).ok_or_else(|| EditError::Invalid("a selected object no longer exists; select the objects again".into()))?;
                for i in 0..4 {
                    los[i] = los[i].min(o.rect[i]);
                    his[i] = his[i].max(o.rect[i]);
                }
            }
            if targets.is_empty() {
                return Err(EditError::Invalid("select between 1 and 1000 objects to align".into()));
            }
            [los[0], los[1], his[2], his[3]]
        };
        let moves = pdfcraft_edit::align_offsets(&objects, &targets, op, scope)?;
        if moves.is_empty() {
            return Err(EditError::Invalid("the objects are already aligned".into()));
        }
        let moved = moves.len();
        let edits = moves.iter().map(|(target, offset)| Edit::MoveObjects { page, objects: vec![*target], offset: *offset }).collect::<Vec<_>>();
        let label = if op.is_distribute() { "Distribute objects" } else { "Align objects" };
        self.apply(id, Edit::Batch { label: label.into(), edits })?;
        Ok(AlignReport { moved, placed: targets.len().saturating_sub(moved), deltas: moves })
    }
}
