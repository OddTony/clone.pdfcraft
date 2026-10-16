//! The Fill & Sign library of saved signatures and, separately, saved initials (#1288).
//!
//! Each kind keeps up to [`MAX_SAVED_SIGNATURES`] entries. One is *in use*: it is what the
//! Signature (or Initials) tool places, and it lives where it always has (`PdfCraftApp::signature`
//! and `initials`, which the digital-signing graphic and older settings files also read). The
//! others live in `other_signatures` / `other_initials`, so every entry is stored exactly once.
//! The cards keep a fixed order, the order they were added (a new one goes at the end); choosing
//! one only moves the in-use mark. `slot` is where the entry in use sits in that order, and
//! [`SavedList`] is the one place that knows these rules.
//!
//! Entries get an `id` for the session so an open dialog or a pending click can name "the entry I
//! meant" even if the list changes under it (a control-channel command, a removal); ids are not
//! saved.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::fill_sign::SavedSig;

/// Most signatures kept in settings, the one in use included; initials are counted separately.
/// An image entry can be up to 4 MiB of settings, so this also bounds the settings file.
pub const MAX_SAVED_SIGNATURES: usize = 8;

/// Longest name for an entry, in characters.
pub const MAX_LABEL_CHARS: usize = 40;

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A name the user gave an entry, trimmed and cut to [`MAX_LABEL_CHARS`]; `None` for blank.
pub fn clean_label(text: &str) -> Option<String> {
    let label: String = text.trim().chars().filter(|c| !c.is_control()).take(MAX_LABEL_CHARS).collect();
    let label = label.trim().to_string();
    (!label.is_empty()).then_some(label)
}

/// What is known about the entry in use besides its image: its session id, its name and its
/// place (0-based) among all the cards.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedMeta {
    pub id: u64,
    pub label: Option<String>,
    pub slot: usize,
}

impl Default for SavedMeta {
    fn default() -> Self {
        Self { id: next_id(), label: None, slot: 0 }
    }
}

/// An entry that is saved but not in use.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedEntry {
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "lenient_label")]
    pub label: Option<String>,
    pub sig: SavedSig,
    #[serde(skip, default = "next_id")]
    pub id: u64,
}

/// A name that is not text is dropped; the entry stays.
fn lenient_label<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    use serde::Deserialize as _;
    Ok(Value::deserialize(d)?.as_str().map(str::to_string))
}

impl SavedEntry {
    pub fn new(sig: SavedSig, label: Option<String>) -> Self {
        Self { label, sig, id: next_id() }
    }
}

/// One entry as the picker draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: u64,
    pub label: Option<String>,
    pub sig: SavedSig,
    pub in_use: bool,
}

/// A new entry was refused because the list already holds [`MAX_SAVED_SIGNATURES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListFull;

/// What a picker click asked for on a card that is not a plain "use it": consumed by the
/// `sign.fill.*.change` and `.remove` commands, which otherwise act on the entry in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Start a new entry instead of changing one.
    New,
    /// The entry with this id.
    Entry(u64),
}

/// One kind's saved signatures (or initials), in the order they were added.
pub struct SavedList<'a> {
    in_use: &'a mut Option<SavedSig>,
    meta: &'a mut SavedMeta,
    others: &'a mut Vec<SavedEntry>,
}

impl<'a> SavedList<'a> {
    pub fn new(in_use: &'a mut Option<SavedSig>, meta: &'a mut SavedMeta, others: &'a mut Vec<SavedEntry>) -> Self {
        // Others with nothing in use would be unreachable from the tool: promote the first.
        if in_use.is_none() && !others.is_empty() {
            let first = others.remove(0);
            *in_use = Some(first.sig);
            *meta = SavedMeta { id: first.id, label: first.label, slot: 0 };
        }
        // A place saved by hand (or by a list since shortened) is brought back into range.
        meta.slot = meta.slot.min(others.len());
        Self { in_use, meta, others }
    }

    pub fn len(&self) -> usize {
        usize::from(self.in_use.is_some()) + self.others.len()
    }

    pub fn is_empty(&self) -> bool {
        self.in_use.is_none()
    }

    pub fn is_full(&self) -> bool {
        self.len() >= MAX_SAVED_SIGNATURES
    }

