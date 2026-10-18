//! Small shared helpers: URL (percent) encoding compatible with Python's
//! `urllib.parse.quote`/`unquote`, href manipulation, and XHTML DOCTYPE
//! standardization.

use any_ascii::any_ascii;
use anyhow::Result;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use regex::Regex;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Parse XML text taken from a book with roxmltree — after epubveri's
/// `xmlguard::check`, which declines a document roxmltree can't safely take:
/// nesting deep enough to overflow the stack (an abort, not a catchable
/// panic — one upload would take the whole web server down), attribute or
/// element counts that make parsing quadratic or unbounded, or entities that
/// expand without limit. The limits sit far above any real book (the deepest
/// document on our 474-book shelf nests 24 elements; the limit is 256).
pub fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>> {
    epubveri::xmlguard::check(text)?;
    Ok(roxmltree::Document::parse(text)?)
}

/// [`parse_xml`] for a content document, which may carry a DOCTYPE (XHTML 1.1
/// pages do; roxmltree refuses one unless asked). The guard has already
/// bounded entity expansion.
pub fn parse_xml_with_dtd(text: &str) -> Result<roxmltree::Document<'_>> {
    epubveri::xmlguard::check(text)?;
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    Ok(roxmltree::Document::parse_with_options(text, opts)?)
}

/// Characters that Python's `urllib.parse.quote` leaves untouched by default:
/// unreserved characters (`A-Z a-z 0-9 _ . - ~`) plus the `/` path separator
/// (because `quote` uses `safe='/'` by default).
const QUOTE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'-')
    .remove(b'~')
    .remove(b'/');

/// Percent-encode a string the way Python's `urllib.parse.quote` does.
pub fn quote(s: &str) -> String {
    utf8_percent_encode(s, QUOTE_SET).to_string()
}

/// Percent-decode a string the way Python's `urllib.parse.unquote` does.
pub fn unquote(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// True when an archive entry's contents are **text/markup** (XHTML/CSS/OPF/…)
/// rather than already-compressed media (images/fonts) or the `mimetype`.
///
/// Classification is purely by extension so independent code paths (the archival
/// packer and the experimental Zstd-OCF packer) agree without a side manifest.
/// `mimetype` is excluded so it is always stored verbatim/first.
#[cfg(feature = "zstd-experimental")]
pub fn is_text_entry(name: &str) -> bool {
    if name == "mimetype" {
        return false;
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "xhtml" | "html" | "htm" | "css" | "opf" | "ncx" | "xml" | "svg" | "txt" | "json"
    )
}

/// Return the final path component (everything after the last `/`).
pub fn basename(href: &str) -> &str {
    match href.rfind('/') {
        Some(i) => &href[i + 1..],
        None => href,
    }
}

/// Replace the file extension of an href with `.{ext}`, mirroring
/// `pathlib.Path(href).with_suffix('.{ext}')`. `ext` is given without the dot.
pub fn with_ext(href: &str, ext: &str) -> String {
    let slash = href.rfind('/').map(|i| i as isize).unwrap_or(-1);
    match href.rfind('.') {
        Some(dot) if (dot as isize) > slash => format!("{}.{ext}", &href[..dot]),
        _ => format!("{href}.{ext}"),
    }
}

/// Transliterate a filename stem to an ASCII-safe slug for the `--ascii` option.
/// Unicode letters are romanized (e.g. Turkish `Işık Doğudan` → `Isik_Dogudan`),
/// whitespace becomes underscores, and any character outside `[A-Za-z0-9._-]` is
/// dropped. Runs of underscores are collapsed and leading/trailing ones trimmed.
/// Falls back to `output` if nothing printable remains.
pub fn slugify_ascii(stem: &str) -> String {
    let romanized = any_ascii(stem);
    let mut out = String::with_capacity(romanized.len());
    let mut prev_underscore = false;
    for ch in romanized.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
            out.push(ch);
            prev_underscore = false;
        } else if (ch.is_whitespace() || ch == '_') && !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
        // anything else (punctuation, symbols) is dropped
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "output".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Standardize HTML/XHTML files to EPUB 3 best practices:
/// - Replace legacy DOCTYPE declarations with the HTML5 `<!DOCTYPE html>`.
///   An XHTML 1.x DOCTYPE declared the HTML named entities (`&nbsp;`, …); its
///   replacement declares none, so the references are written out as the
///   characters they denote (see [`resolve_dtd_entities`]).
/// - Ensure the XHTML namespace is declared on the `<html>` element.
/// - Drop an SVG 1.1 DOCTYPE (`OPF-073`: EPUB 3 allows no external identifier
///   on SVG), unless it has an internal subset, whose entities the file needs.
pub fn standardize_xhtml_files(root: &Path, progress: &dyn Fn(&str)) -> Result<()> {
    let doctype_re = Regex::new(r"(?i)<!DOCTYPE\s+html[^>]*>").unwrap();
    let svg_doctype_re = Regex::new(r"<!DOCTYPE\s+svg\s+(?:PUBLIC|SYSTEM)[^>\[]*>\s*").unwrap();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if ext == "svg" {
            if let Ok(raw) = fs::read(path)
                && let Ok(content) = std::str::from_utf8(&raw)
                && svg_doctype_re.is_match(content)
            {
                let _ = fs::write(path, svg_doctype_re.replace(content, "").as_bytes());
            }
            continue;
        }
        if !matches!(ext.as_str(), "html" | "xhtml" | "htm") {
            continue;
        }

        let raw = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                progress(&format!(
                    "  [!] Warning: Could not modernize HTML tag in {}: {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    e
                ));
                continue;
            }
        };
        let mut content = String::from_utf8_lossy(&raw).into_owned();

        // Only a DOCTYPE with an external identifier (XHTML 1.0/1.1) declared
        // the entities; a book already on `<!DOCTYPE html>` gains nothing here.
        let declared_entities = doctype_re
            .find(&content)
            .is_some_and(|m| m.as_str().contains("PUBLIC") || m.as_str().contains("SYSTEM"));
        content = doctype_re
            .replace_all(&content, "<!DOCTYPE html>")
            .into_owned();
        if declared_entities {
            content = resolve_dtd_entities(&content);
        }

        if !content.contains("xmlns=\"http://www.w3.org/1999/xhtml\"") {
            content = content.replace("<html", "<html xmlns=\"http://www.w3.org/1999/xhtml\"");
        }

        if let Err(e) = fs::write(path, content) {
            progress(&format!(
                "  [!] Warning: Could not modernize HTML tag in {}: {}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                e
            ));
        }
    }
    Ok(())
}

