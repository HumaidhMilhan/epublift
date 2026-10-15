//! The epubsana pass that finishes an upgrade.
//!
//! Moving a book from EPUB 2 to EPUB 3 makes some of what EPUB 2 accepted
//! invalid: named entities with no DTD to declare them, inline SVG without the
//! manifest's `svg` property, an empty `<title>`, a `Content-Type` meta in its
//! EPUB 2 spelling, an empty `dc:rights`. epubsana repairs these, so the
//! upgrade runs it on the packaged book rather than repairing them itself.
//!
//! epubsana's safe fixes are applied as they are everywhere. Of the fixes that
//! need a decision, only [`UPGRADE_FIXES`] are applied: each answers something
//! EPUB 3 requires of a book EPUB 2 accepted, with one obvious answer in that
//! setting. Every other fix that needs a decision is skipped, as `epublift
//! repair` skips it without a terminal.

use anyhow::{Result, anyhow};
use epubsana::{Confirmer, Decision, Goal, Policy, ProposedFix, Workspace};
use std::fs;
use std::path::Path;

/// The fixes needing a decision that the upgrade applies without asking
/// (decided 2026-10-16): an empty `<title>` filled from the book's own table
/// of contents, heading or title; the `Content-Type` meta spelled as EPUB 3
/// requires; an empty optional metadata element dropped.
const UPGRADE_FIXES: &[&str] = &[
    "fix.empty_title",
    "fix.content_type_meta",
    "fix.empty_metadata_element",
];

/// A pass that changed the book is followed by another, at most this many in
/// all.
const MAX_PASSES: usize = 3;

struct UpgradeConfirmer;

impl Confirmer for UpgradeConfirmer {
    fn decide(&mut self, fix: &ProposedFix) -> Decision {
        if UPGRADE_FIXES.contains(&fix.fix_id) {
            Decision::Approve
        } else {
            Decision::Reject
        }
    }
}

/// Run the pass over the packaged EPUB at `epub`, in place. Returns the number
/// of fixes applied.
pub fn apply(epub: &Path, progress: &dyn Fn(&str)) -> Result<usize> {
    let bytes = fs::read(epub)?;
    let mut ws = Workspace::load(&bytes)
        .map_err(|e| anyhow!("epubsana could not open the upgraded book: {e}"))?;
    let mut applied = 0;
    let mut changed = false;
    for _ in 0..MAX_PASSES {
        let pass = epubsana::repair(
            &mut ws,
            Goal::Valid,
            Policy::AutoSafeThenAsk,
            &mut UpgradeConfirmer,
        )
        .map_err(|e| anyhow!("epubsana could not repair the upgraded book: {e}"))?;
        applied += pass
            .fixes
            .iter()
            .filter(|f| f.outcome == epubsana::Outcome::Applied)
            .count();
        // Plan again after any change, not only when epubsana counts revealed
        // findings: an entity fixed in a document lets the schema check reach
        // it, and its empty <title> shows up only then.
        if !pass.changed() {
            break;
        }
        changed = true;
    }
    if changed {
        let out = ws
            .serialize()
            .map_err(|e| anyhow!("could not write the repaired book: {e}"))?;
        fs::write(epub, out)?;
    }
    progress(&format!(
        "  [+] Applied {applied} EPUB 3 fix(es) with epubsana."
    ));
    Ok(applied)
}
