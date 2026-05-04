use std::collections::{HashSet};
use xmltree::{Element, EmitterConfig, XMLNode};
use regex::Regex;
use lazy_static::lazy_static;

#[derive(Debug, Clone)]
pub struct StripConfig {
    pub remove_metadata: bool,
    pub remove_comments: bool,
    pub remove_hidden: bool,
    pub strip_ids: bool,
    pub remove_empty_groups: bool,
    pub strip_whitespace: bool,
    pub inline_mode: bool,
    pub color_shrink: bool,
    pub decimal_precision: Option<u8>,
}

impl Default for StripConfig {
    fn default() -> Self {
        Self {
            remove_metadata: true,
            remove_comments: true,
            remove_hidden: true,
            strip_ids: true,
            remove_empty_groups: true,
            strip_whitespace: true,
            inline_mode: false,
            color_shrink: true,
            decimal_precision: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct OptimizeStats {
    pub colors_shrunk: bool,
}

pub struct SvgStripper {
    config: StripConfig,
}

impl Default for SvgStripper {
    fn default() -> Self {
        Self::new()
    }
}

impl SvgStripper {
    pub fn new() -> Self {
        Self {
            config: StripConfig::default(),
        }
    }

    pub fn with_config(config: StripConfig) -> Self {
        Self { config }
    }

    /// Parse, optimize, and serialize an SVG string.
    pub fn strip_str(&self, input: &str) -> Result<(String, OptimizeStats), Box<dyn std::error::Error>> {
        let mut root = Element::parse(input.as_bytes())?;
        let stats = self.optimize(&mut root);

        let mut buf = Vec::new();
        let config = EmitterConfig::new()
            .perform_indent(false)
            .write_document_declaration(false);
        root.write_with_config(&mut buf, config)?;
        let result = String::from_utf8(buf)?;
        // Strip any remaining line breaks to the maximum!
        Ok((result.replace(['\n', '\r'], ""), stats))
    }

    fn optimize(&self, root: &mut Element) -> OptimizeStats {
        let mut stats = OptimizeStats::default();
        
        // Strip xmlns and xmlns:* attributes from the <svg> tag as browsers do not need them if inline
        if self.config.inline_mode && root.name == "svg" {
            root.attributes.retain(|k, _| !k.starts_with("xmlns"));
            // xmltree parses xmlns into these specific fields, so we must clear them
            root.namespace = None;
            root.namespaces = None;
        }

        if self.config.remove_metadata {
            remove_metadata(root);
        }
        if self.config.remove_comments {
            remove_comments(root);
        }
        if self.config.remove_hidden {
            remove_hidden_elements(root);
        }
        if self.config.strip_ids {
            strip_unused_ids(root);
        }
        if self.config.remove_empty_groups {
            // Repeat until stable because removing an empty group may
            // cause its parent <g> to become empty as well.
            loop {
                let changed = remove_empty_groups(root);
                if !changed {
                    break;
                }
            }
        }
        if self.config.strip_whitespace {
            strip_whitespace(root);
        }
        if self.config.color_shrink {
            stats.colors_shrunk = optimize_colors(root);
        }
        if let Some(precision) = self.config.decimal_precision {
            optimize_decimals(root, precision);
        }
        
        stats
    }
}

fn remove_metadata(elem: &mut Element) {
    elem.children.retain(|child| {
        if let XMLNode::Element(e) = child {
            !matches!(e.name.as_str(), "title" | "desc" | "metadata")
        } else {
            true
        }
    });
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            remove_metadata(e);
        }
    }
}

fn remove_comments(elem: &mut Element) {
    elem.children.retain(|child| !matches!(child, XMLNode::Comment(_)));
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            remove_comments(e);
        }
    }
}

fn remove_hidden_elements(elem: &mut Element) {
    elem.children.retain(|child| {
        if let XMLNode::Element(e) = child {
            !is_hidden(e)
        } else {
            true
        }
    });
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            remove_hidden_elements(e);
        }
    }
}

fn is_hidden(elem: &Element) -> bool {
    if let Some(v) = elem.attributes.get("display") {
        if v.trim() == "none" {
            return true;
        }
    }
    if let Some(v) = elem.attributes.get("visibility") {
        if v.trim() == "hidden" {
            return true;
        }
    }
    if let Some(v) = elem.attributes.get("opacity") {
        if parse_zero(v) {
            return true;
        }
    }
    if let Some(style) = elem.attributes.get("style") {
        if is_hidden_in_style(style) {
            return true;
        }
    }
    false
}

fn is_hidden_in_style(style: &str) -> bool {
    for decl in style.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        if let Some((prop, val)) = decl.split_once(':') {
            let prop = prop.trim();
            let val = val.trim();
            match prop {
                "display" if val == "none" => return true,
                "visibility" if val == "hidden" => return true,
                "opacity" if parse_zero(val) => return true,
                _ => {}
            }
        }
    }
    false
}

fn parse_zero(s: &str) -> bool {
    s.trim().parse::<f64>().map(|f| f == 0.0).unwrap_or(false)
}