/// Replace each named reference the XHTML 1.x DTDs declared (`&nbsp;`,
/// `&rsquo;`, …) with the character it denotes, as the DTD defined it. The
/// table is epubsana's (`epubsana::entities::lookup`, from epubveri's DTD
/// sets), so the upgrade and the repair agree on every name; the XML five
/// (`&amp;`, …) and names outside those sets are left as written. Comments and
/// CDATA sections are copied unchanged: a reference there was never expanded.
pub fn resolve_dtd_entities(content: &str) -> String {
    let skip_re = Regex::new(r"(?s)<!--.*?-->|<!\[CDATA\[.*?\]\]>").unwrap();
    let ref_re = Regex::new(r"&([A-Za-z][A-Za-z0-9]*);").unwrap();
    let resolve = |text: &str| {
        ref_re
            .replace_all(
                text,
                |c: &regex::Captures| match epubsana::entities::lookup(&c[1]) {
                    Some(ch) => ch.to_string(),
                    None => c[0].to_string(),
                },
            )
            .into_owned()
    };
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for m in skip_re.find_iter(content) {
        out.push_str(&resolve(&content[last..m.start()]));
        out.push_str(m.as_str());
        last = m.end();
    }
    out.push_str(&resolve(&content[last..]));
    out
}

/// Detect content documents that use **HTML syntax** rather than XHTML, which
/// EPUB 3.4 removed. The reliable signal is an HTML *void* element that isn't
/// self-closed (valid XHTML always self-closes them: `<br/>`, `<img .../>`).
/// Returns one human-readable warning per offending file. A heuristic for the
/// lint — we report it; converting HTML→XHTML is out of scope.
pub fn detect_html_syntax(root: &Path) -> Vec<String> {
    let void_re = Regex::new(
        r"(?is)<(?:br|hr|wbr|img|input|link|meta|area|base|col|embed|source|track|param)\b[^>]*>",
    )
    .unwrap();
    let mut warnings = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_html = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "html" | "xhtml" | "htm"))
            .unwrap_or(false);
        if !is_html {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        // HTML syntax = a void element tag that doesn't self-close (`...>` not `.../>`).
        let html_syntax = void_re
            .find_iter(&content)
            .any(|m| !m.as_str().trim_end().ends_with("/>"));
        if html_syntax {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            warnings.push(format!(
                "content document `{name}` uses HTML syntax (EPUB 3.4 requires XHTML)"
            ));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::{detect_html_syntax, slugify_ascii, standardize_xhtml_files};
    use std::fs;

    #[test]
    fn xhtml11_doctype_swap_writes_out_its_entities() {
        // The XHTML 1.1 DTD declared &nbsp; and &rsquo;; <!DOCTYPE html> does
        // not, so they become characters. The XML five, unknown names, comments
        // and CDATA stay as written.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.html");
        fs::write(
            &f,
            "<?xml version=\"1.0\"?>\n<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.1//EN\" \"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd\">\n\
             <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head><body>\
             <p>a&nbsp;b &rsquo;c&amp;d &NewLine; &Eacute;</p><!-- &nbsp; --><script>/*<![CDATA[*/ x=\"&nbsp;\" /*]]>*/</script></body></html>",
        )
        .unwrap();
        standardize_xhtml_files(dir.path(), &|_| {}).unwrap();
        let out = fs::read_to_string(&f).unwrap();
        assert!(out.contains("<!DOCTYPE html>"), "{out}");
        assert!(
            out.contains("<p>a\u{a0}b \u{2019}c&amp;d &NewLine; \u{c9}</p>"),
            "{out}"
        );
        assert!(out.contains("<!-- &nbsp; -->"), "{out}");
        assert!(out.contains("x=\"&nbsp;\""), "{out}");
    }

    #[test]
    fn html5_doctype_keeps_its_references() {
        // Without a DTD the references were never declared; the swap did not
        // cause them and leaves them to the repair.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("d.xhtml");
        let text = "<?xml version=\"1.0\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>a&nbsp;b</p></body></html>";
        fs::write(&f, text).unwrap();
        standardize_xhtml_files(dir.path(), &|_| {}).unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), text);
    }

    #[test]
    fn drops_svg_doctype_unless_it_declares_entities() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("a.svg");
        let subset = dir.path().join("b.svg");
        fs::write(
            &plain,
            "<?xml version=\"1.0\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();
        let with_entities = "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [<!ENTITY ns \"y\">]>\n<svg/>";
        fs::write(&subset, with_entities).unwrap();
        standardize_xhtml_files(dir.path(), &|_| {}).unwrap();
        assert_eq!(
            fs::read_to_string(&plain).unwrap(),
            "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        );
        assert_eq!(fs::read_to_string(&subset).unwrap(), with_entities);
    }

    #[test]
    fn flags_html_syntax_but_not_xhtml() {
        let dir = tempfile::tempdir().unwrap();
        // HTML syntax: bare void elements.
        fs::write(
            dir.path().join("html.xhtml"),
            "<html><body><p>Hi<br>there<img src=\"a.jpg\"></p></body></html>",
        )
        .unwrap();
        // Valid XHTML: self-closed voids.
        fs::write(
            dir.path().join("xhtml.xhtml"),
            "<html><body><p>Hi<br/>there<img src=\"a.jpg\"/></p></body></html>",
        )
        .unwrap();
        // Non-content file: ignored.
        fs::write(dir.path().join("styles.css"), "br { color: red; }").unwrap();

        let w = detect_html_syntax(dir.path());
        assert_eq!(w.len(), 1, "exactly one offending file: {w:?}");
        assert!(w[0].contains("html.xhtml"));
    }

    #[test]
    fn slugifies_turkish_title() {
        assert_eq!(
            slugify_ascii("Işık Doğudan Yükselir"),
            "Isik_Dogudan_Yukselir"
        );
    }

    #[test]
    fn collapses_and_trims_separators() {
        assert_eq!(slugify_ascii("  A  --  B!! "), "A_--_B");
        assert_eq!(slugify_ascii("Çöl: Bir_Öykü"), "Col_Bir_Oyku");
    }

    #[test]
    fn keeps_dots_and_dashes() {
        assert_eq!(slugify_ascii("Vol.2 - Part 3"), "Vol.2_-_Part_3");
    }

    #[test]
    fn empty_after_strip_falls_back() {
        assert_eq!(slugify_ascii("！？"), "output");
    }
}
