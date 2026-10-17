//! Document-local bookmark interaction state. All destinations retain PDF outline paths.
use pdfcraft_engine::Edit;
use pdfcraft_render::OutlineItem;

#[derive(Default)]
pub struct BookmarkState {
    pub selected: Option<Vec<usize>>,
    pub rename: Option<(Vec<usize>, String)>,
    pub(crate) drag: Option<Drag>,
    pub(crate) expanded: std::collections::HashMap<Vec<usize>, bool>,
    generation: Option<u64>,
}

pub(crate) struct Drag {
    pub source: Vec<usize>,
    pub hover: Option<(Vec<usize>, f64)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Placement {
    Before,
    After,
    Inside,
}

pub(crate) fn at<'a>(mut items: &'a [OutlineItem], path: &[usize]) -> Option<&'a OutlineItem> {
    let (last, parents) = path.split_last()?;
    for &i in parents {
        items = &items.get(i)?.children;
    }
    items.get(*last)
}

fn siblings<'a>(items: &'a [OutlineItem], parent: &[usize]) -> Option<&'a [OutlineItem]> {
    if parent.is_empty() { Some(items) } else { Some(&at(items, parent)?.children) }
}

/// Remove a path's source sibling, adjusting any destination parent that runs through a
/// later sibling. The engine performs this adjustment itself; callers pass the ORIGINAL
/// parent to MoveBookmark and use this only for the resulting selection path.
fn after_removal(path: &[usize], source: &[usize]) -> Option<Vec<usize>> {
    if path.starts_with(source) {
        return None;
    }
    let (&index, parent) = source.split_last()?;
    let mut path = path.to_vec();
    if path.starts_with(parent)
        && let Some(i) = path.get_mut(parent.len())
        && *i > index
    {
        *i -= 1;
    }
    Some(path)
}

fn after_insertion(mut path: Vec<usize>, parent: &[usize], index: usize) -> Vec<usize> {
    if path.starts_with(parent)
        && let Some(i) = path.get_mut(parent.len())
        && *i >= index
    {
        *i = i.saturating_add(1);
    }
    path
}

/// Translate a row's pre-removal gap to the engine's post-removal index. Reject self,
/// descendants, invalid paths and the two gaps that would leave the source unchanged.
pub(crate) fn drop_edit(items: &[OutlineItem], source: &[usize], target: &[usize], placement: Placement) -> Option<Edit> {
    at(items, source)?;
    let target_item = at(items, target)?;
    if target.starts_with(source) {
        return None;
    }
    let (&target_index, target_parent) = target.split_last()?;
    let (parent, gap) = match placement {
        Placement::Before => (target_parent.to_vec(), target_index),
        Placement::After => (target_parent.to_vec(), target_index.checked_add(1)?),
        Placement::Inside => (target.to_vec(), target_item.children.len()),
    };
    let (&source_index, source_parent) = source.split_last()?;
    let index = if parent == source_parent && source_index < gap { gap.saturating_sub(1) } else { gap };
    if parent == source_parent && index == source_index {
        return None;
    }
    Some(Edit::MoveBookmark { from: source.to_vec(), to_parent: parent, index })
}

impl BookmarkState {
    pub(crate) fn prepared(&self, edit: &Edit, items: &[OutlineItem]) -> Self {
        let mut next = Self { selected: self.selected.clone(), expanded: self.expanded.clone(), ..Default::default() };
        next.committed(edit, items, self.generation.unwrap_or_default());
        next
    }

    pub(crate) fn accept_generation(&mut self, generation: u64) {
        self.generation = Some(generation);
    }
    pub(crate) fn sync(&mut self, generation: u64, items: &[OutlineItem]) {
        if self.generation.is_some_and(|old| old != generation) {
            // An edit outside this UI, reload or undo may reuse a valid path for a different
            // bookmark. Never infer identity from a title (titles may be duplicated).
            self.selected = None;
            self.rename = None;
            self.drag = None;
            self.expanded.clear();
        }
        self.generation = Some(generation);
        if self.selected.as_ref().is_some_and(|p| at(items, p).is_none()) {
            self.selected = None;
        }
        if self.rename.as_ref().is_some_and(|(p, _)| at(items, p).is_none()) {
            self.rename = None;
        }
    }

    pub(crate) fn insertion(&self, items: &[OutlineItem]) -> (Vec<usize>, usize) {
        if let Some(path) = &self.selected
            && at(items, path).is_some()
            && let Some((&i, parent)) = path.split_last()
        {
            return (parent.to_vec(), i.saturating_add(1));
        }
        (vec![], items.len())
    }

    /// Prepare remapped state using the outline BEFORE the edit for lengths. The caller
    /// installs this state and its new generation only if the engine edit succeeds.
    pub(crate) fn committed(&mut self, edit: &Edit, items: &[OutlineItem], generation: u64) {
        self.drag = None;
        self.rename = None;
        self.generation = Some(generation);
        self.selected = self.selected.take().and_then(|path| remap(path, edit, items));
        self.expanded = std::mem::take(&mut self.expanded).into_iter().filter_map(|(path, open)| Some((remap(path, edit, items)?, open))).collect();
    }
}

