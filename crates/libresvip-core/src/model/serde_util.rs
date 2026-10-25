//! Small serde helpers mirroring pydantic's permissive (non-strict) coercion.

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;

/// Accepts integers, integral floats and numeric strings.
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
            Ok(v as i64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            let trimmed = v.trim();
            if let Ok(parsed) = trimmed.parse::<i64>() {
                return Ok(parsed);
            }
            if let Ok(parsed) = trimmed.parse::<f64>() {
                return Ok(parsed as i64);
            }
            Err(E::custom(format!("invalid integer: {v}")))
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
