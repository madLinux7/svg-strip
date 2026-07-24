use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashSet;
use std::fmt;
use xmltree::{Element, EmitterConfig, XMLNode};

#[derive(Clone)]
pub struct SvgStripError {
    message: String,
}

impl SvgStripError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Debug for SvgStripError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl fmt::Display for SvgStripError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SvgStripError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSize {
    width: String,
    height: String,
}

impl IconSize {
    pub fn parse(value: &str) -> Result<Self, SvgStripError> {
        lazy_static! {
            static ref ICON_SIZE: Regex =
                Regex::new(r"^([0-9]+(?:\.[0-9]+)?)(?:[xX]([0-9]+(?:\.[0-9]+)?))?$").unwrap();
        }

        let captures = ICON_SIZE.captures(value).ok_or_else(|| {
            SvgStripError::new(format!(
                "invalid icon size \"{value}\"; expected SIZE or WIDTHxHEIGHT using positive pixel values"
            ))
        })?;
        let width = normalize_icon_dimension(&captures[1])?;
        let height = captures
            .get(2)
            .map(|height| normalize_icon_dimension(height.as_str()))
            .transpose()?
            .unwrap_or_else(|| width.clone());
        Ok(Self { width, height })
    }

    pub fn width(&self) -> &str {
        &self.width
    }

    pub fn height(&self) -> &str {
        &self.height
    }
}

