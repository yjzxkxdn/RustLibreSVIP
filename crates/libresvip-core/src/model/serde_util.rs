//! Small serde helpers mirroring pydantic's permissive (non-strict) coercion.

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;

/// Accepts integers, integral floats and numeric strings.
///
/// Matches pydantic's non-strict `int` coercion exactly: `120.0` becomes `120`,
/// but `1.5` is rejected (`int_from_float`) and so is `"1e3"` (`int_parsing`).
/// Truncating a fractional value instead would silently move notes.
pub fn lenient_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(|_| E::custom("integer out of range"))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
            integral_float(v).ok_or_else(|| {
                E::custom(format!(
                    "invalid type: floating point `{v}`, expected an integer"
                ))
            })
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            parse_python_int(v).ok_or_else(|| {
                E::custom(format!(
                    "invalid type: string `{v}`, expected an integer"
                ))
            })
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<i64, E> {
            Ok(i64::from(v))
        }

        fn visit_unit<E: de::Error>(self) -> Result<i64, E> {
            Ok(0)
        }
    }
    deserializer.deserialize_any(V)
}

/// `Some(value)` when `value` is a finite float with no fractional part.
fn integral_float(value: f64) -> Option<i64> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    // `as i64` saturates, so reject anything that would saturate.
    if value > i64::MAX as f64 || value < i64::MIN as f64 {
        return None;
    }
    Some(value as i64)
}

/// pydantic's string-to-`int` rule (it calls Python's `int(text)`).
///
/// It accepts `"120"`, `" 120 "`, `"1_000"` (single underscores between digits,
/// as Python does) and any number of trailing zeros after the point (`"120.0"`,
/// `"12.000"`), but rejects `"120."`, `".5"`, `"1e3"` and `"2.5"`.
fn parse_python_int(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    let (sign, digits) = match trimmed.as_bytes().first() {
        Some(b'+') => ("", &trimmed[1..]),
        Some(b'-') => ("-", &trimmed[1..]),
        _ => ("", trimmed),
    };
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte == b'0') {
            return None;
        }
    }
    let cleaned = strip_python_underscores(whole)?;
    format!("{sign}{cleaned}").parse::<i64>().ok()
}

/// Python allows `1_000` but not `_1000`, `1000_` or `1__0`.
fn strip_python_underscores(digits: &str) -> Option<String> {
    if digits.is_empty() {
        return None;
    }
    let bytes = digits.as_bytes();
    let mut cleaned = String::with_capacity(digits.len());
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'_' {
            let neighbour_is_digit = index > 0
                && bytes[index - 1].is_ascii_digit()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit);
            if !neighbour_is_digit {
                return None;
            }
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        cleaned.push(*byte as char);
    }
    Some(cleaned)
}

/// Accepts integers, integral floats, numeric strings and `null`.
///
/// The `Option<i64>` counterpart of [`lenient_i64`]: CBOR payloads written by
/// ACE Studio 2.0.7+ encode whole numbers as floats (`"tempoTrackHeight": 120.0`),
/// which pydantic silently coerces and a plain `Option<i64>` rejects.
pub fn lenient_opt_i64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i64>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<i64>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer or null")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            i64::try_from(v)
                .map(Some)
                .map_err(|_| E::custom("integer out of range"))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            integral_float(v)
                .map(Some)
                .ok_or_else(|| {
                    E::custom(format!(
                        "invalid type: floating point `{v}`, expected an integer"
                    ))
                })
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            parse_python_int(v).map(Some).ok_or_else(|| {
                E::custom(format!(
                    "invalid type: string `{v}`, expected an integer"
                ))
            })
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
            Ok(Some(i64::from(v)))
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_any(V)
        }
    }
    deserializer.deserialize_any(V)
}

/// Accepts a list of integers written as floats (`[3.0, 4.0]`).
pub fn lenient_vec_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<i64>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<i64>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a list of integers")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(value) = seq.next_element_seed(LaxInt)? {
                values.push(value);
            }
            Ok(values)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }

    struct LaxInt;
    impl<'de> de::DeserializeSeed<'de> for LaxInt {
        type Value = i64;

        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            lenient_i64(deserializer)
        }
    }

    deserializer.deserialize_seq(V)
}