fn strip_unused_ids(root: &mut Element) {
    let mut used = HashSet::new();
    collect_referenced_ids(root, &mut used);
    strip_ids_recursive(root, &used);
}

fn collect_referenced_ids(elem: &Element, used: &mut HashSet<String>) {
    for value in elem.attributes.values() {
        find_ids_in_value(value, used);
    }
    for child in &elem.children {
        if let XMLNode::Element(e) = child {
            collect_referenced_ids(e, used);
        }
    }
}

/// Scans a raw attribute value for `url(#id)` and plain `#id` references.
fn find_ids_in_value(value: &str, used: &mut HashSet<String>) {
    let lower = value.to_lowercase();
    let mut search_from = 0;

    while let Some(idx) = lower[search_from..].find("url(") {
        let abs = search_from + idx;
        let after = &value[abs + 4..];
        let after = after.trim_start();
        let after = if after.starts_with('\'') || after.starts_with('"') {
            &after[1..]
        } else {
            after
        };

        if let Some(after_hash) = after.strip_prefix('#') {
            let end = after_hash
                .find(&[')', '\'', '"', ' ', '\t', '\n', '\r', ';'][..])
                .unwrap_or(after_hash.len());
            let id = &after_hash[..end];
            if !id.is_empty() {
                used.insert(id.to_string());
            }
        }
        search_from = abs + 4;
    }

    // Plain fragment references: href="#id", xlink:href="#id", etc.
    if value.starts_with('#') {
        let id = &value[1..];
        let end = id
            .find(&[' ', '\t', '\n', '\r', '"', '\'', ')', ';'][..])
            .unwrap_or(id.len());
        let id = &id[..end];
        if !id.is_empty() {
            used.insert(id.to_string());
        }
    }
}

fn strip_ids_recursive(elem: &mut Element, used: &HashSet<String>) {
    if let Some(id) = elem.attributes.get("id") {
        if !used.contains(id) {
            elem.attributes.remove("id");
        }
    }
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            strip_ids_recursive(e, used);
        }
    }
}

/* ------------------------------------------------------------------ */
/*  Remove empty <g> wrappers                                         */
/* ------------------------------------------------------------------ */

fn remove_empty_groups(elem: &mut Element) -> bool {
    let mut changed = false;

    // Bottom-up: clean children first.
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            if remove_empty_groups(e) {
                changed = true;
            }
        }
    }

    let before = elem.children.len();
    elem.children.retain(|child| {
        if let XMLNode::Element(e) = child {
            !is_removable_empty_group(e)
        } else {
            true
        }
    });

    if elem.children.len() != before {
        changed = true;
    }
    changed
}

/// A <g> is removable when it has no element children, no non-empty text,
/// and no id attribute (because an id means something may reference it).
fn is_removable_empty_group(elem: &Element) -> bool {
    if elem.name != "g" {
        return false;
    }
    if elem.attributes.contains_key("id") {
        return false;
    }
    for child in &elem.children {
        match child {
            XMLNode::Element(_) => return false,
            XMLNode::Text(t) if !t.trim().is_empty() => return false,
            _ => {}
        }
    }
    true
}

/* ------------------------------------------------------------------ */
/*  Collapse whitespace                                               */
/* ------------------------------------------------------------------ */

/// Elements where text nodes are semantically significant.
const TEXTUAL_ELEMENTS: &[&str] = &["text", "tspan", "textPath", "style", "script"];

fn strip_whitespace(elem: &mut Element) {
    let preserve = elem
        .attributes
        .get("xml:space")
        .map(|s| s == "preserve")
        .unwrap_or(false);

    if !preserve {
        let is_text_parent = TEXTUAL_ELEMENTS.contains(&elem.name.as_str());

        // Drop whitespace-only text nodes unless inside a textual element.
        elem.children.retain(|child| {
            if let XMLNode::Text(t) = child {
                if t.trim().is_empty() && !is_text_parent {
                    return false;
                }
            }
            true
        });

        for child in &mut elem.children {
            match child {
                XMLNode::Text(t) => {
                    if is_text_parent {
                        *t = collapse_whitespace(t);
                    } else {
                        *t = t.trim().to_string();
                    }
                }
                XMLNode::Element(e) => strip_whitespace(e),
                _ => {}
            }
        }
    } else {
        for child in &mut elem.children {
            if let XMLNode::Element(e) = child {
                strip_whitespace(e);
            }
        }
    }
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true; // trim leading

    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }

    if out.ends_with(' ') {
        out.pop(); // trim trailing
    }
    out
}

/* ------------------------------------------------------------------ */
/*  Optimize colors and decimals                                      */
/* ------------------------------------------------------------------ */

const DECIMAL_ATTRIBUTES: &[&str] = &[
    "d", "points", "viewBox", "x", "y", "width", "height", 
    "rx", "ry", "cx", "cy", "r", "x1", "y1", "x2", "y2", 
    "transform", "offset", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "opacity", "stop-opacity"
];
const COLOR_ATTRIBUTES: &[&str] = &[
    "fill", "stroke", "stop-color", "color", "background-color"
];