fn normalize_icon_dimension(value: &str) -> Result<String, SvgStripError> {
    let (integer, fraction) = value
        .split_once('.')
        .map_or((value, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = fraction.map(|fraction| fraction.trim_end_matches('0'));
    let normalized = match fraction {
        Some(fraction) if !fraction.is_empty() => format!("{integer}.{fraction}"),
        _ => integer.to_string(),
    };

    if normalized == "0" {
        return Err(SvgStripError::new(
            "icon dimensions must be greater than zero",
        ));
    }
    Ok(normalized)
}

#[derive(Debug, Clone)]
pub struct StripConfig {
    pub remove_metadata: bool,
    pub remove_comments: bool,
    pub remove_hidden: bool,
    pub strip_ids: bool,
    pub remove_empty_groups: bool,
    pub strip_whitespace: bool,
    pub inline_mode: bool,
    pub component_mode: bool,
    pub icon_size: Option<IconSize>,
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
            component_mode: false,
            icon_size: None,
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
    pub fn strip_str(
        &self,
        input: &str,
    ) -> Result<(String, OptimizeStats), Box<dyn std::error::Error>> {
        let mut root = Element::parse(input.as_bytes())?;
        let stats = self.optimize(&mut root)?;

        let mut buf = Vec::new();
        let config = EmitterConfig::new()
            .perform_indent(false)
            .write_document_declaration(false);
        root.write_with_config(&mut buf, config)?;
        let result = String::from_utf8(buf)?;
        // Strip any remaining line breaks to the maximum!
        Ok((result.replace(['\n', '\r'], ""), stats))
    }

    fn optimize(&self, root: &mut Element) -> Result<OptimizeStats, SvgStripError> {
        let mut stats = OptimizeStats::default();
        let component_mode = self.config.component_mode || self.config.icon_size.is_some();

        if component_mode {
            validate_component_root(root)?;
        }

        // Strip xmlns and xmlns:* attributes from the <svg> tag as browsers do not need them if inline
        if (self.config.inline_mode || component_mode) && root.name == "svg" {
            root.attributes.retain(|k, _| !k.starts_with("xmlns"));
            // xmltree stores parsed namespaces on every element — if we only
            // clear the root, the serializer re-emits xmlns on each child.
            strip_namespaces_recursive(root);
        }

        if self.config.remove_metadata {
            remove_metadata(root);
        }
        if self.config.remove_comments {
            remove_comments(root);
        }
        if component_mode {
            inline_component_styles(root)?;
            clean_component_root(root)?;
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
        if component_mode {
            unwrap_redundant_groups(root);
        }
        if let Some(icon_size) = &self.config.icon_size {
            apply_icon_style(root, icon_size)?;
        }

        Ok(stats)
    }
}

const COMPONENT_PRESENTATION_ATTRIBUTES: &[&str] = &[
    "color",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "clip-rule",
    "opacity",
    "stop-color",
    "stop-opacity",
    "paint-order",
    "vector-effect",
    "shape-rendering",
];

#[derive(Debug)]
struct ComponentStyleRule {
    classes: Vec<String>,
    declarations: Vec<CssDeclaration>,
}

#[derive(Debug, Clone)]
struct CssDeclaration {
    property: String,
    value: String,
}

#[derive(Clone, Copy)]
enum CssDeclarationContext {
    ComponentStylesheet,
    InlineStyle,
}

fn validate_component_root(root: &Element) -> Result<(), SvgStripError> {
    if root.name != "svg" {
        return Err(SvgStripError::new(
            "component mode requires an <svg> root element",
        ));
    }
    if root
        .attributes
        .get("viewBox")
        .is_none_or(|view_box| view_box.trim().is_empty())
    {
        return Err(SvgStripError::new(
            "component mode requires a non-empty viewBox attribute",
        ));
    }
    Ok(())
}

fn inline_component_styles(root: &mut Element) -> Result<(), SvgStripError> {
    let mut rules = Vec::new();
    collect_component_style_rules(root, &mut rules)?;

    for rule in &rules {
        apply_component_style_rule(root, rule);
    }

    let consumed_classes: HashSet<&str> = rules
        .iter()
        .flat_map(|rule| rule.classes.iter().map(String::as_str))
        .collect();
    remove_consumed_class_tokens(root, &consumed_classes);
    remove_style_elements(root);
    Ok(())
}

fn collect_component_style_rules(
    element: &Element,
    rules: &mut Vec<ComponentStyleRule>,
) -> Result<(), SvgStripError> {
    if element.name == "style" {
        rules.extend(parse_component_style_element(element)?);
        return Ok(());
    }

    for child in &element.children {
        if let XMLNode::Element(child_element) = child {
            collect_component_style_rules(child_element, rules)?;
        }
    }
    Ok(())
}

fn parse_component_style_element(
    style: &Element,
) -> Result<Vec<ComponentStyleRule>, SvgStripError> {
    for (name, value) in &style.attributes {
        if name != "type" {
            return Err(SvgStripError::new(format!(
                "component mode does not support the <style> attribute \"{name}\""
            )));
        }
        if !value.trim().eq_ignore_ascii_case("text/css") {
            return Err(SvgStripError::new(format!(
                "component mode only supports <style type=\"text/css\">, found \"{value}\""
            )));
        }
    }

    let mut css = String::new();
    for child in &style.children {
        match child {
            XMLNode::Text(text) | XMLNode::CData(text) => css.push_str(text),
            XMLNode::Comment(_) => {}
            _ => {
                return Err(SvgStripError::new(
                    "component mode only supports text or CDATA inside <style>",
                ));
            }
        }
    }

    parse_component_stylesheet(&strip_css_comments(&css)?)
}

fn strip_css_comments(input: &str) -> Result<String, SvgStripError> {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut quote = None;
    let mut escaped = false;

    while let Some(character) = chars.next() {
        if let Some(active_quote) = quote {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        if character == '\'' || character == '"' {
            quote = Some(character);
            output.push(character);
        } else if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut terminated = false;
            while let Some(comment_character) = chars.next() {
                if comment_character == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    terminated = true;
                    break;
                }
            }
            if !terminated {
                return Err(SvgStripError::new(
                    "component mode found an unterminated CSS comment",
                ));
            }
        } else {
            output.push(character);
        }
    }

    if quote.is_some() {
        return Err(SvgStripError::new(
            "component mode found an unterminated CSS string",
        ));
    }
    Ok(output)
}

fn parse_component_stylesheet(input: &str) -> Result<Vec<ComponentStyleRule>, SvgStripError> {
    lazy_static! {
        static ref SIMPLE_CLASS_SELECTOR: Regex =
            Regex::new(r"^\.-?[_A-Za-z][_A-Za-z0-9-]*$").unwrap();
    }

    let mut remaining = input.trim();
    let mut rules = Vec::new();

    while !remaining.is_empty() {
        let open = remaining.find('{').ok_or_else(|| {
            SvgStripError::new("component mode found CSS outside a complete rule")
        })?;
        let selectors = remaining[..open].trim();
        if selectors.starts_with('@') {
            return Err(SvgStripError::new(format!(
                "component mode does not support CSS at-rule \"{selectors}\""
            )));
        }

        let after_open = &remaining[open + 1..];
        let close = after_open.find('}').ok_or_else(|| {
            SvgStripError::new(format!(
                "component mode found an unterminated CSS rule for \"{selectors}\""
            ))
        })?;
        let declaration_text = &after_open[..close];
        if declaration_text.contains(['{', '}']) {
            return Err(SvgStripError::new(format!(
                "component mode does not support nested CSS in rule \"{selectors}\""
            )));
        }

        let mut classes = Vec::new();
        for selector in selectors.split(',').map(str::trim) {
            if !SIMPLE_CLASS_SELECTOR.is_match(selector) {
                return Err(SvgStripError::new(format!(
                    "component mode does not support selector \"{selector}\"; only simple class selectors are supported"
                )));
            }
            classes.push(selector[1..].to_string());
        }

        let declarations =
            parse_css_declarations(declaration_text, CssDeclarationContext::ComponentStylesheet)?;
        rules.push(ComponentStyleRule {
            classes,
            declarations,
        });
        remaining = after_open[close + 1..].trim();
    }

    Ok(rules)
}

fn parse_css_declarations(
    input: &str,
    context: CssDeclarationContext,
) -> Result<Vec<CssDeclaration>, SvgStripError> {
    lazy_static! {
        static ref IMPORTANT: Regex = Regex::new(r"(?i)!\s*important\b").unwrap();
    }

    let mut declarations = Vec::new();
    for declaration in split_css_at_top_level(input, ';')? {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let colon = find_css_delimiter_at_top_level(declaration, ':')?.ok_or_else(|| {
            SvgStripError::new(format!(
                "component mode found an invalid CSS declaration \"{declaration}\""
            ))
        })?;
        let raw_property = declaration[..colon].trim();
        let normalized_property = raw_property.to_ascii_lowercase();
        let property = if matches!(context, CssDeclarationContext::ComponentStylesheet) {
            normalized_property.clone()
        } else {
            raw_property.to_string()
        };
        let value = declaration[colon + 1..].trim();
        if property.is_empty() || value.is_empty() {
            return Err(SvgStripError::new(format!(
                "component mode found an invalid CSS declaration \"{declaration}\""
            )));
        }
        if matches!(context, CssDeclarationContext::ComponentStylesheet)
            && IMPORTANT.is_match(value)
        {
            return Err(SvgStripError::new(format!(
                "component mode does not support !important on property \"{normalized_property}\""
            )));
        }
        if matches!(context, CssDeclarationContext::ComponentStylesheet)
            && !COMPONENT_PRESENTATION_ATTRIBUTES.contains(&normalized_property.as_str())
        {
            return Err(SvgStripError::new(format!(
                "component mode does not support CSS property \"{normalized_property}\""
            )));
        }
        declarations.push(CssDeclaration {
            property,
            value: value.to_string(),
        });
    }
    Ok(declarations)
}

fn split_css_at_top_level(input: &str, delimiter: char) -> Result<Vec<&str>, SvgStripError> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;

    for (index, character) in input.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => parentheses += 1,
            ')' if parentheses > 0 => parentheses -= 1,
            ')' => {
                return Err(SvgStripError::new(
                    "component mode found unbalanced CSS parentheses",
                ));
            }
            '{' | '}' => {
                return Err(SvgStripError::new(
                    "component mode does not support nested CSS syntax",
                ));
            }
            _ if character == delimiter && parentheses == 0 => {
                parts.push(&input[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    if quote.is_some() {
        return Err(SvgStripError::new(
            "component mode found an unterminated CSS string",
        ));
    }
    if parentheses != 0 {
        return Err(SvgStripError::new(
            "component mode found unbalanced CSS parentheses",
        ));
    }
    parts.push(&input[start..]);
    Ok(parts)
}

fn find_css_delimiter_at_top_level(
    input: &str,
    delimiter: char,
) -> Result<Option<usize>, SvgStripError> {
    let parts = split_css_at_top_level(input, delimiter)?;
    if parts.len() == 1 {
        return Ok(None);
    }
    Ok(Some(parts[0].len()))
}

fn apply_component_style_rule(element: &mut Element, rule: &ComponentStyleRule) {
    let matches = element.attributes.get("class").is_some_and(|classes| {
        classes
            .split_whitespace()
            .any(|class| rule.classes.iter().any(|candidate| candidate == class))
    });

    if matches {
        for declaration in &rule.declarations {
            element
                .attributes
                .insert(declaration.property.clone(), declaration.value.clone());
        }
    }

    for child in &mut element.children {
        if let XMLNode::Element(child_element) = child {
            apply_component_style_rule(child_element, rule);
        }
    }
}

fn remove_consumed_class_tokens(element: &mut Element, consumed_classes: &HashSet<&str>) {
    if let Some(classes) = element.attributes.get("class") {
        let retained: Vec<&str> = classes
            .split_whitespace()
            .filter(|class| !consumed_classes.contains(class))
            .collect();
        if retained.is_empty() {
            element.attributes.remove("class");
        } else {
            element
                .attributes
                .insert("class".to_string(), retained.join(" "));
        }
    }

    for child in &mut element.children {
        if let XMLNode::Element(child_element) = child {
            remove_consumed_class_tokens(child_element, consumed_classes);
        }
    }
}

fn remove_style_elements(element: &mut Element) {
    element.children.retain(
        |child| !matches!(child, XMLNode::Element(child_element) if child_element.name == "style"),
    );
    for child in &mut element.children {
        if let XMLNode::Element(child_element) = child {
            remove_style_elements(child_element);
        }
    }
}

fn clean_component_root(root: &mut Element) -> Result<(), SvgStripError> {
    root.attributes.remove("width");
    root.attributes.remove("height");
    root.attributes.remove("version");

    if root
        .attributes
        .get("fill")
        .is_some_and(|fill| is_default_black(fill))
    {
        root.attributes.remove("fill");
    }

    if root.attributes.contains_key("space") || root.attributes.contains_key("xml:space") {
        if contains_text_elements(root) {
            return Err(SvgStripError::new(
                "component mode cannot safely remove xml:space from an SVG containing text",
            ));
        }
        root.attributes.remove("space");
        root.attributes.remove("xml:space");
    }

    remove_unused_enable_background(root)?;
    Ok(())
}

fn is_default_black(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "black" | "#000" | "#000000"
    )
}

fn contains_text_elements(element: &Element) -> bool {
    element.name == "text"
        || element.name == "tspan"
        || element.name == "textPath"
        || element.children.iter().any(|child| {
            matches!(child, XMLNode::Element(child_element) if contains_text_elements(child_element))
        })
}

fn remove_unused_enable_background(root: &mut Element) -> Result<(), SvgStripError> {
    let Some(style) = root.attributes.get("style") else {
        return Ok(());
    };
    if !style.to_ascii_lowercase().contains("enable-background") {
        return Ok(());
    }

    let mut declarations = parse_inline_style(style)?;
    if !declarations.iter().any(|declaration| {
        declaration
            .property
            .eq_ignore_ascii_case("enable-background")
    }) {
        return Ok(());
    }
    if contains_filter_usage(root) {
        return Err(SvgStripError::new(
            "component mode cannot safely remove enable-background from an SVG that uses filters",
        ));
    }

    declarations.retain(|declaration| {
        !declaration
            .property
            .eq_ignore_ascii_case("enable-background")
    });
    if declarations.is_empty() {
        root.attributes.remove("style");
    } else {
        root.attributes.insert(
            "style".to_string(),
            serialize_css_declarations(&declarations),
        );
    }
    Ok(())
}

fn contains_filter_usage(element: &Element) -> bool {
    element.name == "filter"
        || element.attributes.contains_key("filter")
        || element
            .attributes
            .get("style")
            .is_some_and(|style| style.to_ascii_lowercase().contains("filter"))
        || element.children.iter().any(|child| {
            matches!(child, XMLNode::Element(child_element) if contains_filter_usage(child_element))
        })
}

fn serialize_css_declarations(declarations: &[CssDeclaration]) -> String {
    declarations
        .iter()
        .map(|declaration| format!("{}: {}", declaration.property, declaration.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn parse_inline_style(style: &str) -> Result<Vec<CssDeclaration>, SvgStripError> {
    let without_comments = strip_css_comments(style)?;
    parse_css_declarations(&without_comments, CssDeclarationContext::InlineStyle)
}

fn apply_icon_style(root: &mut Element, icon_size: &IconSize) -> Result<(), SvgStripError> {
    let mut declarations = match root.attributes.get("style") {
        Some(style) => parse_inline_style(style)?,
        None => Vec::new(),
    };
    declarations.retain(|declaration| {
        !matches!(
            declaration.property.to_ascii_lowercase().as_str(),
            "width" | "height" | "overflow" | "fill"
        )
    });
    declarations.extend([
        CssDeclaration {
            property: "width".to_string(),
            value: format!("{}px", icon_size.width()),
        },
        CssDeclaration {
            property: "height".to_string(),
            value: format!("{}px", icon_size.height()),
        },
        CssDeclaration {
            property: "overflow".to_string(),
            value: "hidden".to_string(),
        },
        CssDeclaration {
            property: "fill".to_string(),
            value: "currentColor".to_string(),
        },
    ]);

    root.attributes.remove("width");
    root.attributes.remove("height");
    root.attributes.remove("fill");
    root.attributes.insert(
        "style".to_string(),
        serialize_css_declarations(&declarations),
    );
    Ok(())
}

fn unwrap_redundant_groups(element: &mut Element) {
    for child in &mut element.children {
        if let XMLNode::Element(child_element) = child {
            unwrap_redundant_groups(child_element);
        }
    }

    let mut flattened = Vec::with_capacity(element.children.len());
    for child in std::mem::take(&mut element.children) {
        match child {
            XMLNode::Element(mut child_element) if is_redundant_group(&child_element) => {
                flattened.append(&mut child_element.children);
            }
            other => flattened.push(other),
        }
    }
    element.children = flattened;
}

fn is_redundant_group(element: &Element) -> bool {
    const ANIMATION_ELEMENTS: &[&str] = &["animate", "animateMotion", "animateTransform", "set"];

    element.name == "g"
        && element.attributes.is_empty()
        && !element.children.iter().any(|child| {
            matches!(
                child,
                XMLNode::Element(child_element)
                    if ANIMATION_ELEMENTS.contains(&child_element.name.as_str())
            )
        })
}

/// Recursively remove namespace declarations from all elements.
/// Required for inline SVG mode: when the root <svg> no longer
/// declares xmlns, xmltree would re-emit it on every child.
fn strip_namespaces_recursive(elem: &mut Element) {
    elem.namespace = None;
    elem.namespaces = None;
    for child in &mut elem.children {
        if let XMLNode::Element(e) = child {
            strip_namespaces_recursive(e);
        }
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
    elem.children
        .retain(|child| !matches!(child, XMLNode::Comment(_)));
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
    if let Some(id) = value.strip_prefix('#') {
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
    "d",
    "points",
    "viewBox",
    "x",
    "y",
    "width",
    "height",
    "rx",
    "ry",
    "cx",
    "cy",
    "r",
    "x1",
    "y1",
    "x2",
    "y2",
    "transform",
    "offset",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "stop-opacity",
];
const COLOR_ATTRIBUTES: &[&str] = &["fill", "stroke", "stop-color", "color", "background-color"];

fn optimize_decimals(elem: &mut Element, precision: u8) {
    lazy_static! {
        static ref RE_NUM: Regex = Regex::new(r"[-+]?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?").unwrap();
    }

    for (key, val) in &mut elem.attributes {
        if DECIMAL_ATTRIBUTES.contains(&key.as_str()) {
            *val = RE_NUM
                .replace_all(val, |caps: &regex::Captures| {
                    if let Ok(num) = caps[0].parse::<f64>() {
                        let p = 10_f64.powi(precision as i32);
                        let rounded = (num * p).round() / p;
                        format!("{}", rounded)
                    } else {
                        caps[0].to_string()
                    }
                })
                .into_owned();
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
            format!(
                "#{}{}{}",
                lower.chars().nth(1).unwrap(),
                lower.chars().nth(3).unwrap(),
                lower.chars().nth(5).unwrap()
            )
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
        let config = StripConfig {
            decimal_precision: Some(2),
            ..StripConfig::default()
        };
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

    #[test]
    fn component_mode_produces_component_ready_svg() {
        let input = r##"<?xml version="1.0" encoding="utf-8"?>
<svg xmlns="http://www.w3.org/2000/svg"
     xmlns:xlink="http://www.w3.org/1999/xlink"
     fill="#000000"
     width="800px"
     height="800px"
     viewBox="0 -0.09 122.88 122.88"
     version="1.1"
     style="enable-background:new 0 0 122.88 122.7"
     xml:space="preserve">
  <style type="text/css">.st0{fill-rule:evenodd;clip-rule:evenodd;}</style>
  <g>
    <path class="st0" d="M0.18,0h44.63v44.45H0.18V0z"/>
  </g>
</svg>"##;
        let config = StripConfig {
            component_mode: true,
            ..StripConfig::default()
        };

        let output = SvgStripper::with_config(config).strip_str(input).unwrap().0;
        let root = Element::parse(output.as_bytes()).unwrap();

        assert_eq!(root.name, "svg");
        assert_eq!(
            root.attributes,
            [("viewBox".to_string(), "0 -0.09 122.88 122.88".to_string())]
                .into_iter()
                .collect()
        );

        let element_children: Vec<&Element> = root
            .children
            .iter()
            .filter_map(|child| match child {
                XMLNode::Element(element) => Some(element),
                _ => None,
            })
            .collect();
        assert_eq!(element_children.len(), 1);

        let path = element_children[0];
        assert_eq!(path.name, "path");
        assert_eq!(
            path.attributes.get("fill-rule").map(String::as_str),
            Some("evenodd")
        );
        assert_eq!(
            path.attributes.get("clip-rule").map(String::as_str),
            Some("evenodd")
        );
        assert!(!path.attributes.contains_key("class"));
    }

    #[test]
    fn component_mode_rejects_unsupported_stylesheet_rules() {
        let cases = [
            (
                r#"<svg viewBox="0 0 10 10"><style>path.st0 { fill: red; }</style><path class="st0"/></svg>"#,
                r#"does not support selector "path.st0""#,
            ),
            (
                r#"<svg viewBox="0 0 10 10"><style>.st0 { fill: red !important; }</style><path class="st0"/></svg>"#,
                r#"does not support !important on property "fill""#,
            ),
            (
                r#"<svg viewBox="0 0 10 10"><style>.st0 { transform: rotate(10deg); }</style><path class="st0"/></svg>"#,
                r#"does not support CSS property "transform""#,
            ),
        ];
        let config = StripConfig {
            component_mode: true,
            ..StripConfig::default()
        };
        let stripper = SvgStripper::with_config(config);

        for (input, expected_error) in cases {
            let error = stripper.strip_str(input).unwrap_err().to_string();
            assert!(
                error.contains(expected_error),
                "expected {error:?} to contain {expected_error:?}"
            );
        }
    }

    #[test]
    fn component_mode_preserves_basic_cascade_and_unrelated_classes() {
        let input = r##"<svg viewBox="0 0 10 10">
            <style><![CDATA[
                /* Exported rules */
                .first, .second { fill: #FFFFFF; }
                .second { fill: #000000; stroke-width: 2; }
            ]]></style>
            <path class="external first second" fill="red" style="fill: blue"/>
        </svg>"##;
        let config = StripConfig {
            component_mode: true,
            ..StripConfig::default()
        };

        let output = SvgStripper::with_config(config).strip_str(input).unwrap().0;
        let root = Element::parse(output.as_bytes()).unwrap();
        let path = root.get_child("path").unwrap();

        assert_eq!(
            path.attributes.get("class").map(String::as_str),
            Some("external")
        );
        assert_eq!(
            path.attributes.get("fill").map(String::as_str),
            Some("#000")
        );
        assert_eq!(
            path.attributes.get("stroke-width").map(String::as_str),
            Some("2")
        );
        assert_eq!(
            path.attributes.get("style").map(String::as_str),
            Some("fill: blue")
        );
    }

    #[test]
    fn component_mode_inlines_references_before_stripping_unused_ids() {
        let input = r##"<svg viewBox="0 0 10 10">
            <style>.paint { fill: url(#gradient); }</style>
            <defs><linearGradient id="gradient"/></defs>
            <path class="paint"/>
        </svg>"##;
        let config = StripConfig {
            component_mode: true,
            ..StripConfig::default()
        };

        let output = SvgStripper::with_config(config).strip_str(input).unwrap().0;

        assert!(output.contains(r#"fill="url(#gradient)""#));
        assert!(output.contains(r#"id="gradient""#));
    }

    #[test]
    fn component_mode_requires_view_box_and_protects_filter_semantics() {
        let cases = [
            (
                r#"<svg width="10" height="10"><path/></svg>"#,
                "requires a non-empty viewBox",
            ),
            (
                r#"<svg viewBox="0 0 10 10" style="enable-background:new 0 0 10 10"><filter id="blur"/></svg>"#,
                "cannot safely remove enable-background",
            ),
        ];
        let config = StripConfig {
            component_mode: true,
            ..StripConfig::default()
        };
        let stripper = SvgStripper::with_config(config);

        for (input, expected_error) in cases {
            let error = stripper.strip_str(input).unwrap_err().to_string();
            assert!(
                error.contains(expected_error),
                "expected {error:?} to contain {expected_error:?}"
            );
        }
    }

    #[test]
    fn inline_mode_keeps_styles_dimensions_and_groups() {
        let input = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
            <style>.st0 { fill: red; }</style>
            <g><path class="st0"/></g>
        </svg>"#;
        let config = StripConfig {
            inline_mode: true,
            ..StripConfig::default()
        };

        let output = SvgStripper::with_config(config).strip_str(input).unwrap().0;

        assert!(!output.contains("xmlns"));
        assert!(output.contains(r#"width="10""#));
        assert!(output.contains(r#"height="10""#));
        assert!(output.contains("<style>"));
        assert!(output.contains(r#"class="st0""#));
        assert!(output.contains("<g>"));
    }

    #[test]
    fn icon_mode_implies_component_mode_and_sets_root_style() {
        let input = r#"<svg xmlns="http://www.w3.org/2000/svg"
                            viewBox="0 0 24 24"
                            width="800"
                            height="800"
                            fill="blue"
                            style="stroke: red; width: 12px; /* exporter */ fill: green !important">
                         <g><path d="M0 0h24v24z"/></g>
                       </svg>"#;
        let config = StripConfig {
            icon_size: Some(IconSize::parse("20x20").unwrap()),
            ..StripConfig::default()
        };

        let output = SvgStripper::with_config(config).strip_str(input).unwrap().0;
        let root = Element::parse(output.as_bytes()).unwrap();

        assert!(!root.attributes.contains_key("xmlns"));
        assert!(!root.attributes.contains_key("width"));
        assert!(!root.attributes.contains_key("height"));
        assert!(!root.attributes.contains_key("fill"));
        assert_eq!(
            root.attributes.get("style").map(String::as_str),
            Some("stroke: red; width: 20px; height: 20px; overflow: hidden; fill: currentColor")
        );
        assert!(matches!(
            root.children.as_slice(),
            [XMLNode::Element(element)] if element.name == "path"
        ));
    }

    #[test]
    fn icon_size_accepts_positive_pixels_and_normalizes_them() {
        let square = IconSize::parse("20").unwrap();
        assert_eq!(square.width(), "20");
        assert_eq!(square.height(), "20");

        let size = IconSize::parse("020.500X16.00").unwrap();
        assert_eq!(size.width(), "20.5");
        assert_eq!(size.height(), "16");

        for invalid in ["20px", "20pxx20px", "0", "0x20", "-1x20", "20 x 20"] {
            assert!(IconSize::parse(invalid).is_err(), "{invalid} should fail");
        }
    }
}
