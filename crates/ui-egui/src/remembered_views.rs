//! Local reading history, independent of the last open-tab session. Never writes to a PDF.
use crate::{
    DocView, PdfCraftApp,
    canvas::{Fit, PageLayout},
};
use serde::{Deserialize, Serialize};

const MAX_VIEWS: usize = 5_000;
// Accommodates native Unicode paths while bounding copies from untrusted settings.
const MAX_PATH_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RememberedView {
    path: String,
    page: usize,
    zoom: f32,
    fit: Fit,
    layout: PageLayout,
    continuous: bool,
    cover: bool,
}

#[derive(Clone, Default, Serialize)]
#[serde(transparent)]
pub(crate) struct RememberedViews(Vec<RememberedView>);

fn path_key(path: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::last_session::absolute(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        path.to_string()
    }
}

impl RememberedViews {
    pub(crate) fn from_json(value: &serde_json::Value) -> Self {
        let mut history = Self::default();
        let mut seen = std::collections::HashSet::new();
        if let Some(entries) = value.as_array() {
            for entry in entries.iter().take(MAX_VIEWS) {
                let Some(path) = entry.get("path").and_then(serde_json::Value::as_str) else { continue };
                if path.is_empty() || path.len() > MAX_PATH_BYTES {
                    continue;
                }
                if let Ok(mut view) = RememberedView::deserialize(entry) {
                    if !view.zoom.is_finite() {
                        continue;
                    }
                    view.path = path_key(&view.path);
                    view.zoom = view.zoom.clamp(0.08, 64.0);
                    if seen.insert(view.path.clone()) {
                        history.0.push(view);
                    }
                }
            }
        }
        history
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        let path = path_key(path);
        self.0.iter().any(|entry| entry.path == path)
    }

    fn upsert(&mut self, entry: RememberedView) {
        self.0.retain(|saved| saved.path != entry.path);
        self.0.insert(0, entry);
        self.0.truncate(MAX_VIEWS);
    }

    fn capture(&mut self, path: &str, view: &DocView) {
        if path.is_empty() || path.len() > MAX_PATH_BYTES || !view.zoom.is_finite() {
            return;
        }
        self.upsert(RememberedView {
            path: path_key(path),
            page: view.current,
            zoom: view.zoom.clamp(0.08, 64.0),
            fit: view.fit,
            layout: view.layout,
            continuous: view.continuous,
            cover: view.cover,
        });
    }
}

impl PdfCraftApp {
    pub(crate) fn remember_view(&mut self, index: usize) {
        if let Some(view) = self.views.get(index)
            && let Some(path) = self.session.get(view.id).and_then(|doc| doc.path.as_deref())
        {
            self.remembered_views.capture(path, view);
        }
    }

    pub(crate) fn remembered_views_to_save(&self) -> RememberedViews {
        let mut history = self.remembered_views.clone();
        // Capture open tabs too: autosave must not depend on closing a document.
        for view in &self.views {
            if let Some(path) = self.session.get(view.id).and_then(|doc| doc.path.as_deref()) {
                history.capture(path, view);
            }
        }
        history
    }

    pub(crate) fn restore_remembered_view(&mut self, index: usize) {
        let Some(view) = self.views.get_mut(index) else {
            return;
        };
        let Some(path) = self.session.get(view.id).and_then(|doc| doc.path.as_deref()) else {
            return;
        };
        let path = path_key(path);
        let Some(saved) = self.remembered_views.0.iter().find(|entry| entry.path == path) else {
            return;
        };
        view.set_display(saved.layout, saved.continuous);
        view.set_cover(saved.cover);
        view.set_zoom(saved.zoom);
        view.set_fit(saved.fit);
        // The file may have changed: go_to_page clamps to its current page count.
        view.go_to_page(saved.page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(path: String) -> RememberedView {
        RememberedView { path: path_key(&path), page: 7, zoom: 1.375, fit: Fit::None, layout: PageLayout::TwoUp, continuous: false, cover: true }
    }

    #[test]
    fn malformed_history_is_skipped_deduplicated_and_bounded() {
        let good = serde_json::to_value(entry("book.pdf".into())).unwrap();
        let mut bad = good.clone();
        bad["fit"] = serde_json::json!("unknown");
        let mut negative = good.clone();
        negative["page"] = serde_json::json!(-1);
        let mut extreme = good.clone();
        extreme["zoom"] = serde_json::json!(1e300);
        let mut oversized_path = good.clone();
        oversized_path["path"] = serde_json::json!("x".repeat(MAX_PATH_BYTES + 1));
        let history = RememberedViews::from_json(&serde_json::json!([null, bad, negative, extreme, oversized_path, good, good]));
        assert_eq!(history.0.len(), 1);
        assert!(RememberedViews::from_json(&serde_json::json!(null)).0.is_empty());
        let entries: Vec<_> = (0..6_000).map(|i| serde_json::to_value(entry(format!("{i}.pdf"))).unwrap()).collect();
        assert_eq!(RememberedViews::from_json(&serde_json::json!(entries)).0.len(), 5_000);
    }

    #[test]
    fn five_thousand_records_survive_roundtrip_and_only_oldest_is_evicted() {
        let mut history = RememberedViews::default();
        for i in 0..5_000 {
            history.upsert(entry(format!("{i}.pdf")));
        }
        let mut history = RememberedViews::from_json(&serde_json::to_value(history).unwrap());
        assert_eq!(history.0.len(), 5_000);
        assert!(history.contains("0.pdf"));
        history.upsert(entry("0.pdf".into()));
        history.upsert(entry("new.pdf".into()));
        assert_eq!(history.0.len(), 5_000);
        assert!(history.contains("0.pdf"));
        assert!(history.contains("new.pdf"));
        assert!(!history.contains("1.pdf"));
    }
}
