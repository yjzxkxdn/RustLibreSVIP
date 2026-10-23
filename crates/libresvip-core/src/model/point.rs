//! Port of `libresvip.model.point`.

use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// x coordinate of the synthetic start point.
pub const START_POINT_X: i64 = -192_000;
/// x coordinate of the synthetic end point.
pub const END_POINT_X: i64 = 1_073_741_823;

/// A single point of a parameter curve.
///
/// Python's `Point` is a `NamedTuple`, so it serialises as a two element JSON
/// array. The Rust type keeps the same wire format while exposing named fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    #[inline]
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// `Point.start_point()` — the synthetic curve start.
    #[inline]
    pub const fn start_point() -> Self {
        Self::start_point_with(-100)
    }

    #[inline]
    pub const fn start_point_with(value: i64) -> Self {
        Self::new(START_POINT_X, value)
    }

    /// `Point.end_point()` — the synthetic curve end.
    #[inline]
    pub const fn end_point() -> Self {
        Self::end_point_with(-100)
    }

    #[inline]
    pub const fn end_point_with(value: i64) -> Self {
        Self::new(END_POINT_X, value)
    }
}

impl From<(i64, i64)> for Point {
    fn from(value: (i64, i64)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<Point> for (i64, i64) {
    fn from(value: Point) -> Self {
        (value.x, value.y)
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl Serialize for Point {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.x, &self.y).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PointVisitor;

        impl<'de> Visitor<'de> for PointVisitor {
            type Value = Point;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a point as [x, y] or {\"x\": .., \"y\": ..}")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Point, A::Error> {
                let x = seq
                    .next_element::<i64>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let y = seq
                    .next_element::<i64>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(Point::new(x, y))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Point, A::Error> {
                let mut x = None;
                let mut y = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "x" | "X" => x = Some(map.next_value::<i64>()?),
                        "y" | "Y" => y = Some(map.next_value::<i64>()?),
                        _ => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Point::new(
                    x.ok_or_else(|| de::Error::missing_field("x"))?,
                    y.ok_or_else(|| de::Error::missing_field("y"))?,
                ))
            }
        }

        deserializer.deserialize_any(PointVisitor)
    }
}
