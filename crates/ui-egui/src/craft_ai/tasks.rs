//! What Craft AI does inside PdfCraft. Craft AI is one assistant across the ArtCraft family
//! — the model, the runtime and the shared folder belong to the family — but its *work* is the
//! app's own: the system prompt tells the model which app it is inside and what that app does,
//! and every ready-made task is framed for that app's world, so the answer that comes back is
//! about pages and form fields and redaction marks, not generic advice.
//!
//! Each app of the family carries its own `tasks.rs` — VectorCraft's asks about paths and
//! Pathfinder steps, GridCraft's about formulas and pivot tables, and so on — same model, same
//! shared folder, different craft. The labels and hints are catalog keys: the panel shows them
//! translated.

/// The app Craft AI is embedded in, as the shared folder's records name it.
pub const APP: &str = "PdfCraft";

/// One ready-made task of the Assistant tab.
pub struct Task {
    /// The label the picker shows (a catalog key).
    pub label: &'static str,
    /// The input field's hint (a catalog key).
    pub hint: &'static str,
}

/// The tasks, in the picker's order.
pub const TASKS: &[Task] = &[
    Task { label: "Ask anything", hint: "Ask in any language" },
    Task { label: "Organize pages plan", hint: "What you want to end up with (order, extract, split, combine)" },
    Task { label: "Form field advice", hint: "The form's purpose and its fields (names, types, tab order)" },
    Task { label: "Redaction checklist", hint: "What must go, and what the document is" },
    Task { label: "PDF/A and standards advice", hint: "The standard it must meet and where the file goes" },
    Task { label: "Compress and export", hint: "Where the file is going and what matters (size, quality, compatibility)" },
];

/// Tell the model where it is and how to answer: briefly, practically, in the language the user
/// wrote in, about this app's own world — the words of its interface, the files it opens and
/// writes. The last line is the honesty clause: a local model can be wrong, and it is told to
/// say so rather than invent. Redaction is called out by name, because it is the one thing in
/// this app that cannot be taken back.
pub fn system_prompt() -> String {
    "You are Craft AI, the assistant built into PdfCraft, a desktop PDF editor. \
     PdfCraft's world, in its own words: pages and their order — extract, split, combine, \
     rotate, move and renumber them; forms and their fields (text fields, checkboxes, radio \
     buttons, dropdowns, signatures), field naming and tab order; redaction that removes \
     content for good; text recognition (OCR); headers, footers, watermarks and backgrounds; \
     bookmarks, links, comments and annotations; PDF/A and the standards a file must meet; \
     compression and export; and pdf files opened and written. Answer in the language the user \
     wrote in, Arabic included. Be brief and practical: name the tool or the panel, give the \
     steps as a short list, say what to watch out for — and when it is about redaction, say \
     plainly that applying it is irreversible and hidden information should be removed before \
     it, not after. When a question is not about PdfCraft or PDFs, answer it anyway, briefly. \
     Never invent menu entries or panel names: if you are not sure PdfCraft has a feature, say \
     so plainly."
        .to_string()
}

/// Frame the user's input for the task they picked.
pub fn user_prompt(task: usize, input: &str) -> String {
    let input = input.trim();
    match TASKS[task.min(TASKS.len() - 1)].label {
        "Organize pages plan" => format!(
            "Plan the steps in PdfCraft to reorganize this document's pages — name the tool for each step, and check the plan against what the user wants: {input}"
        ),
        "Form field advice" => format!("Advise on this PDF form in PdfCraft — field types, names, and the tab order people will fill them in: {input}"),
        "Redaction checklist" => format!(
            "Write a checklist for redacting this PDF in PdfCraft — what to search for, what to mark, what to check before applying. Say that applying redaction is irreversible: {input}"
        ),
        "PDF/A and standards advice" => format!("Advise on making this document meet a PDF standard in PdfCraft — which flavour, what it requires, what will not survive the conversion: {input}"),
        "Compress and export" => format!("Advise on compressing or exporting this PDF in PdfCraft for where it is going — image quality, fonts, what to drop — in a short list: {input}"),
        _ => input.to_string(),
    }
}
