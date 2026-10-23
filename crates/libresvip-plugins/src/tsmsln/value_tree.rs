//! Port of `libresvip.utils.binary.value_tree`.
//!
//! VoiSona Mobile project files are JUCE `ValueTree` dumps. The binary layout is:
//!
//! ```text
//! node   = cstring name, compressed-int attr count,
//!          attr*  (cstring name, variant),
//!          compressed-int child count, node*
//! variant = compressed-int body length,
//!           [ type byte, value ]
//! ```
//!
//! `compressed-int` is a single width byte followed by that many little-endian
//! bytes carrying the unsigned value. The variant body is optional: a zero
//! length means "no value" (`None` in Python).

use libresvip_core::error::{LibreSvipError, Result};

/// Maximum nesting depth accepted while parsing. Python hits its interpreter
/// recursion limit on pathological files; Rust would overflow the stack, so a
/// bounded depth turns that into an error instead.
const MAX_DEPTH: usize = 256;

const TYPE_INT: u8 = 1;
const TYPE_BOOL_TRUE: u8 = 2;
const TYPE_BOOL_FALSE: u8 = 3;
const TYPE_DOUBLE: u8 = 4;
const TYPE_STRING: u8 = 5;
const TYPE_INT64: u8 = 6;
const TYPE_ARRAY: u8 = 7;
const TYPE_BINARY: u8 = 8;

/// One value inside a [`Node`] attribute (`value_tree.Variant`).
#[derive(Clone, Debug, PartialEq)]
pub enum Variant {
    Bool(bool),
    Int(i32),
    Int64(i64),
    Double(f64),
    Str(String),
    Array(Vec<Variant>),
    Binary(Vec<u8>),
    /// `Optional` produced no value (an empty variant body).
    None,
    /// The `UNDEFINED` variant type: present in the tree, but without a value.
    Undefined,
}

/// A named attribute ("NamedVariant").
#[derive(Clone, Debug, PartialEq)]
pub struct NamedVariant {
    pub name: String,
    pub value: Variant,
}

/// A JUCE `ValueTree` node (`JUCENode`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<NamedVariant>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn with_attr(mut self, name: impl Into<String>, value: Variant) -> Self {
        self.attrs.push(NamedVariant {
            name: name.into(),
            value,
        });
        self
    }

    pub fn push_attr(&mut self, name: impl Into<String>, value: Variant) {
        self.attrs.push(NamedVariant {
            name: name.into(),
            value,
        });
    }

    pub fn push_child(&mut self, child: Node) {
        self.children.push(child);
    }

    pub fn set_child(&mut self, child: Node) {
        self.children.push(child);
    }

    /// Attribute lookup. Later duplicates win; empty variants are skipped, like
    /// `build_tree_dict`'s `if attr.data is not None`.
    pub fn attr(&self, name: &str) -> Option<&Variant> {
        self.attrs
            .iter()
            .rev()
            .filter(|attr| !matches!(attr.value, Variant::None))
            .find(|attr| attr.name == name)
            .map(|attr| &attr.value)
    }

    /// Children whose node name matches, in file order.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |child| child.name == name)
    }

    pub fn children_named_vec<'a>(&'a self, name: &str) -> Vec<&'a Node> {
        self.children
            .iter()
            .filter(|child| child.name == name)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn read_u8(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.pos)
            .ok_or_else(|| LibreSvipError::Parse("unexpected end of value tree".to_owned()))?;
        self.pos += 1;
        Ok(byte)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| LibreSvipError::Parse("value tree length overflow".to_owned()))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| LibreSvipError::Parse("unexpected end of value tree".to_owned()))?;
        self.pos = end;
        Ok(slice)
    }

    /// `JUCECompressedInt._parse`.
    fn read_compressed(&mut self) -> Result<u64> {
        let width = self.read_u8()? as usize;
        let bytes = self.read_bytes(width)?;
        if width > 8 {
            return Err(LibreSvipError::Parse(
                "value tree integer is too large".to_owned(),
            ));
        }
        let mut value = 0u64;
        for (index, byte) in bytes.iter().enumerate() {
            value |= u64::from(*byte) << (index * 8);
        }
        Ok(value)
    }

    fn read_cstring(&mut self) -> Result<String> {
        let start = self.pos;
        let end = self.data[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| start + offset)
            .ok_or_else(|| LibreSvipError::Parse("unterminated value tree string".to_owned()))?;
        let text = std::str::from_utf8(&self.data[start..end])
            .map_err(|error| LibreSvipError::Parse(format!("invalid UTF-8 string: {error}")))?
            .to_owned();
        self.pos = end + 1;
        Ok(text)
    }
}

