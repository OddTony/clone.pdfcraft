//! Document-only scrollbar sizing (#2056). All dimensions are egui points, following display scale.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentScrollbarWidth {
    #[default]
    Standard,
    Wide,
}

impl DocumentScrollbarWidth {
    pub const ALL: [Self; 2] = [Self::Standard, Self::Wide];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Wide => "Wide",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.label().eq_ignore_ascii_case(value))
    }

    pub(crate) fn apply(self, style: &mut egui::style::ScrollStyle) {
        if self == Self::Wide {
            // egui hit-tests bar_width even while a floating bar is drawn narrower. Widen
            // both so the visible handle and the pointer target are consistently 16 points.
            // Retain floating allocation, opacity and colours from the active theme.
            style.bar_width = 16.0;
            style.floating_width = 16.0;
        }
    }
}
