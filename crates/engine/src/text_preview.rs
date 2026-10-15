//! The page beneath an inline text editor, with the source glyphs removed on a COS clone.

use std::sync::Arc;

use pdfcraft_render::{PageRenderer, RenderConfig, RenderPool, RenderRequest, RenderedPage};

use crate::{Document, guard};

#[derive(Clone, Default)]
pub(crate) struct EditorFontsCache(Arc<std::sync::Mutex<Option<CachedFonts>>>);
type CachedFonts = (pdfcraft_cos::SourceIdentity, usize, bool, Vec<pdfcraft_edit::text::TextEditorFont>);

pub struct TextEditPreview {
    page: usize,
    bytes: Arc<Vec<u8>>,
    config: RenderConfig,
}

impl TextEditPreview {
    pub fn renderer(&self) -> RenderPool {
        RenderPool::new(self.bytes.clone(), 1, self.config.clone())
    }

    pub fn render_background(&self, scale: f32) -> Result<RenderedPage, String> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("preview scale must be finite and positive".into());
        }
        guard(|| {
            let mut renderer = PageRenderer::new(self.bytes.clone(), self.config.clone());
            let out = renderer.render(RenderRequest { page: self.page, scale, ..Default::default() });
            if let Some(e) = &out.error {
                return Err(e.clone());
            }
            Ok(out)
        })?
    }
}

impl Document {
    /// Measure a draft without changing bytes, dirty state, or undo history.
    pub fn text_edit_layout(
        &self,
        page: usize,
        block: usize,
        text: &str,
        style: &pdfcraft_edit::BlockStyle,
    ) -> Result<pdfcraft_edit::text::TextEditLayout, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            pdfcraft_edit::text::text_edit_layout(&editor.cos, page, block, text, style).map_err(|e| e.to_string())
        })?
    }

    /// Source faces and installed reading substitutes for an on-screen editor. Neither this
    /// query nor its preview fonts changes the document or supplies an embedding input.
    pub fn text_editor_fonts(&self, page: usize) -> Vec<pdfcraft_edit::text::TextEditorFont> {
        let Some(editor) = self.editor.as_ref() else { return Vec::new() };
        let enabled = std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_none_or(|v| v != "0");
        let source = (!editor.cos.is_modified()).then(|| editor.cos.source_identity());
        if let Some(source) = &source
            && let Ok(cache) = editor.editor_fonts.0.lock()
            && let Some((identity, p, system, fonts)) = cache.as_ref()
            && source == identity
            && *p == page
            && *system == enabled
        {
            return fonts.clone();
        }
        let mut fonts = self.editor.as_ref().and_then(|e| pdfcraft_edit::text::text_editor_fonts(&e.cos, page).ok()).unwrap_or_default();
        // SimSun, KaiTi and SimHei together exceed 32 MiB. Bound the complete page catalogue,
        // and cache it so the Format panel does not copy font programs on every frame.
        let mut remaining = (64usize << 20).saturating_sub(fonts.iter().map(|f| f.bytes.len()).sum::<usize>());
        let mut seen: std::collections::HashSet<_> = fonts.iter().map(|f| f.resource.clone()).collect();
        for line in self.text_lines(page).into_iter().take(4096) {
            if remaining == 0 || !seen.insert(line.font.clone()) {
                continue;
            }
            if let Some((bytes, index)) = pdfcraft_render::installed_editor_preview(&line.base_font)
                && bytes.len() <= remaining
            {
                remaining -= bytes.len();
                fonts.push(pdfcraft_edit::text::TextEditorFont { resource: line.font, face: line.base_font, bytes: Arc::new(bytes), index });
            }
        }
        if let Some(source) = source
            && let Ok(mut cache) = editor.editor_fonts.0.lock()
        {
            *cache = Some((source, page, enabled, fonts.clone()));
        }
        fonts
    }

    /// Preparing the background never changes working bytes, dirty state or undo history.
    pub fn text_edit_preview(&self, page: usize, block: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::clear_block(&mut cos, page, block).map_err(|e| e.to_string())?;
            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: self.config.clone() })
        })?
    }

    /// The page beneath an added item while it is dragged: `index` counts the page's added
    /// items, as `Edit::DeleteContent` does. Read-only, like [`Self::text_edit_preview`].
    pub fn added_content_preview(&self, page: usize, index: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::delete_content(&mut cos, page, index).map_err(|e| e.to_string())?;
            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: self.config.clone() })
        })?
    }
}