fn optimize_decimals(elem: &mut Element, precision: u8) {
    lazy_static! {
        static ref RE_NUM: Regex = Regex::new(r"[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?").unwrap();
    }
    
    for (key, val) in &mut elem.attributes {
        if DECIMAL_ATTRIBUTES.contains(&key.as_str()) {
            *val = RE_NUM.replace_all(val, |caps: &regex::Captures| {
                if let Ok(num) = caps[0].parse::<f64>() {
                    let p = 10_f64.powi(precision as i32);
                    let rounded = (num * p).round() / p;
                    format!("{}", rounded)
                } else {
                    caps[0].to_string()
                }
            }).into_owned();
        }
    }
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            optimize_decimals(e, precision);
        }
    }
}

fn optimize_colors(elem: &mut Element) -> bool {
    let mut any_shrunk = false;
    
    for (key, val) in &mut elem.attributes {
        if COLOR_ATTRIBUTES.contains(&key.as_str()) {
            let (new_val, shrunk) = shrink_colors_in_str(val);
            if shrunk {
                *val = new_val.into_owned();
                any_shrunk = true;
            }
        }
    }

    for child in &mut elem.children {
        match child {
            XMLNode::Element(e) => {
                if optimize_colors(e) {
                    any_shrunk = true;
                }
            }
            XMLNode::Text(t) if elem.name == "style" => {
                let (new_val, shrunk) = shrink_colors_in_str(t);
                if shrunk {
                    *t = new_val.into_owned();
                    any_shrunk = true;
                }
            }
            _ => {}
        }
    }
    any_shrunk
}

fn shrink_colors_in_str(s: &str) -> (std::borrow::Cow<'_, str>, bool) {
    lazy_static! {
        static ref RE_COLOR: Regex = Regex::new(r"(?i)#[0-9a-fA-F]{6}\b").unwrap();
    }
    let mut shrunk = false;
    let res = RE_COLOR.replace_all(s, |caps: &regex::Captures| {
        let lower = caps[0].to_ascii_lowercase();
        let b = lower.as_bytes();
        if b[1] == b[2] && b[3] == b[4] && b[5] == b[6] {
            shrunk = true;
            format!("#{}{}{}", lower.chars().nth(1).unwrap(), lower.chars().nth(3).unwrap(), lower.chars().nth(5).unwrap())
        } else {
            lower
        }
    });
    (res, shrunk)
}

/* ------------------------------------------------------------------ */
/*  Tests                                                             */
/* ------------------------------------------------------------------ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pipeline() {
        let input = r#"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <!-- comment -->
  <title>Title</title>
  <desc>Desc</desc>
  <metadata><x /></metadata>
  <g id="unused">
    <rect display="none" width="10" height="10"/>
  </g>
  <g id="used">
    <rect fill="url(#grad1)" width="50" height="50"/>
  </g>
  <linearGradient id="grad1"><stop offset="0%" stop-color="red"/></linearGradient>
  <g id="empty"></g>
</svg>
"#;
        let stripper = SvgStripper::new();
        let out = stripper.strip_str(input).unwrap().0;

        // Metadata, comments, hidden elements, and empty groups gone.
        assert!(!out.contains("comment"));
        assert!(!out.contains("<title>"));
        assert!(!out.contains("<desc>"));
        assert!(!out.contains("<metadata>"));
        assert!(!out.contains(r#"display="none""#));
        assert!(!out.contains(r#"id="unused""#));
        assert!(!out.contains(r#"id="empty""#));
        assert!(!out.contains(r#"id="used""#)); // id stripped because unreferenced

        // Referenced id kept.
        assert!(out.contains(r#"id="grad1""#));
        assert!(out.contains(r#"fill="url(#grad1)""#));
    }

    #[test]
    fn test_optimizations() {
        let input = r##"
<svg viewBox="0 0 100.123 100.987">
  <style>.st0{fill:#FFFFFF;}</style>
  <path d="M 10.12345 20.98765 L 30 40" fill="#FF0000" stroke="#aabbcc"/>
  <rect fill="#123456" stroke="#f1f1f1" />
</svg>
"##;
        let mut config = StripConfig::default();
        config.decimal_precision = Some(2);
        config.color_shrink = true;
        let stripper = SvgStripper::with_config(config);
        let (out, stats) = stripper.strip_str(input).unwrap();

        assert!(stats.colors_shrunk);
        // Check decimal truncation
        assert!(out.contains(r#"viewBox="0 0 100.12 100.99""#));
        assert!(out.contains(r#"d="M 10.12 20.99 L 30 40""#));
        // Check color shrink
        assert!(out.contains(r##"fill="#f00""##));
        assert!(out.contains(r##"stroke="#abc""##));
        assert!(out.contains(r##"{fill:#fff;}"##));
        // Check non-shrinkable colors remain untouched
        assert!(out.contains(r##"fill="#123456""##));
        assert!(out.contains(r##"stroke="#f1f1f1""##));
    }
}