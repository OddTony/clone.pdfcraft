//! A document-tab drag is a placement intent, never a document close/open.
//! Ordering is committed only after release inside the visible document strip.

use egui::{Pos2, Rect};
use pdfcraft_engine::DocId;

#[derive(Clone, Debug)]
pub(crate) struct TabDrag {
    pub source: DocId,
    pub target: Option<usize>,
}

impl TabDrag {
    /// Destination is the final index after removing the source, not an insertion
    /// index in the original list. Hidden tabs still count toward that index.
    pub fn update(&mut self, pointer: Option<Pos2>, region: Rect, tabs: &[(DocId, Rect)]) {
        self.target = pointer.filter(|p| region.contains(*p)).and_then(|p| {
            // Closing/replacing the source during a drag invalidates the intent.
            tabs.iter().position(|(id, _)| *id == self.source)?;
            Some(tabs.iter().filter(|(id, rect)| *id != self.source && p.x >= rect.center().x).count())
        });
    }

    pub fn finish(self, canceled: bool) -> Option<(DocId, usize)> {
        (!canceled).then_some(self.target).flatten().map(|target| (self.source, target))
    }
}

/// Bounded edge scrolling, in logical points per second. No animation is
/// required for feedback, including when reduced motion is enabled.
pub(crate) fn edge_scroll(pointer: Option<Pos2>, region: Rect, offset: f32, max: f32, dt: f32) -> Option<f32> {
    let p = pointer.filter(|p| region.contains(*p))?;
    let edge = 24.0_f32.min(region.width() / 3.0);
    if edge <= 0.0 {
        return None;
    }
    let speed = if p.x < region.left() + edge {
        -((region.left() + edge - p.x) / edge)
    } else if p.x > region.right() - edge {
        (p.x - (region.right() - edge)) / edge
    } else {
        return None;
    };
    let next = (offset + speed * 420.0 * dt.clamp(0.0, 0.05)).clamp(0.0, max.max(0.0));
    (next != offset).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn tabs() -> Vec<(DocId, Rect)> {
        (0..4).map(|i| (DocId(i), Rect::from_min_size(pos2(i as f32 * 100.0, 0.0), vec2(100.0, 30.0)))).collect()
    }

    #[test]
    fn insertion_ignores_source_and_commits_only_valid_release() {
        let tabs = tabs();
        let region = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 30.0));
        let mut drag = TabDrag { source: tabs[0].0, target: None };
        drag.update(Some(pos2(399.0, 15.0)), region, &tabs);
        assert_eq!(drag.clone().finish(false), Some((tabs[0].0, 3)));
        assert_eq!(drag.clone().finish(true), None);
        drag.update(Some(pos2(399.0, 40.0)), region, &tabs);
        assert_eq!(drag.clone().finish(false), None);
        drag.update(None, region, &tabs);
        assert_eq!(drag.finish(false), None);
    }

    #[test]
    fn scrolled_geometry_and_closed_source_are_respected() {
        let mut tabs = tabs();
        for (_, rect) in &mut tabs {
            *rect = rect.translate(vec2(-150.0, 0.0));
        }
        let region = Rect::from_min_max(pos2(0.0, 0.0), pos2(250.0, 30.0));
        let mut drag = TabDrag { source: tabs[3].0, target: None };
        drag.update(Some(pos2(1.0, 15.0)), region, &tabs);
        assert_eq!(drag.target, Some(2));
        tabs.pop();
        drag.update(Some(pos2(1.0, 15.0)), region, &tabs);
        assert_eq!(drag.finish(false), None);
    }

    #[test]
    fn overflow_scroll_is_bounded_and_stops_outside_strip() {
        let region = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 30.0));
        assert!(edge_scroll(Some(pos2(199.0, 15.0)), region, 50.0, 100.0, 0.016).unwrap() > 50.0);
        assert_eq!(edge_scroll(Some(pos2(199.0, 15.0)), region, 100.0, 100.0, 0.016), None);
        assert_eq!(edge_scroll(Some(pos2(199.0, 40.0)), region, 50.0, 100.0, 0.016), None);
        assert_eq!(edge_scroll(Some(pos2(100.0, 15.0)), region, 50.0, 100.0, 0.016), None);
        assert_eq!(edge_scroll(Some(pos2(1.0, 15.0)), region, 0.0, 100.0, 1.0), None);
    }
}
