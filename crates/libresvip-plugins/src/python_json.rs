//! Python-compatible spaced JSON output.
//!
//! `json.dumps` defaults to the separators `", "` and `": "`. serde_json's
//! compact form has no spaces, so formats that byte-match Python output
//! (vpr, vfp) previously serialized compactly and then re-scanned the whole
//! string to insert the spaces. Writing through this `Formatter` instead
//! produces the same bytes in one pass.

use std::io;

use serde::Serialize;

/// serde_json formatter emitting `", "` / `": "` separators like `json.dumps`.
struct PythonSpaced;

impl serde_json::ser::Formatter for PythonSpaced {
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b": ")
    }
}

/// Serializes `value` with Python's `json.dumps` spacing.
pub fn to_spaced_json<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<String> {
    let mut bytes = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut bytes, PythonSpaced);
    serde::Serialize::serialize(value, &mut serializer)?;
    String::from_utf8(bytes).map_err(|error| serde::ser::Error::custom(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct Sample {
        first: &'static str,
        list: Vec<u32>,
        inner: Inner,
    }

    #[derive(serde::Serialize)]
    struct Inner {
        flag: bool,
        nothing: Option<u8>,
    }

    #[test]
    fn matches_python_dumps_spacing() {
        let value = Sample {
            first: "text, with: punctuation",
            list: vec![1, 2, 3],
            inner: Inner {
                flag: true,
                nothing: None,
            },
        };
        let spaced = to_spaced_json(&value).expect("serialize");
        // Exactly what `json.dumps` emits for the equivalent Python object
        // (separators ", " and ": ", no spaces inside empty containers).
        assert_eq!(
            spaced,
            "{\"first\": \"text, with: punctuation\", \"list\": [1, 2, 3], \"inner\": {\"flag\": true, \"nothing\": null}}"
        );
    }
}