    /// Every entry in display order; the one in use is marked.
    pub fn cards(&self) -> Vec<Card> {
        let mut cards: Vec<Card> = self.others.iter().map(|e| Card { id: e.id, label: e.label.clone(), sig: e.sig.clone(), in_use: false }).collect();
        if let Some(sig) = self.in_use.as_ref() {
            let card = Card { id: self.meta.id, label: self.meta.label.clone(), sig: sig.clone(), in_use: true };
            cards.insert(self.meta.slot.min(cards.len()), card);
        }
        cards
    }

    pub fn get(&self, id: u64) -> Option<Card> {
        self.cards().into_iter().find(|c| c.id == id)
    }

    /// Put entry `id` in use. Nothing moves: only the mark changes places.
    /// `false`: there is no such entry.
    pub fn choose(&mut self, id: u64) -> bool {
        if self.in_use.is_some() && self.meta.id == id {
            return true;
        }
        let Some(i) = self.others.iter().position(|e| e.id == id) else { return false };
        let Some(old_sig) = self.in_use.take() else { return false };
        // The chosen card's place among all cards, and the old one's among the rest.
        let chosen_slot = if i < self.meta.slot { i } else { i.saturating_add(1) };
        let old_index = if self.meta.slot < chosen_slot { self.meta.slot } else { self.meta.slot.saturating_sub(1) };
        let chosen = self.others.remove(i);
        let old_meta = std::mem::replace(self.meta, SavedMeta { id: chosen.id, label: chosen.label, slot: chosen_slot });
        *self.in_use = Some(chosen.sig);
        self.others.insert(old_index.min(self.others.len()), SavedEntry { label: old_meta.label, sig: old_sig, id: old_meta.id });
        true
    }

    /// Forget entry `id`. Removing the one in use puts the next card in use, or the previous one
    /// if it was last.
    pub fn remove(&mut self, id: u64) {
        if self.in_use.is_some() && self.meta.id == id {
            let slot = self.meta.slot;
            *self.in_use = None;
            *self.meta = SavedMeta::default();
            if self.others.is_empty() {
                return;
            }
            // The next card is others[slot]; if there is none the previous one, others[slot - 1].
            let (index, new_slot) = if slot < self.others.len() { (slot, slot) } else { (self.others.len() - 1, self.others.len() - 1) };
            let next = self.others.remove(index);
            *self.in_use = Some(next.sig);
            *self.meta = SavedMeta { id: next.id, label: next.label, slot: new_slot };
        } else if let Some(i) = self.others.iter().position(|e| e.id == id) {
            self.others.remove(i);
            if i < self.meta.slot {
                self.meta.slot -= 1;
            }
        }
    }

    /// Replace entry `target` (keeping its place), or, with `None` or an entry that has been
    /// removed meanwhile, add `sig` as a new entry at the end and put it in use. A full list is
    /// never trimmed: `Err` leaves everything as it was. Returns the id of the entry written.
    pub fn store(&mut self, sig: SavedSig, label: Option<String>, target: Option<u64>) -> Result<u64, ListFull> {
        if let Some(id) = target {
            if self.in_use.is_some() && self.meta.id == id {
                *self.in_use = Some(sig);
                self.meta.label = label;
                return Ok(id);
            }
            if let Some(entry) = self.others.iter_mut().find(|e| e.id == id) {
                entry.sig = sig;
                entry.label = label;
                return Ok(id);
            }
        }
        if self.is_full() {
            return Err(ListFull);
        }
        let slot = self.len();
        let fresh = SavedMeta { id: next_id(), label, slot };
        let id = fresh.id;
        if let Some(old) = self.in_use.replace(sig) {
            let old_meta = std::mem::replace(self.meta, fresh);
            // The old one keeps its place; the new one is last.
            self.others.insert(old_meta.slot.min(self.others.len()), SavedEntry { label: old_meta.label, sig: old, id: old_meta.id });
        } else {
            *self.meta = SavedMeta { slot: 0, ..fresh };
        }
        Ok(id)
    }
}

impl SavedSig {
    /// Settings may be edited by hand: drawn points must be finite and typed text drawable.
    pub fn is_valid(&self) -> bool {
        match self {
            SavedSig::Drawn(strokes) => strokes.iter().flatten().flatten().all(|v| v.is_finite()),
            SavedSig::Typed(text) => {
                !text.trim().is_empty() && text.chars().take(pdfcraft_engine::MAX_SIGNATURE_CHARS + 1).count() <= pdfcraft_engine::MAX_SIGNATURE_CHARS
            }
            SavedSig::Image(_) => true,
        }
    }
}