/// Accepts floats, integers and numeric strings.
pub fn lenient_f64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = f64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a float")
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.trim()
                .parse::<f64>()
                .map_err(|_| E::custom(format!("invalid float: {v}")))
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<f64, E> {
            Ok(if v { 1.0 } else { 0.0 })
        }

        fn visit_unit<E: de::Error>(self) -> Result<f64, E> {
            Ok(0.0)
        }
    }
    deserializer.deserialize_any(V)
}

/// Accepts `[1.0, "2.5", null, ...]` — pydantic's `list[float | None]`.
///
/// ACE Studio writes some parameter curves as arrays of numeric *strings*, and
/// pydantic silently coerces them; a plain `Vec<Option<f64>>` would reject the
/// whole project instead.
pub fn lenient_vec_opt_f64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Option<f64>>, D::Error> {
    struct LaxOptFloat;
    impl<'de> de::DeserializeSeed<'de> for LaxOptFloat {
        type Value = Option<f64>;

        fn deserialize<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Option<f64>;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a float, a numeric string or null")
                }

                fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                    Ok(Some(v))
                }

                fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                    Ok(Some(v as f64))
                }

                fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                    Ok(Some(v as f64))
                }

                fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                    v.trim()
                        .parse::<f64>()
                        .map(Some)
                        .map_err(|_| E::custom(format!("invalid float: {v}")))
                }

                fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                    Ok(Some(if v { 1.0 } else { 0.0 }))
                }

                fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }

                fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }

                fn visit_some<D: Deserializer<'de>>(
                    self,
                    deserializer: D,
                ) -> Result<Self::Value, D::Error> {
                    deserializer.deserialize_any(V)
                }
            }
            deserializer.deserialize_any(V)
        }
    }

    struct SeqVisitor;
    impl<'de> Visitor<'de> for SeqVisitor {
        type Value = Vec<Option<f64>>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a list of floats")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(value) = seq.next_element_seed(LaxOptFloat)? {
                values.push(value);
            }
            Ok(values)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_seq(SeqVisitor)
}

/// Accepts booleans, 0/1 and "true"/"false".
pub fn lenient_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = bool;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a boolean")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<bool, E> {
            Ok(v != 0.0)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<bool, E> {
            match v.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "y" | "on" => Ok(true),
                "false" | "0" | "no" | "n" | "off" => Ok(false),
                other => Err(E::custom(format!("invalid boolean: {other}"))),
            }
        }

        fn visit_unit<E: de::Error>(self) -> Result<bool, E> {
            Ok(false)
        }
    }
    deserializer.deserialize_any(V)
}

/// Accepts strings and stringifies numbers/booleans, mirroring pydantic.
pub fn lenient_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = String;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_owned())
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_unit<E: de::Error>(self) -> Result<String, E> {
            Ok(String::new())
        }
    }
    deserializer.deserialize_any(V)
}

/// Serialises a float the way Python's `json.dumps` does for whole numbers
/// (`1.0` rather than `1`).
pub fn serialize_f64<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(*value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_from_json(text: &str) -> Result<i64, serde_json::Error> {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            #[serde(deserialize_with = "lenient_i64")]
            value: i64,
        }
        serde_json::from_str::<Wrapper>(&format!("{{\"value\":{text}}}")).map(|wrapper| wrapper.value)
    }

    /// pydantic's non-strict int rule, checked against pydantic 2.14 directly.
    #[test]
    fn strings_follow_pydantic() {
        for (text, expected) in [
            ("\"120\"", Some(120)),
            ("\" 120 \"", Some(120)),
            ("\"+120\"", Some(120)),
            ("\"1_000\"", Some(1000)),
            ("\"120.0\"", Some(120)),
            ("\"12.000\"", Some(12)),
            ("\"-0.0\"", Some(0)),
            ("\"120.\"", None),
            ("\".5\"", None),
            ("\"1e3\"", None),
            ("\"2.5\"", None),
            ("\"\"", None),
            ("120.0", Some(120)),
            ("-0.0", Some(0)),
            ("120.5", None),
            ("true", Some(1)),
            ("null", Some(0)),
        ] {
            let got = int_from_json(text).ok();
            assert_eq!(got, expected, "input {text}");
        }
    }

    /// 1e30 saturates through s i64; pydantic rejects it instead.
    #[test]
    fn out_of_range_floats_are_rejected() {
        assert!(int_from_json("1e30").is_err());
        assert!(int_from_json("-1e30").is_err());
    }
}