//! Comparison-only normalization; the document's original text is never rewritten here.

pub(crate) fn normalize(text: &str) -> String {
    text.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ")
}

/// A removed run with optional word spaces at forward `TJ` displacements. Kerning is ambiguous:
/// compare both interpretations without enumerating the exponentially many possible strings.
pub(crate) struct KernedFragment {
    pub text: String,
    pub gaps: Vec<usize>,
}

impl KernedFragment {
    pub fn normalized(&self) -> Self {
        let mut text = String::new();
        let mut gaps = Vec::new();
        let (mut space, mut gap) = (false, false);
        for (offset, c) in self.text.char_indices() {
            gap |= self.gaps.binary_search(&offset).is_ok();
            if c.is_whitespace() {
                space = !text.is_empty();
                continue;
            }
            if space {
                text.push(' ');
            } else if gap && !text.is_empty() {
                gaps.push(text.len());
            }
            text.extend(c.to_lowercase());
            space = false;
            gap = false;
        }
        Self { text, gaps }
    }

    pub fn matches(&self, value: &str) -> bool {
        let Some(first) = self.text.chars().next() else { return false };
        // A complete removed run inside a metadata value.
        if value.match_indices(first).any(|(start, _)| self.prefix(0, value.get(start..).unwrap_or_default(), true)) {
            return true;
        }
        let Some(first) = value.chars().next() else { return false };
        // Or the complete metadata value inside a removed run. Never match isolated terms.
        self.text.match_indices(first).any(|(start, _)| self.prefix(start, value, false))
    }

    fn prefix(&self, start: usize, value: &str, whole_run: bool) -> bool {
        let Some(run) = self.text.get(start..) else { return false };
        let mut value = value.chars().peekable();
        for (offset, c) in run.char_indices() {
            if value.peek().is_none() {
                return !whole_run;
            }
            if offset > 0 && self.gaps.binary_search(&start.saturating_add(offset)).is_ok() && value.peek() == Some(&' ') {
                value.next();
            }
            if value.next() != Some(c) {
                return false;
            }
        }
        whole_run || value.next().is_none()
    }
}

/// XML parsers expose decoded text and attributes, excluding tag/attribute names and namespace
/// declarations. DTDs are disabled by the parser's defaults. Uninspectable XML fails closed.
pub(crate) fn packet_hit(text: &str, hit: &impl Fn(&str) -> bool) -> bool {
    let Ok(doc) = roxmltree::Document::parse(text) else { return true };
    doc.descendants().any(|node| {
        ((node.is_text() || node.is_comment()) && node.text().is_some_and(hit)) || node.attributes().any(|attribute| hit(attribute.value()))
    })
}
