//! Minimal XML tree + serializer used by the Vocaloid XML plugins (`vsqx`,
//! `xvsq`).
//!
//! Python serialises these models with `xsdata`'s `LxmlEventWriter`, i.e. lxml
//! `etree.tostring` after `etree.indent(tree, "\t")`. The observable rules are
//! reproduced here:
//!
//! * every element is written on its own line, one indent per depth,
//! * an element without content is self-closed (`<x/>`),
//! * an element with plain text keeps it inline, escaping `&`, `<` and `>`,
//! * an element with CDATA text keeps it inline as `<![CDATA[...]]>` (a
//!   literal `]]>` is split into `]]]]><![CDATA[>`),
//! * a trailing newline is emitted when pretty printing.
//!
//! Non-pretty output keeps the stream on a single line (the declaration keeps
//! its own newline, matching `XmlWriter.start_document`).

use libresvip_core::error::{LibreSvipError, Result};

#[derive(Clone, Debug)]
pub enum Content {
    Empty,
    Text(String),
    CData(String),
    Children(Vec<Element>),
}

#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub content: Content,
}

impl Element {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Vec::new(),
            content: Content::Empty,
        }
    }

    pub fn attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.push((key.into(), value.into()));
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.content = Content::Text(value.into());
        self
    }

    /// Return a text element, or an empty element when the value is `None`.
    pub fn opt_text(self, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.text(value),
            None => self,
        }
    }

    pub fn cdata(mut self, value: impl Into<String>) -> Self {
        self.content = Content::CData(value.into());
        self
    }

    pub fn child(mut self, child: Element) -> Self {
        self.push(child);
        self
    }

    /// Chaining variant of [`Element::children`].
    pub fn with_children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for child in children {
            self.push(child);
        }
        self
    }

    pub fn children(&mut self, children: impl IntoIterator<Item = Element>) {
        for child in children {
            self.push(child);
        }
    }

    pub fn push(&mut self, child: Element) {
        match &mut self.content {
            Content::Children(children) => children.push(child),
            other => {
                *other = Content::Children(vec![child]);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.content, Content::Empty)
    }
}

/// Serialise `root` exactly like lxml after `etree.indent(..., "\t")`.
pub fn render(root: &Element, declaration: &str, pretty: bool) -> String {
    let mut out = String::new();
    out.push_str(declaration);
    write_element(&mut out, root, 0, pretty);
    if pretty {
        out.push('\n');
    }
    out
}

pub const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
pub const DECLARATION_STANDALONE: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n";

fn write_element(out: &mut String, element: &Element, depth: usize, pretty: bool) {
    if pretty {
        for _ in 0..depth {
            out.push('\t');
        }
    }
    out.push('<');
    out.push_str(&element.name);
    for (key, value) in &element.attrs {
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        escape_attribute(out, value);
        out.push('"');
    }
    match &element.content {
        Content::Empty => out.push_str("/>"),
        Content::Text(value) => {
            out.push('>');
            escape_text(out, value);
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
        Content::CData(value) => {
            out.push_str("><![CDATA[");
            // lxml splits the CDATA terminator so the document stays well-formed.
            out.push_str(&value.replace("]]>", "]]]]><![CDATA[>"));
            out.push_str("]]></");
            out.push_str(&element.name);
            out.push('>');
        }
        Content::Children(children) => {
            out.push('>');
            if pretty {
                out.push('\n');
            }
            for child in children {
                write_element(out, child, depth + 1, pretty);
                if pretty {
                    out.push('\n');
                }
            }
            if pretty {
                for _ in 0..depth {
                    out.push('\t');
                }
            }
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
        }
    }
}

fn escape_text(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            other => out.push(other),
        }
    }
}

fn escape_attribute(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            other => out.push(other),
        }
    }
}

// ---------------------------------------------------------------------------
// Reading helpers (roxmltree)
// ---------------------------------------------------------------------------

pub fn parse_xml(bytes: &[u8]) -> Result<roxmltree::Document<'_>> {
    roxmltree::Document::parse(std::str::from_utf8(bytes).map_err(|error| {
        LibreSvipError::Parse(format!("input is not valid UTF-8: {error}"))
    })?)
    .map_err(|error| LibreSvipError::Parse(format!("invalid XML: {error}")))
}

pub fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

pub fn children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

pub fn child_text<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> Option<String> {
    child(node, name).map(text_of)
}

pub fn text_of(node: roxmltree::Node<'_, '_>) -> String {
    let mut result = String::new();
    for descendant in node.descendants() {
        if descendant.is_text() {
            if let Some(text) = descendant.text() {
                result.push_str(text);
            }
        }
    }
    result
}

/// Element text of a node, without whitespace trimming (Python keeps it).
pub fn node_text(node: roxmltree::Node<'_, '_>) -> String {
    node.children()
        .filter(|child| child.is_text())
        .filter_map(|child| child.text())
        .collect()
}

pub fn parse_int(value: Option<String>) -> Option<i64> {
    value.and_then(|text| text.trim().parse::<i64>().ok())
}

pub fn parse_float(value: Option<String>) -> Option<f64> {
    value.and_then(|text| text.trim().parse::<f64>().ok())
}

pub fn parse_bool(value: Option<String>) -> Option<bool> {
    value.and_then(|text| match text.trim() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    })
}

/// Read an element text, trying each candidate element name in order.
pub fn first_text<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    names: &[&'a str],
) -> Option<String> {
    names.iter().find_map(|name| child_text(node, name))
}

pub fn first_int<'a, 'input>(node: roxmltree::Node<'a, 'input>, names: &[&'a str]) -> Option<i64> {
    parse_int(first_text(node, names))
}

pub fn first_float<'a, 'input>(node: roxmltree::Node<'a, 'input>, names: &[&'a str]) -> Option<f64> {
    parse_float(first_text(node, names))
}

/// Resolve the `id` attribute of a `TypeParamAttr`-like element.
pub fn id_attr(node: roxmltree::Node<'_, '_>) -> Option<String> {
    node.attribute("id").map(str::to_owned)
}
