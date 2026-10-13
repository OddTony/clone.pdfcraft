//! Edit a PDF ▸ change the vector paths a page paints ([`crate::paths`]): move and resize them
//! (the path is wrapped in `q … cm … Q`), recolour them (`q … rg … RG … Q`) or delete them.
//! `q` and colour operators may not appear inside a path object but may come right before it.
//! Every change is spliced into the page's content: only the streams it touches become new
//! objects, and every other byte stays as it was.

use std::collections::{BTreeMap, BTreeSet};

use pdfcraft_content::{Matrix, Op, Pieces};
use pdfcraft_cos::{Document, Object};

use crate::EditError;
use crate::objects::{MAX_MOVE_OBJECTS, checked_streams, valid_rect};
use crate::paths::page_paths;

/// Larger transform values come from runaway input.
const MAX_TRANSFORM: f64 = 1e12;

/// What to do with a vector path.
#[derive(Clone, Debug, PartialEq)]
pub enum PathChange {
    /// Apply a user-space transform after its placement (move and resize, see
    /// [`crate::rect_to_rect`]). Scaling scales its line width too. Refused for a path painted
    /// with a pattern ([`crate::PagePath::pattern`]), and for one it would hide.
    Transform([f64; 6]),
    /// Paint it in these DeviceRGB colours (components 0–1), whatever colour space it used;
    /// `None` keeps that part's colour.
    Recolour { fill: Option<[f64; 3]>, stroke: Option<[f64; 3]> },
}

enum Edit {
    Transform(Matrix),
    Recolour { fill: Option<[f64; 3]>, stroke: Option<[f64; 3]> },
    Delete,
}

/// Change path `index` (from [`page_paths`]) on `page`. Clips set in a `q … Q` that paints
/// nothing but the path move with it; clips shared with other content stay.
pub fn change_path(doc: &mut Document, page: usize, index: usize, change: &PathChange) -> Result<(), EditError> {
    match change {
        PathChange::Transform(t) => rewrite(doc, page, &[(index, Edit::Transform(Matrix(*t)))]),
        PathChange::Recolour { fill, stroke } => recolour_paths(doc, page, &[index], *fill, *stroke),
    }
}

/// Paint paths `indexes` (from [`page_paths`]) on `page` in DeviceRGB: `fill` where they fill,
/// `stroke` where they stroke. Refuses when a path paints neither part given.
pub fn recolour_paths(doc: &mut Document, page: usize, indexes: &[usize], fill: Option<[f64; 3]>, stroke: Option<[f64; 3]>) -> Result<(), EditError> {
    if fill.is_none() && stroke.is_none() {
        return Err(EditError::Invalid("choose a fill or stroke colour".into()));
    }
    if fill.iter().chain(&stroke).flatten().any(|v| !v.is_finite()) {
        return Err(EditError::Invalid("colour components must be numbers from 0 to 1".into()));
    }
    let clamp = |c: Option<[f64; 3]>| c.map(|c| c.map(|v| v.clamp(0.0, 1.0)));
    let (fill, stroke) = (clamp(fill), clamp(stroke));
    rewrite(doc, page, &indexes.iter().map(|&i| (i, Edit::Recolour { fill, stroke })).collect::<Vec<_>>())
}

/// Delete paths `indexes` (from [`page_paths`]) from `page`. Only their operators go: the
/// graphics state and marked content around them stay.
pub fn delete_paths(doc: &mut Document, page: usize, indexes: &[usize]) -> Result<(), EditError> {
    rewrite(doc, page, &indexes.iter().map(|&i| (i, Edit::Delete)).collect::<Vec<_>>())
}

/// Moves several paths, each by its own user-space offset, in one splice.
pub(crate) fn translate_paths(doc: &mut Document, page: usize, moves: &[(usize, [f64; 2])]) -> Result<(), EditError> {
    // Like the other kinds' group moves: a selection without paths has nothing to do here.
    if moves.is_empty() {
        return Ok(());
    }
    rewrite(doc, page, &moves.iter().map(|&(i, [x, y])| (i, Edit::Transform(Matrix::translate(x, y)))).collect::<Vec<_>>())
}

