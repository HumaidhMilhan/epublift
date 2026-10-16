//! Generation of an EPUB 3 XHTML Navigation Document (`nav.xhtml`) from a
//! legacy `toc.ncx`, including a `landmarks` block built from the OPF `<guide>`.

use crate::opf::GuideRef;
use anyhow::{Context, Result};
use quick_xml::escape::escape;
use regex::Regex;
use std::fs;
use std::path::Path;

/// Remove a `<!DOCTYPE ...>` declaration so the strict, DTD-rejecting XML parser
/// (roxmltree) will accept the document. NCX files commonly carry a DOCTYPE.
fn strip_doctype(xml: &str) -> String {
    let re = Regex::new(r"(?is)<!DOCTYPE[^>]*>").unwrap();
    re.replace(xml, "").into_owned()
}

/// A node in the navigation tree parsed from `toc.ncx`.
struct NavPoint {
    title: String,
    href: String,
    children: Vec<NavPoint>,
}

/// Parse `toc.ncx` and write a valid EPUB 3 Navigation Document to `out_path`.
pub fn generate_nav_xhtml(ncx_path: &Path, out_path: &Path, guide_refs: &[GuideRef]) -> Result<()> {
    let raw = fs::read_to_string(ncx_path).context("Failed to read toc.ncx")?;
    let xml = strip_doctype(&raw);
    let doc = crate::util::parse_xml(&xml).context("Failed to parse toc.ncx")?;

    // Document title.
    let title_text = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "docTitle")
        .and_then(|d| {
            d.children()
                .find(|n| n.is_element() && n.tag_name().name() == "text")
        })
        .and_then(|t| t.text())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Table of Contents".to_string());
    let title_text = escape(&title_text);

    // Navigation tree.
    let toc_items = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "navMap")
        .map(|nav_map| parse_nav_points(nav_map))
        .unwrap_or_default();

    let mut out: Vec<String> = Vec::new();
    out.push(r#"<?xml version="1.0" encoding="utf-8"?>"#.to_string());
    out.push("<!DOCTYPE html>".to_string());
    out.push(r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="en" xml:lang="en">"#.to_string());
    out.push("<head>".to_string());
    out.push(format!("  <title>{}</title>", title_text));
    out.push(r#"  <meta charset="utf-8" />"#.to_string());
    out.push("  <style>".to_string());
    out.push("    body { font-family: sans-serif; margin: 2em; }".to_string());
    out.push("    nav ol { list-style-type: none; padding-left: 1.5em; }".to_string());
    out.push("    nav ol li { margin: 0.5em 0; }".to_string());
    out.push("    a { text-decoration: none; color: #1a73e8; }".to_string());
    out.push("    a:hover { text-decoration: underline; }".to_string());
    out.push("    h1 { color: #333333; }".to_string());
    out.push("  </style>".to_string());
    out.push("</head>".to_string());
    out.push("<body>".to_string());
    out.push(r#"  <nav epub:type="toc" id="toc">"#.to_string());
    out.push(format!("    <h1>{}</h1>", title_text));

    if toc_items.is_empty() {
        out.push("    <p>No table of contents available.</p>".to_string());
    } else {
        render_ol(&toc_items, 2, &mut out);
    }

    out.push("  </nav>".to_string());

    // Landmarks derived from the legacy <guide>.
    if !guide_refs.is_empty() {
        out.push("\n  <nav epub:type=\"landmarks\" id=\"landmarks\" hidden=\"\">".to_string());
        out.push("    <h2>Guide Landmarks</h2>".to_string());
        out.push("    <ol>".to_string());
        let mut seen = std::collections::HashSet::new();
        for r in guide_refs {
            let href = nav_href(&r.href);
            let mapped = map_landmark_type(&r.ref_type);
            // EPUB 3 forbids two landmarks with the same type and target.
            if !seen.insert((mapped.clone(), href.clone())) {
                continue;
            }
            // A landmark needs a label; a `<reference>` without a title
            // falls back to its type ("Cover").
            let label = match r.title.trim() {
                "" => capitalize_first(&mapped),
                t => t.to_string(),
            };
            out.push(format!(
                "      <li><a epub:type=\"{}\" href=\"{}\">{}</a></li>",
                escape(&mapped),
                escape(&href),
                escape(&label)
            ));
        }
        out.push("    </ol>".to_string());
        out.push("  </nav>".to_string());
    }

    out.push("</body>".to_string());
    out.push("</html>".to_string());

    fs::write(out_path, out.join("\n"))?;
    Ok(())
}

/// Recursively collect `<navPoint>` children of `parent`.
fn parse_nav_points(parent: roxmltree::Node) -> Vec<NavPoint> {
    let mut items = Vec::new();
    for p in parent
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
    {
        let title = p
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "navLabel")
            .and_then(|label| {
                label
                    .children()
                    .find(|n| n.is_element() && n.tag_name().name() == "text")
            })
            .and_then(|t| t.text())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("Untitled")
            .to_string();

        let href = p
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "content")
            .and_then(|c| c.attribute("src"))
            .unwrap_or("")
            .to_string();

        let children = parse_nav_points(p);
        items.push(NavPoint {
            title,
            href,
            children,
        });
    }
    items
}

/// Render a nested ordered list mirroring the navigation tree.
fn render_ol(items: &[NavPoint], level: usize, out: &mut Vec<String>) {
    let indent = "  ".repeat(level);
    out.push(format!("{}<ol>", indent));
    for item in items {
        let href = nav_href(&item.href);
        out.push(format!("{}  <li>", indent));
        if href.is_empty() {
            out.push(format!(
                "{}    <span>{}</span>",
                indent,
                escape(&item.title)
            ));
        } else {
            out.push(format!(
                "{}    <a href=\"{}\">{}</a>",
                indent,
                escape(&href),
                escape(&item.title)
            ));
        }
        if !item.children.is_empty() {
            render_ol(&item.children, level + 2, out);
        }
        out.push(format!("{}  </li>", indent));
    }
    out.push(format!("{}</ol>", indent));
}

/// An NCX `src` or guide `href` as a valid URL for `nav.xhtml`: forward
/// slashes, and interior spaces percent-encoded (the URL parser already
/// strips leading and trailing ones).
fn nav_href(href: &str) -> String {
    href.trim().replace('\\', "/").replace(' ', "%20")
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Map EPUB 2 guide types to EPUB 3 landmark semantics.
fn map_landmark_type(ref_type: &str) -> String {
    match ref_type {
        "text" => "bodymatter",
        "title-page" => "titlepage",
        "acknowledgements" => "acknowledgments",
        "cover" => "cover",
        "toc" => "toc",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_hrefs_are_escaped() {
        let dir = tempfile::tempdir().unwrap();
        let ncx = dir.path().join("toc.ncx");
        fs::write(
            &ncx,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
<head/><docTitle><text>Tom &amp; Jerry</text></docTitle>
<navMap><navPoint id="n1" playOrder="1"><navLabel><text>Yazar &amp; &lt;Çevirmen&gt;</text></navLabel>
<content src="a.xhtml?x=1&amp;y=2"/></navPoint></navMap></ncx>"#,
        )
        .unwrap();
        let guide = [GuideRef {
            ref_type: "text".into(),
            href: "a.xhtml".into(),
            title: "Start & go".into(),
        }];
        let out = dir.path().join("nav.xhtml");
        generate_nav_xhtml(&ncx, &out, &guide).unwrap();
        let nav = fs::read_to_string(&out).unwrap();
        assert!(nav.contains("<title>Tom &amp; Jerry</title>"));
        assert!(nav.contains(r#"<a href="a.xhtml?x=1&amp;y=2">Yazar &amp; &lt;Çevirmen&gt;</a>"#));
        assert!(nav.contains(">Start &amp; go</a>"));
        crate::util::parse_xml(&strip_doctype(&nav)).unwrap();
    }

    #[test]
    fn anchors_have_labels_valid_urls_and_unique_landmarks() {
        let dir = tempfile::tempdir().unwrap();
        let ncx = dir.path().join("toc.ncx");
        fs::write(
            &ncx,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head/>
<navMap><navPoint id="n1" playOrder="1"><navLabel><text> </text></navLabel>
<content src="a.xhtml"/></navPoint>
<navPoint id="n2" playOrder="2"><navLabel><text>Two</text></navLabel>
<content src=" Gurur Ve_split_004.html#p 1"/></navPoint></navMap></ncx>"#,
        )
        .unwrap();
        let r = |t: &str, h: &str, title: &str| GuideRef {
            ref_type: t.into(),
            href: h.into(),
            title: title.into(),
        };
        let guide = [
            r("cover", "c.xhtml", ""),
            r("text", "a.xhtml", "Text"),
            r("text", "b.xhtml", "Text"),
            r("text", "a.xhtml", "Text"),
        ];
        let out = dir.path().join("nav.xhtml");
        generate_nav_xhtml(&ncx, &out, &guide).unwrap();
        let nav = fs::read_to_string(&out).unwrap();
        assert!(nav.contains(r#"<a href="a.xhtml">Untitled</a>"#));
        assert!(nav.contains(r#"<a href="Gurur%20Ve_split_004.html#p%201">Two</a>"#));
        assert!(nav.contains(r#"<a epub:type="cover" href="c.xhtml">Cover</a>"#));
        assert_eq!(nav.matches(r#"href="a.xhtml">Text</a>"#).count(), 1);
        assert_eq!(nav.matches(r#"href="b.xhtml">Text</a>"#).count(), 1);
    }
}