/// `JUCENode.parse`.
pub fn parse(bytes: &[u8]) -> Result<Node> {
    let mut reader = Reader::new(bytes);
    read_node(&mut reader, 0)
}

fn read_node(reader: &mut Reader<'_>, depth: usize) -> Result<Node> {
    if depth > MAX_DEPTH {
        return Err(LibreSvipError::Parse(
            "value tree is nested too deeply".to_owned(),
        ));
    }
    let name = reader.read_cstring()?;
    let attr_count = usize::try_from(reader.read_compressed()?)
        .map_err(|_| LibreSvipError::Parse("invalid attribute count".to_owned()))?;
    let mut attrs = Vec::with_capacity(attr_count.min(1024));
    for _ in 0..attr_count {
        let attr_name = reader.read_cstring()?;
        let value = read_variant(reader, depth)?;
        attrs.push(NamedVariant {
            name: attr_name,
            value,
        });
    }
    let child_count = usize::try_from(reader.read_compressed()?)
        .map_err(|_| LibreSvipError::Parse("invalid child count".to_owned()))?;
    let mut children = Vec::with_capacity(child_count.min(1024));
    for _ in 0..child_count {
        children.push(read_node(reader, depth + 1)?);
    }
    Ok(Node {
        name,
        attrs,
        children,
    })
}

