//! Kobo `.kepub` transformation, by the family's
//! [kepubverto](https://crates.io/crates/kepubverto).
//!
//! Kobo e-readers gain their richer reading features (accurate page turns,
//! reading statistics, dictionary lookup) when a book's content documents carry
//! Kobo's `koboSpan` markup. kepubverto adds it the way kepubify does, with the
//! same span ids, but at byte offsets: every other byte of a document stays as
//! it was, and the style element it adds is valid in EPUB 2 and EPUB 3.
//!
//! It runs on the packaged book, after the normal pipeline, so a `.kepub.epub`
//! is still a valid EPUB 3 — Kobo keys on the `.kepub.epub` extension and the
//! spans.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Add Kobo markup to the packaged EPUB at `epub`, in place.
///
/// A content document kepubverto cannot convert is left as it was, with a
/// warning, so a single odd document never aborts the conversion.
pub fn kobo_spanify(epub: &Path, progress: &dyn Fn(&str)) -> Result<()> {
    let book = fs::read(epub)?;
    let conversion = kepubverto::convert(&book).context("kepubverto could not convert the book")?;
    for (path, reason) in conversion.left_alone() {
        progress(&format!(
            "  [!] Warning: could not add koboSpans to {path}: {reason}"
        ));
    }
    fs::write(epub, &conversion.epub)?;
    progress(&format!(
        "  [+] Added {} Kobo koboSpan(s) to {} content document(s).",
        conversion.spans(),
        conversion
            .documents
            .iter()
            .filter(|d| matches!(d.outcome, kepubverto::Outcome::Converted { .. }))
            .count()
    ));
    Ok(())
}