impl crate::PdfCraftApp {
    /// The saved signatures, or with `initials` the saved initials.
    pub fn saved_list(&mut self, initials: bool) -> SavedList<'_> {
        if initials {
            SavedList::new(&mut self.initials, &mut self.initials_meta, &mut self.other_initials)
        } else {
            SavedList::new(&mut self.signature, &mut self.signature_meta, &mut self.other_signatures)
        }
    }

    /// Add the library's keys to the settings: the entry in use keeps its older keys, and its name
    /// and the other entries are new ones that older versions ignore.
    pub(crate) fn persist_saved_signatures(&self, v: &mut Value) {
        v["signature_label"] = serde_json::json!(self.signature.as_ref().and(self.signature_meta.label.as_ref()));
        v["initials_label"] = serde_json::json!(self.initials.as_ref().and(self.initials_meta.label.as_ref()));
        // Where the entry in use sits among all the cards; the others are written in order.
        v["signature_slot"] = serde_json::json!(self.signature_meta.slot.min(self.other_signatures.len()));
        v["initials_slot"] = serde_json::json!(self.initials_meta.slot.min(self.other_initials.len()));
        v["other_signatures"] = serde_json::json!(self.other_signatures);
        v["other_initials"] = serde_json::json!(self.other_initials);
    }

    /// Read what [`Self::persist_saved_signatures`] wrote, entry by entry: a damaged entry loses
    /// only itself, and no more than [`MAX_SAVED_SIGNATURES`] are kept. Settings written before
    /// the library existed have none of these keys and leave the list as it is.
    pub(crate) fn restore_saved_signatures(&mut self, v: &Value) {
        if ["signature", "signature_text", "signature_image"].iter().any(|k| !v[*k].is_null()) {
            self.signature_meta =
                SavedMeta { id: next_id(), label: v["signature_label"].as_str().and_then(clean_label), slot: slot_of(&v["signature_slot"]) };
        }
        if !v["initials"].is_null() {
            self.initials_meta =
                SavedMeta { id: next_id(), label: v["initials_label"].as_str().and_then(clean_label), slot: slot_of(&v["initials_slot"]) };
        }
        if let Some(others) = read_entries(&v["other_signatures"], self.signature.is_some()) {
            self.other_signatures = others;
        }
        if let Some(others) = read_entries(&v["other_initials"], self.initials.is_some()) {
            self.other_initials = others;
        }
        // Others without one in use are promoted, and the total is held to the limit.
        self.saved_list(false);
        self.saved_list(true);
    }
}

/// A saved card place; anything that is not a small count (older files have none) is the first.
fn slot_of(value: &Value) -> usize {
    value.as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(0)
}