fn read_variant(reader: &mut Reader<'_>, depth: usize) -> Result<Variant> {
    let length = usize::try_from(reader.read_compressed()?)
        .map_err(|_| LibreSvipError::Parse("invalid variant length".to_owned()))?;
    let body = reader.read_bytes(length)?;
    let mut inner = Reader::new(body);
    if inner.is_empty() {
        return Ok(Variant::None);
    }
    let type_byte = inner.read_u8()?;
    match type_byte {
        TYPE_INT => {
            let bytes = inner.read_bytes(4)?;
            Ok(Variant::Int(i32::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ])))
        }
        TYPE_BOOL_TRUE => Ok(Variant::Bool(true)),
        TYPE_BOOL_FALSE => Ok(Variant::Bool(false)),
        TYPE_DOUBLE => {
            let bytes = inner.read_bytes(8)?;
            let mut array = [0u8; 8];
            array.copy_from_slice(bytes);
            Ok(Variant::Double(f64::from_le_bytes(array)))
        }
        TYPE_STRING => Ok(Variant::Str(inner.read_cstring()?)),
        TYPE_INT64 => {
            let bytes = inner.read_bytes(8)?;
            let mut array = [0u8; 8];
            array.copy_from_slice(bytes);
            Ok(Variant::Int64(i64::from_le_bytes(array)))
        }
        TYPE_ARRAY => {
            if depth > MAX_DEPTH {
                return Err(LibreSvipError::Parse(
                    "value tree is nested too deeply".to_owned(),
                ));
            }
            let count = usize::try_from(inner.read_compressed()?)
                .map_err(|_| LibreSvipError::Parse("invalid array length".to_owned()))?;
            let mut items = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                items.push(read_variant(&mut inner, depth + 1)?);
            }
            Ok(Variant::Array(items))
        }
        TYPE_BINARY => Ok(Variant::Binary(inner.data[inner.pos..].to_vec())),
        // `Switch` has no case for `UNDEFINED`, so it yields `None`.
        9 => Ok(Variant::Undefined),
        // `Switch`/`Enum` raise on any other type byte.
        other => Err(LibreSvipError::Parse(format!(
            "unknown value tree variant type {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// `JUCENode.build`.
pub fn build(node: &Node) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_node(node, &mut out, 0)?;
    Ok(out)
}

fn write_node(node: &Node, out: &mut Vec<u8>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(LibreSvipError::Parse(
            "value tree is nested too deeply".to_owned(),
        ));
    }
    out.extend_from_slice(node.name.as_bytes());
    out.push(0);
    write_compressed(out, node.attrs.len() as u64);
    for attr in &node.attrs {
        out.extend_from_slice(attr.name.as_bytes());
        out.push(0);
        write_variant(&attr.value, out)?;
    }
    write_compressed(out, node.children.len() as u64);
    for child in &node.children {
        write_node(child, out, depth + 1)?;
    }
    Ok(())
}

fn write_compressed(out: &mut Vec<u8>, value: u64) {
    let bits = 64 - value.leading_zeros();
    let width = (((bits + 7) / 8) as usize).max(1);
    out.push(width as u8);
    out.extend_from_slice(&value.to_le_bytes()[..width]);
}

fn write_compressed_len(out: &mut Vec<u8>, value: usize) -> Result<()> {
    let value = u64::try_from(value)
        .map_err(|_| LibreSvipError::Parse("value tree is too large".to_owned()))?;
    write_compressed(out, value);
    Ok(())
}

fn write_variant(value: &Variant, out: &mut Vec<u8>) -> Result<()> {
    let mut body = Vec::new();
    match value {
        Variant::None => {}
        Variant::Bool(true) => body.push(TYPE_BOOL_TRUE),
        Variant::Bool(false) => body.push(TYPE_BOOL_FALSE),
        Variant::Int(number) => {
            body.push(TYPE_INT);
            body.extend_from_slice(&number.to_le_bytes());
        }
        Variant::Int64(number) => {
            body.push(TYPE_INT64);
            body.extend_from_slice(&number.to_le_bytes());
        }
        Variant::Double(number) => {
            body.push(TYPE_DOUBLE);
            body.extend_from_slice(&number.to_le_bytes());
        }
        Variant::Str(text) => {
            body.push(TYPE_STRING);
            body.extend_from_slice(text.as_bytes());
            body.push(0);
        }
        Variant::Array(items) => {
            body.push(TYPE_ARRAY);
            write_compressed_len(&mut body, items.len())?;
            for item in items {
                write_variant(item, &mut body)?;
            }
        }
        Variant::Binary(bytes) => {
            body.push(TYPE_BINARY);
            body.extend_from_slice(bytes);
        }
        Variant::Undefined => body.push(9),
    }
    write_compressed_len(out, body.len())?;
    out.extend_from_slice(&body);
    Ok(())
}

// ---------------------------------------------------------------------------
// Attribute helpers
// ---------------------------------------------------------------------------

pub fn attr_str(node: &Node, name: &str) -> Option<String> {
    match node.attr(name)? {
        Variant::Str(text) => Some(text.clone()),
        _ => None,
    }
}

pub fn attr_string_or(node: &Node, name: &str, default: &str) -> String {
    attr_str(node, name).unwrap_or_else(|| default.to_owned())
}

pub fn attr_i64(node: &Node, name: &str) -> Option<i64> {
    match node.attr(name)? {
        Variant::Int(value) => Some(i64::from(*value)),
        Variant::Int64(value) => Some(*value),
        Variant::Double(value) if value.fract() == 0.0 => Some(*value as i64),
        Variant::Bool(value) => Some(i64::from(*value)),
        Variant::Str(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub fn attr_f64(node: &Node, name: &str) -> Option<f64> {
    match node.attr(name)? {
        Variant::Double(value) => Some(*value),
        Variant::Int(value) => Some(f64::from(*value)),
        Variant::Int64(value) => Some(*value as f64),
        Variant::Bool(value) => Some(f64::from(u8::from(*value))),
        Variant::Str(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub fn attr_bool(node: &Node, name: &str) -> Option<bool> {
    match node.attr(name)? {
        Variant::Bool(value) => Some(*value),
        Variant::Int(value) => Some(*value != 0),
        Variant::Int64(value) => Some(*value != 0),
        Variant::Double(value) => Some(*value != 0.0),
        Variant::Str(text) => Some(matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "y" | "on"
        )),
        _ => None,
    }
}

pub fn required_i64(node: &Node, name: &str, struct_name: &str) -> Result<i64> {
    attr_i64(node, name).ok_or_else(|| {
        LibreSvipError::Validation(format!("{struct_name}: missing integer attribute {name}"))
    })
}

pub fn required_f64(node: &Node, name: &str, struct_name: &str) -> Result<f64> {
    attr_f64(node, name).ok_or_else(|| {
        LibreSvipError::Validation(format!("{struct_name}: missing float attribute {name}"))
    })
}

pub fn required_string(node: &Node, name: &str, struct_name: &str) -> Result<String> {
    attr_str(node, name).ok_or_else(|| {
        LibreSvipError::Validation(format!("{struct_name}: missing string attribute {name}"))
    })
}
