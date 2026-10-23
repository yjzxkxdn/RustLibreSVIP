//! Minimal ordered XML tree + writer that reproduces
//! `xsdata`'s lxml output (`pretty_print=True`, 2-space indent).
//!
//! The lxml writer serialises the whole tree with `etree.indent(tree, "  ")`
//! and `etree.tostring(...)`, then appends a single newline. Empty text nodes
//! are omitted (they serialise as self-closing elements), while non-empty text
//! is escaped like lxml does.

/// An element with a fixed (already-qualified) name, ordered attributes and
/// either text content or children.
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: &'static str,
    pub attrs: Vec<(&'static str, String)>,
    pub text: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn attr(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((key, value.into()));
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// Renders the XML document (declaration, doctype and trailing newline
    /// are added by [`render_document`]).
    pub fn write(&self, output: &mut String, depth: usize) {
        for _ in 0..depth {
            output.push_str("  ");
        }
        output.push('<');
        output.push_str(self.name);
        for (key, value) in &self.attrs {
            output.push(' ');
            output.push_str(key);
            output.push_str("=\"");
            escape_attribute(value, output);
            output.push('"');
        }
        let has_text = self
            .text
            .as_ref()
            .is_some_and(|text| !text.is_empty());
        if has_text {
            output.push('>');
            if let Some(text) = &self.text {
                escape_text(text, output);
            }
            output.push_str("</");
            output.push_str(self.name);
            output.push('>');
        } else if !self.children.is_empty() {
            output.push('>');
            for child in &self.children {
                output.push('\n');
                child.write(output, depth + 1);
            }
            output.push('\n');
            for _ in 0..depth {
                output.push_str("  ");
            }
            output.push_str("</");
            output.push_str(self.name);
            output.push('>');
        } else {
            output.push_str("/>");
        }
    }
}

/// `MusicXMLConverter.dump` output: XML declaration, doctype, tree, newline.
pub fn render_document(root: &Node) -> String {
    let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    output.push_str(
        "<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \
         \"http://www.musicxml.org/dtds/partwise.dtd\">\n",
    );
    root.write(&mut output, 0);
    output.push('\n');
    output
}

fn escape_text(text: &str, output: &mut String) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\r' => output.push_str("&#13;"),
            other => output.push(other),
        }
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\n' => output.push_str("&#10;"),
            '\t' => output.push_str("&#9;"),
            '\r' => output.push_str("&#13;"),
            other => output.push(other),
        }
    }
}

/// Python's `str()` for a float (used for `<per-minute>` and other text
/// fields that the generator fills from a Python float).
pub fn py_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        // Python repr keeps the ".0" suffix for integral floats.
        return format!("{value:.1}");
    }
    let rendered = format!("{value}");
    if let Some(index) = rendered.find(['e', 'E']) {
        let (mantissa, exponent) = rendered.split_at(index);
        let exponent = &exponent[1..];
        let (sign, digits) = match exponent.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("+", exponent),
        };
        return format!("{mantissa}e{sign}{digits:0>2}");
    }
    rendered
}

/// `str(decimal.Decimal.from_float(value))` for the values used by the
/// generator (`divisions` and `sound/@tempo`).
///
/// Python represents the exact binary value and prints the full decimal
/// expansion, e.g. `120.0 -> "120"` and `0.1 -> "0.10000000000000000555..."`.
pub fn decimal_from_float(value: f64) -> String {
    if !value.is_finite() {
        return match value {
            v if v.is_nan() => "NaN".to_owned(),
            v if v > 0.0 => "Infinity".to_owned(),
            v if v < 0.0 => "-Infinity".to_owned(),
            _ => "0".to_owned(),
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0".to_owned()
        } else {
            "0".to_owned()
        };
    }
    let negative = value.is_sign_negative();
    let value = value.abs();
    let bits = value.to_bits();
    let exponent_bits = ((bits >> 52) & 0x7FF) as i32;
    let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
    let (mut mantissa, mut exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        (fraction | 0x0010_0000_0000_0000, exponent_bits - 1075)
    };
    if mantissa == 0 {
        return "0".to_owned();
    }
    // Python's Decimal.from_float reduces the fraction, so values whose
    // binary mantissa is even do not gain trailing decimal zeros.
    while mantissa % 2 == 0 {
        mantissa /= 2;
        exponent += 1;
    }
    // value = mantissa * 2^exponent exactly.
    let mut digits: Vec<u32> = Vec::new();
    push_u64(&mut digits, mantissa);
    if exponent >= 0 {
        for _ in 0..exponent {
            mul_small(&mut digits, 2);
        }
        let mut output = if negative {
            String::from("-")
        } else {
            String::new()
        };
        output.push_str(&digits_to_string(&digits));
        return output;
    }
    // mantissa / 2^k = mantissa * 5^k / 10^k
    let k = (-exponent) as usize;
    for _ in 0..k {
        mul_small(&mut digits, 5);
    }
    let mut rendered = digits_to_string(&digits);
    let mut output = if negative {
        String::from("-")
    } else {
        String::new()
    };
    if rendered.len() <= k {
        output.push_str("0.");
        for _ in 0..(k - rendered.len()) {
            output.push('0');
        }
        output.push_str(&rendered);
    } else {
        let split = rendered.len() - k;
        let fraction = rendered.split_off(split);
        output.push_str(&rendered);
        let trimmed = fraction.trim_end_matches('0');
        if !trimmed.is_empty() {
            output.push('.');
            output.push_str(trimmed);
        }
    }
    output
}

fn push_u64(digits: &mut Vec<u32>, mut value: u64) {
    while value > 0 {
        digits.push((value % 1_000_000_000) as u32);
        value /= 1_000_000_000;
    }
}

fn mul_small(digits: &mut Vec<u32>, factor: u32) {
    let mut carry: u64 = 0;
    for digit in digits.iter_mut() {
        let value = u64::from(*digit) * u64::from(factor) + carry;
        *digit = (value % 1_000_000_000) as u32;
        carry = value / 1_000_000_000;
    }
    while carry > 0 {
        digits.push((carry % 1_000_000_000) as u32);
        carry /= 1_000_000_000;
    }
}

fn digits_to_string(digits: &[u32]) -> String {
    let mut output = String::new();
    for (index, digit) in digits.iter().rev().enumerate() {
        if index == 0 {
            output.push_str(&digit.to_string());
        } else {
            output.push_str(&format!("{digit:09}"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_match_python_from_float() {
        assert_eq!(decimal_from_float(960.0), "960");
        assert_eq!(decimal_from_float(120.0), "120");
        assert_eq!(decimal_from_float(120.5), "120.5");
        assert_eq!(
            decimal_from_float(0.1),
            "0.1000000000000000055511151231257827021181583404541015625"
        );
    }

    #[test]
    fn empty_text_is_self_closing() {
        let node = Node::new("text").text(String::new());
        let mut output = String::new();
        node.write(&mut output, 0);
        assert_eq!(output, "<text/>");
    }
}