/// The entries in a settings array that read cleanly, up to what fits beside an entry in use.
/// `None` when the key is absent or not an array.
fn read_entries(value: &Value, has_in_use: bool) -> Option<Vec<SavedEntry>> {
    let room = MAX_SAVED_SIGNATURES - usize::from(has_in_use);
    let entries = value.as_array()?;
    Some(
        entries
            .iter()
            .filter_map(|e| {
                let mut entry = serde_json::from_value::<SavedEntry>(e.clone()).ok()?;
                entry.label = entry.label.as_deref().and_then(clean_label);
                entry.sig.is_valid().then_some(entry)
            })
            .take(room)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(name: &str) -> SavedSig {
        SavedSig::Typed(name.into())
    }

    struct Lists {
        in_use: Option<SavedSig>,
        meta: SavedMeta,
        others: Vec<SavedEntry>,
    }

    impl Lists {
        fn new() -> Self {
            Self { in_use: None, meta: SavedMeta::default(), others: Vec::new() }
        }

        fn list(&mut self) -> SavedList<'_> {
            SavedList::new(&mut self.in_use, &mut self.meta, &mut self.others)
        }

        /// Entries in order, as `text` or `text (label)`.
        fn shown(&mut self) -> Vec<String> {
            self.list()
                .cards()
                .into_iter()
                .map(|c| match (&c.sig, &c.label) {
                    (SavedSig::Typed(t), Some(l)) => format!("{t} ({l})"),
                    (SavedSig::Typed(t), None) => t.clone(),
                    (other, _) => format!("{other:?}"),
                })
                .collect()
        }

        fn id_of(&mut self, text: &str) -> u64 {
            self.list().cards().into_iter().find(|c| c.sig == typed(text)).map_or(0, |c| c.id)
        }
    }

    /// The text of the card in use.
    fn in_use(l: &mut Lists) -> String {
        l.list()
            .cards()
            .into_iter()
            .find(|c| c.in_use)
            .map_or_else(String::new, |c| format!("{:?}", c.sig).replace("Typed(\"", "").replace("\")", ""))
    }

    #[test]
    fn adding_puts_the_new_entry_in_use_at_the_end_and_never_moves_the_others() {
        let mut l = Lists::new();
        assert!(l.list().is_empty());
        for (n, name) in ["Ada", "Grace", "Alan"].into_iter().enumerate() {
            assert!(l.list().store(typed(name), None, None).is_ok());
            assert_eq!(in_use(&mut l), name, "a new one is put in use");
            assert_eq!(l.list().len(), n + 1);
        }
        assert_eq!(l.shown(), ["Ada", "Grace", "Alan"], "creation order, new ones last");
        // Adding while an earlier card is in use still adds at the end.
        let ada = l.id_of("Ada");
        assert!(l.list().choose(ada));
        assert!(l.list().store(typed("Edsger"), None, None).is_ok());
        assert_eq!(l.shown(), ["Ada", "Grace", "Alan", "Edsger"]);
        assert_eq!(in_use(&mut l), "Edsger");
    }

    #[test]
    fn choosing_only_moves_the_in_use_mark_and_keeps_names_with_their_signatures() {
        let mut l = Lists::new();
        l.list().store(typed("Ada"), Some("Work".into()), None).ok();
        l.list().store(typed("Grace"), Some("Home".into()), None).ok();
        l.list().store(typed("Alan"), None, None).ok();
        let order = ["Ada (Work)", "Grace (Home)", "Alan"];
        for name in ["Ada", "Alan", "Grace", "Alan", "Ada", "Grace"] {
            let id = l.id_of(name);
            assert!(l.list().choose(id));
            assert_eq!(in_use(&mut l), name);
            assert_eq!(l.shown(), order, "choosing {name} must not reorder the cards");
        }
        let grace = l.id_of("Grace");
        assert!(l.list().choose(grace), "the one in use stays");
        assert!(!l.list().choose(u64::MAX), "no such entry");
        assert_eq!(l.shown(), order);
    }

    #[test]
    fn changing_keeps_the_place_and_the_id_and_can_rename() {
        let mut l = Lists::new();
        l.list().store(typed("Ada"), None, None).ok();
        l.list().store(typed("Grace"), None, None).ok();
        let ada = l.id_of("Ada");
        assert_eq!(l.list().store(typed("Ada L."), Some("Work".into()), Some(ada)), Ok(ada));
        assert_eq!(l.shown(), ["Ada L. (Work)", "Grace"]);
        let grace = l.id_of("Grace");
        assert_eq!(l.list().store(typed("Grace"), Some("Home".into()), Some(grace)), Ok(grace));
        assert_eq!(l.shown(), ["Ada L. (Work)", "Grace (Home)"], "the entry in use is changed in place");
        assert_eq!(in_use(&mut l), "Grace");
    }

    #[test]
    fn an_entry_chosen_while_it_was_open_is_still_the_one_changed() {
        let mut l = Lists::new();
        l.list().store(typed("Ada"), None, None).ok();
        l.list().store(typed("Grace"), None, None).ok();
        let ada = l.id_of("Ada");
        // The dialog for Ada is open; meanwhile Ada is put in use.
        assert!(l.list().choose(ada));
        assert_eq!(l.list().store(typed("Ada L."), None, Some(ada)), Ok(ada));
        assert_eq!(l.shown(), ["Ada L.", "Grace"], "changed, not duplicated");
    }

    #[test]
    fn removing_the_one_in_use_puts_the_next_card_in_use_or_the_previous_if_it_was_last() {
        let mut l = Lists::new();
        for name in ["A", "B", "C", "D"] {
            l.list().store(typed(name), None, None).ok();
        }
        // In use is D (last): removing it picks the previous card, C.
        let d = l.id_of("D");
        l.list().remove(d);
        assert_eq!(l.shown(), ["A", "B", "C"]);
        assert_eq!(in_use(&mut l), "C");
        // In use is B (middle): removing it picks the next card, C, and nothing moves.
        let b = l.id_of("B");
        assert!(l.list().choose(b));
        l.list().remove(b);
        assert_eq!(l.shown(), ["A", "C"]);
        assert_eq!(in_use(&mut l), "C");
        // In use is A (first): the next card, C.
        let a = l.id_of("A");
        assert!(l.list().choose(a));
        l.list().remove(a);
        assert_eq!(l.shown(), ["C"]);
        assert_eq!(in_use(&mut l), "C");
        // Removing a card that is not in use keeps the mark on the same entry.
        l.list().store(typed("E"), None, None).ok();
        l.list().store(typed("F"), None, None).ok();
        let c = l.id_of("C");
        let f = l.id_of("F");
        assert!(l.list().choose(f));
        l.list().remove(c);
        assert_eq!(l.shown(), ["E", "F"]);
        assert_eq!(in_use(&mut l), "F");
        l.list().remove(u64::MAX);
        assert_eq!(l.shown(), ["E", "F"]);
        let (e, f) = (l.id_of("E"), l.id_of("F"));
        l.list().remove(e);
        l.list().remove(f);
        assert!(l.list().is_empty());
        assert!(l.in_use.is_none() && l.others.is_empty());
    }

    #[test]
    fn a_saved_place_out_of_range_is_brought_back_without_losing_entries() {
        let mut l = Lists::new();
        l.in_use = Some(typed("Ada"));
        l.meta.slot = 99;
        l.others = vec![SavedEntry::new(typed("Grace"), None)];
        assert_eq!(l.shown(), ["Grace", "Ada"]);
        assert_eq!(l.list().len(), 2);
    }

    #[test]
    fn a_removed_entry_that_was_being_changed_is_added_back_as_a_new_one() {
        let mut l = Lists::new();
        l.list().store(typed("Ada"), None, None).ok();
        let ada = l.id_of("Ada");
        l.list().remove(ada);
        assert!(l.list().store(typed("Ada again"), None, Some(ada)).is_ok());
        assert_eq!(l.shown(), ["Ada again"]);
    }

    #[test]
    fn a_full_list_refuses_a_new_entry_rather_than_dropping_one_but_still_changes_one() {
        let mut l = Lists::new();
        for i in 0..MAX_SAVED_SIGNATURES {
            assert!(l.list().store(typed(&i.to_string()), None, None).is_ok());
        }
        assert!(l.list().is_full());
        assert_eq!(l.list().store(typed("one more"), None, None), Err(ListFull));
        assert_eq!(l.list().len(), MAX_SAVED_SIGNATURES);
        assert!(!l.shown().contains(&"one more".to_string()));
        let oldest = l.id_of("0");
        assert!(l.list().store(typed("changed"), None, Some(oldest)).is_ok(), "changing one still works when full");
        assert_eq!(l.list().len(), MAX_SAVED_SIGNATURES);
        // A stale target on a full list is refused too, not turned into a ninth entry.
        assert_eq!(l.list().store(typed("stale"), None, Some(u64::MAX)), Err(ListFull));
    }

    #[test]
    fn others_without_one_in_use_are_promoted() {
        let mut l = Lists::new();
        l.others = vec![SavedEntry::new(typed("Ada"), Some("Work".into())), SavedEntry::new(typed("Grace"), None)];
        assert_eq!(l.shown(), ["Ada (Work)", "Grace"]);
        assert!(l.in_use.is_some() && l.others.len() == 1);
    }

    #[test]
    fn labels_are_trimmed_cut_and_blank_ones_dropped() {
        assert_eq!(clean_label("  Work \n"), Some("Work".into()));
        assert_eq!(clean_label("   "), None);
        assert_eq!(clean_label(""), None);
        let long = "é".repeat(MAX_LABEL_CHARS + 10);
        assert_eq!(clean_label(&long).map(|l| l.chars().count()), Some(MAX_LABEL_CHARS));
    }

    #[test]
    fn invalid_saved_values_are_rejected() {
        assert!(typed("Ada").is_valid());
        assert!(!typed("   ").is_valid());
        assert!(!SavedSig::Drawn(vec![vec![[f32::NAN, 0.0]]]).is_valid());
        assert!(SavedSig::Drawn(vec![vec![[0.0, 0.5], [1.0, 0.5]]]).is_valid());
        assert!(!typed(&"x".repeat(pdfcraft_engine::MAX_SIGNATURE_CHARS + 1)).is_valid());
    }
}