fn remap(path: Vec<usize>, edit: &Edit, items: &[OutlineItem]) -> Option<Vec<usize>> {
    match edit {
        Edit::AddBookmark { parent, index, .. } => {
            let index = (*index).min(siblings(items, parent)?.len());
            Some(after_insertion(path, parent, index))
        }
        Edit::DeleteBookmark { path: source } => after_removal(&path, source),
        Edit::MoveBookmark { from, to_parent, index } => {
            let (_, source_parent) = from.split_last()?;
            let n = siblings(items, to_parent)?.len().saturating_sub(usize::from(source_parent == to_parent));
            let index = (*index).min(n);
            let parent = after_removal(to_parent, from)?;
            if let Some(tail) = path.strip_prefix(from.as_slice()) {
                let mut moved = parent;
                moved.push(index);
                moved.extend_from_slice(tail);
                Some(moved)
            } else {
                Some(after_insertion(after_removal(&path, from)?, &parent, index))
            }
        }
        Edit::RenameBookmark { .. } | Edit::SetBookmarkPage { .. } => Some(path),
        // Other operations can replace the outline (including page imports). Clearing is
        // safer than pointing an insertion reference at an unrelated bookmark.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, children: Vec<OutlineItem>) -> OutlineItem {
        OutlineItem { title: title.into(), page: Some(0), view: pdfcraft_render::DestView::Top, children, open: true }
    }

    #[test]
    fn every_root_gap_matches_an_independent_remove_insert_model() {
        let items: Vec<_> = (0..8).map(|i| item(&i.to_string(), vec![])).collect();
        for source in 0..items.len() {
            for target in 0..items.len() {
                for placement in [Placement::Before, Placement::After] {
                    let mut expected: Vec<_> = (0..items.len()).collect();
                    // Find the target by identity AFTER removing the source, rather than using
                    // the production gap adjustment. A self target is always rejected.
                    expected.remove(source);
                    let edit = drop_edit(&items, &[source], &[target], placement);
                    if source == target {
                        assert!(edit.is_none());
                        continue;
                    }
                    let position = expected.iter().position(|i| *i == target).unwrap() + usize::from(matches!(placement, Placement::After));
                    expected.insert(position, source);
                    if expected == (0..items.len()).collect::<Vec<_>>() {
                        assert!(edit.is_none());
                    } else {
                        let edit = edit.unwrap();
                        assert!(
                            matches!(&edit, Edit::MoveBookmark { from, to_parent, index } if from == &[source] && to_parent.is_empty() && *index == position)
                        );
                        // Every selection, including later siblings, follows its identity.
                        for selected in 0..items.len() {
                            let mut state = BookmarkState { selected: Some(vec![selected]), ..Default::default() };
                            state.committed(&edit, &items, 1);
                            assert_eq!(state.selected, Some(vec![expected.iter().position(|i| *i == selected).unwrap()]));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn related_subtree_moves_remap_selected_descendants_and_unrelated_siblings() {
        let items =
            vec![item("A", vec![item("A0", vec![item("A00", vec![])]), item("A1", vec![])]), item("B", vec![item("B0", vec![])]), item("C", vec![])];
        for (from, parent, index, selected, expected) in [
            (vec![0], vec![1, 0], 0, vec![0, 0, 0], vec![0, 0, 0, 0, 0]),
            (vec![0], vec![1, 0], 0, vec![1, 0], vec![0, 0]),
            (vec![0, 0], vec![], 1, vec![0, 0, 0], vec![1, 0]),
            (vec![0, 0], vec![], 1, vec![1, 0], vec![2, 0]),
            (vec![1, 0], vec![0], 1, vec![0, 1], vec![0, 2]),
            (vec![1, 0], vec![0], 1, vec![1], vec![1]),
        ] {
            let mut state = BookmarkState { selected: Some(selected), ..Default::default() };
            state.committed(&Edit::MoveBookmark { from, to_parent: parent, index }, &items, 1);
            assert_eq!(state.selected, Some(expected));
        }
        for (source, target) in [(vec![], vec![0]), (vec![9], vec![0]), (vec![0], vec![9]), (vec![0], vec![0, 0, 0])] {
            assert!(drop_edit(&items, &source, &target, Placement::Inside).is_none());
        }
    }

    #[test]
    fn stale_selection_rename_and_drag_cannot_refer_to_a_reused_path() {
        let items = vec![item("Duplicate", vec![]), item("Duplicate", vec![])];
        let mut state = BookmarkState {
            selected: Some(vec![0]),
            rename: Some((vec![0], "Duplicate".into())),
            drag: Some(Drag { source: vec![0], hover: None }),
            generation: Some(4),
            expanded: Default::default(),
        };
        state.sync(5, &items);
        assert!(state.selected.is_none() && state.rename.is_none() && state.drag.is_none());
        assert_eq!(state.insertion(&items), (vec![], 2));
        state.selected = Some(vec![42]);
        assert_eq!(state.insertion(&items), (vec![], 2));
    }
}