/// Plan every edit against one inventory, splice them into the page's content in one pass, and
/// replace `doc` only when the edited page still reads.
fn rewrite(doc: &mut Document, page: usize, edits: &[(usize, Edit)]) -> Result<(), EditError> {
    if edits.is_empty() || edits.len() > MAX_MOVE_OBJECTS {
        return Err(EditError::Invalid(format!("select between 1 and {MAX_MOVE_OBJECTS} vector paths")));
    }
    let paths = page_paths(doc, page)?;
    let p = pdfcraft_model::pages(doc).into_iter().nth(page).ok_or(EditError::NoSuchPage(page))?;
    let streams = checked_streams(doc, page)?;
    let marked: Vec<bool> = streams.iter().map(|(o, _)| crate::stream_tag(doc, o).is_some()).collect();
    let pieces = Pieces::join(&streams.iter().map(|(_, data)| data.as_slice()).collect::<Vec<_>>());
    let ops = pieces.parse().ops;
    let in_mark = |i: usize| ops.get(i).is_some_and(|op| marked.get(pieces.piece_of(op.span.start)) == Some(&true));
    // Operators to insert before an operator and to append after it, and the ones to remove.
    let mut before: BTreeMap<usize, Vec<Op>> = BTreeMap::new();
    let mut after: BTreeMap<usize, Vec<Op>> = BTreeMap::new();
    let mut removed = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for (index, edit) in edits {
        if !seen.insert(*index) {
            return Err(EditError::Invalid("a vector path was selected more than once".into()));
        }
        let path = paths.get(*index).ok_or_else(|| EditError::Invalid("a selected vector path no longer exists; select it again".into()))?;
        let (first, last) = (*path.ops.start(), *path.ops.end());
        let q = Op::new("q", Vec::new());
        match edit {
            Edit::Delete => removed.extend(path.ops.clone()),
            Edit::Recolour { fill, stroke } => {
                let painted_fill = fill.filter(|_| path.paint.fill.is_some());
                let painted_stroke = stroke.filter(|_| path.paint.stroke);
                if painted_fill.is_none() && painted_stroke.is_none() {
                    // Every listed path fills or strokes, so only one part was asked for.
                    let part = if fill.is_some() { "fill" } else { "stroke" };
                    return Err(EditError::Invalid(format!("a selected vector path has no {part} to recolour")));
                }
                let wrap = before.entry(first).or_default();
                wrap.push(q);
                wrap.extend(painted_fill.map(|c| Op::new("rg", c.map(Object::Real).to_vec())));
                wrap.extend(painted_stroke.map(|c| Op::new("RG", c.map(Object::Real).to_vec())));
                after.entry(last).or_default().push(Op::new("Q", Vec::new()));
            }
            Edit::Transform(t) => {
                if *t == Matrix::IDENTITY {
                    continue;
                }
                if path.pattern {
                    return Err(EditError::Invalid("a vector path painted with a pattern can't be moved or resized yet".into()));
                }
                if !t.0.iter().all(|v| v.is_finite() && v.abs() <= MAX_TRANSFORM) || t.invert().is_none() {
                    return Err(EditError::Invalid("the path transform must be finite, keep some area and be no larger than 1e12".into()));
                }
                if !valid_rect(t.bbox(path.rect)) {
                    return Err(EditError::Invalid("the vector path would move too far".into()));
                }
                // A private clip moves with its path, but PdfCraft's q/Q wrapper streams around the
                // page's content are left as they are: the path then moves under the clip.
                let (start, end, placement) =
                    crate::images::transform_scope(&ops, path.ops.clone(), Some(&in_mark))?.unwrap_or((first, last, path.matrix));
                let inverse =
                    placement.invert().ok_or_else(|| EditError::Invalid("this vector path is drawn flat, so it can't be moved or resized".into()))?;
                let local = placement.then(t).then(&inverse);
                if !local.0.iter().all(|v| v.is_finite() && v.abs() <= MAX_TRANSFORM) {
                    return Err(EditError::Invalid("the path transform is too large".into()));
                }
                // Full precision: a small move under a large CTM must not round away.
                before.entry(start).or_default().extend([q, Op::new("cm", local.0.map(Object::Real).to_vec())]);
                after.entry(end).or_default().push(Op::new("Q", Vec::new()));
            }
        }
    }
    if before.is_empty() && removed.is_empty() {
        return Ok(());
    }
    let data = crate::text::splice(&pieces, &ops, |i| {
        if removed.contains(&i) {
            return (Vec::new(), false);
        }
        let mut out = before.remove(&i).unwrap_or_default();
        match after.remove(&i) {
            None => (out, true),
            // Painting operators and `Q` have no operands, so they are written back as they were.
            Some(tail) => {
                out.extend(ops.get(i).cloned());
                out.extend(tail);
                (out, false)
            }
        }
    });
    let mut work = doc.clone();
    let contents = crate::text::rewritten_contents(&mut work, &streams, data);
    work.update_dict(p.obj, |d| d.set(b"Contents".to_vec(), contents))?;
    // The added operators count towards the page's budgets: keep its paths readable. A path
    // moved out of its clip or off the page would leave the list and shift the indexes after it.
    let deleted = edits.iter().filter(|(_, edit)| matches!(edit, Edit::Delete)).count();
    if page_paths(&work, page)?.len() != paths.len().saturating_sub(deleted) {
        return Err(EditError::Invalid("the vector path would be moved off the page or out of its clip".into()));
    }
    *doc = work;
    Ok(())
}

#[cfg(test)]
#[path = "path_edit_tests.rs"]
mod tests;
