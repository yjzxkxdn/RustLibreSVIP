//! Port of the subset of `pydantic_extra_types.color.Color` used by the SVG
//! plugin (`Color.as_hex()` plus validation).

use libresvip_core::error::{LibreSvipError, Result};

/// RGBA color with channels in the `0.0..=1.0` range, matching pydantic's
/// internal `RGBA` representation (alpha `None` when fully opaque).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub alpha: Option<f64>,
}

impl Color {
    fn from_ints(r: u8, g: u8, b: u8, alpha: Option<f64>) -> Self {
        Self {
            r: f64::from(r) / 255.0,
            g: f64::from(g) / 255.0,
            b: f64::from(b) / 255.0,
            alpha,
        }
    }

    /// `Color(value)` — parses a CSS color string.
    pub fn parse(value: &str) -> Result<Self> {
        let lower = value.to_lowercase();
        if let Some(rgb) = named_color(&lower) {
            return Ok(Self::from_ints(rgb.0, rgb.1, rgb.2, None));
        }
        if lower == "transparent" {
            return Ok(Self {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                alpha: Some(0.0),
            });
        }
        if let Some(color) = parse_hex(&lower)? {
            return Ok(color);
        }
        if let Some(color) = parse_rgb(&lower)? {
            return Ok(color);
        }
        if let Some(color) = parse_hsl(&lower)? {
            return Ok(color);
        }
        Err(LibreSvipError::Validation(
            "value is not a valid color: string not recognised as a valid color".to_owned(),
        ))
    }

    /// `Color.as_hex()` with the default `short` format.
    pub fn as_hex(&self) -> String {
        // `float_to_255` = Python `round(c * 255)` (ties to even).
        let values = [
            (self.r * 255.0).round_ties_even() as i64,
            (self.g * 255.0).round_ties_even() as i64,
            (self.b * 255.0).round_ties_even() as i64,
        ];
        let mut channels: Vec<i64> = values.to_vec();
        if let Some(alpha) = self.alpha {
            channels.push((alpha * 255.0).round_ties_even() as i64);
        }
        let long: String = channels.iter().map(|value| format!("{value:02x}")).collect();
        let repeated = channels
            .iter()
            .all(|value| (0..=255).step_by(17).any(|repeat| repeat == *value));
        if repeated {
            let short: String = long
                .chars()
                .step_by(2)
                .take(channels.len())
                .collect();
            format!("#{short}")
        } else {
            format!("#{long}")
        }
    }
}

/// `parse_color_value` — float in `0..=max` scaled to `0..=1`.
fn parse_color_value(text: &str, max: f64) -> Result<f64> {
    let value: f64 = text.trim().parse().map_err(|_| {
        LibreSvipError::Validation(
            "value is not a valid color: color values must be a valid number".to_owned(),
        )
    })?;
    if (0.0..=max).contains(&value) {
        Ok(value / max)
    } else {
        Err(LibreSvipError::Validation(format!(
            "value is not a valid color: color values must be in the range 0 to {max}"
        )))
    }
}

/// `parse_float_alpha` — `None` for a fully-opaque alpha.
fn parse_float_alpha(value: Option<&str>) -> Result<Option<f64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let text = value.trim();
    let alpha: f64 = if let Some(percent) = text.strip_suffix('%') {
        percent.trim().parse::<f64>().map_err(|_| {
            LibreSvipError::Validation(
                "value is not a valid color: alpha values must be a valid float".to_owned(),
            )
        })? / 100.0
    } else {
        text.parse::<f64>().map_err(|_| {
            LibreSvipError::Validation(
                "value is not a valid color: alpha values must be a valid float".to_owned(),
            )
        })?
    };
    normalize_alpha(alpha)
}

fn normalize_alpha(alpha: f64) -> Result<Option<f64>> {
    // `math.isclose(alpha, 1)` with the default relative tolerance.
    if (alpha - 1.0).abs() <= 1e-9 * alpha.abs().max(1.0) {
        return Ok(None);
    }
    if (0.0..=1.0).contains(&alpha) {
        Ok(Some(alpha))
    } else {
        Err(LibreSvipError::Validation(
            "value is not a valid color: alpha values must be in the range 0 to 1".to_owned(),
        ))
    }
}

fn is_hex_digit(character: char) -> bool {
    character.is_ascii_hexdigit()
}

/// `r_hex_short` / `r_hex_long` with their optional `#`/`0x` prefix and
/// surrounding whitespace.
fn parse_hex(value: &str) -> Result<Option<Color>> {
    let trimmed = value.trim();
    let digits = if let Some(rest) = trimmed.strip_prefix('#') {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("0x") {
        rest
    } else {
        trimmed
    };
    if !digits.chars().all(is_hex_digit) {
        return Ok(None);
    }
    match digits.len() {
        3 | 4 => {
            let mut nibbles = digits.chars().map(|character| {
                character.to_digit(16).unwrap_or(0) as u8 * 17
            });
            let r = nibbles.next().unwrap_or(0);
            let g = nibbles.next().unwrap_or(0);
            let b = nibbles.next().unwrap_or(0);
            let alpha = nibbles
                .next()
                .map(|a| normalize_alpha(f64::from(a) / 255.0))
                .transpose()?
                .flatten();
            Ok(Some(Color::from_ints(r, g, b, alpha)))
        }
        6 | 8 => {
            let byte = |offset: usize| -> Option<u8> {
                digits
                    .get(offset..offset + 2)
                    .and_then(|part| u8::from_str_radix(part, 16).ok())
            };
            let Some(r) = byte(0) else { return Ok(None) };
            let Some(g) = byte(2) else { return Ok(None) };
            let Some(b) = byte(4) else { return Ok(None) };
            let alpha = if digits.len() == 8 {
                byte(6)
                    .map(|a| normalize_alpha(f64::from(a) / 255.0))
                    .transpose()?
                    .flatten()
            } else {
                None
            };
            Ok(Some(Color::from_ints(r, g, b, alpha)))
        }
        _ => Ok(None),
    }
}

/// Splits the argument list of `rgb(...)`/`rgba(...)`/`hsl(...)`/`hsla(...)`.
fn function_arguments(value: &str, names: &[&str]) -> Option<String> {
    let value = value.trim();
    for name in names {
        if let Some(rest) = value.strip_prefix(name) {
            let rest = rest.trim_start();
            if let Some(inner) = rest.strip_prefix('(').and_then(|rest| rest.strip_suffix(')')) {
                return Some(inner.trim().to_owned());
            }
        }
    }
    None
}

fn split_components(inner: &str) -> Vec<String> {
    let normalized = inner.replace('/', " ");
    if normalized.contains(',') {
        normalized
            .split(',')
            .map(|part| part.trim().to_owned())
            .filter(|part| !part.is_empty())
            .collect()
    } else {
        normalized
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }
}

fn parse_rgb(value: &str) -> Result<Option<Color>> {
    let Some(inner) = function_arguments(value, &["rgba", "rgb"]) else {
        return Ok(None);
    };
    let components = split_components(&inner);
    if components.len() != 3 && components.len() != 4 {
        return Ok(None);
    }
    let r = parse_color_value(&components[0], 255.0)?;
    let g = parse_color_value(&components[1], 255.0)?;
    let b = parse_color_value(&components[2], 255.0)?;
    let alpha = parse_float_alpha(components.get(3).map(String::as_str))?;
    Ok(Some(Color { r, g, b, alpha }))
}

fn parse_hsl(value: &str) -> Result<Option<Color>> {
    let Some(inner) = function_arguments(value, &["hsla", "hsl"]) else {
        return Ok(None);
    };
    let components = split_components(&inner);
    if components.len() != 3 && components.len() != 4 {
        return Ok(None);
    }
    let hue_text = components[0].to_lowercase();
    let (hue_text, unit) = if let Some(rest) = hue_text.strip_suffix("deg") {
        (rest.to_owned(), "deg")
    } else if let Some(rest) = hue_text.strip_suffix("rad") {
        (rest.to_owned(), "rad")
    } else if let Some(rest) = hue_text.strip_suffix("turn") {
        (rest.to_owned(), "turn")
    } else {
        (hue_text, "deg")
    };
    let hue: f64 = hue_text.trim().parse().map_err(|_| {
        LibreSvipError::Validation(
            "value is not a valid color: color values must be a valid number".to_owned(),
        )
    })?;
    let saturation = parse_color_value(components[1].trim_end_matches('%'), 100.0)?;
    let lightness = parse_color_value(components[2].trim_end_matches('%'), 100.0)?;
    let hue = match unit {
        "rad" => (hue % (2.0 * std::f64::consts::PI)) / (2.0 * std::f64::consts::PI),
        "turn" => hue % 1.0,
        _ => (hue % 360.0) / 360.0,
    };
    let (r, g, b) = hls_to_rgb(hue, lightness, saturation);
    let alpha = parse_float_alpha(components.get(3).map(String::as_str))?;
    Ok(Some(Color {
        r,
        g,
        b,
        alpha,
    }))
}

/// `colorsys.hls_to_rgb`.
fn hls_to_rgb(h: f64, l: f64, s: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let m2 = if l <= 0.5 {
        l * (1.0 + s)
    } else {
        l + s - (l * s)
    };
    let m1 = 2.0 * l - m2;
    let hue_to_rgb = |mut hue: f64| -> f64 {
        if hue < 0.0 {
            hue += 1.0;
        }
        if hue > 1.0 {
            hue -= 1.0;
        }
        if hue < 1.0 / 6.0 {
            return m1 + (m2 - m1) * hue * 6.0;
        }
        if hue < 1.0 / 2.0 {
            return m2;
        }
        if hue < 2.0 / 3.0 {
            return m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0;
        }
        m1
    };
    (
        hue_to_rgb(h + 1.0 / 3.0),
        hue_to_rgb(h),
        hue_to_rgb(h - 1.0 / 3.0),
    )
}

fn named_color(name: &str) -> Option<(u8, u8, u8)> {
    COLORS_BY_NAME
        .iter()
        .find(|(color_name, _)| *color_name == name)
        .map(|(_, value)| *value)
}

static COLORS_BY_NAME: &[(&str, (u8, u8, u8))] = &[
    ("aliceblue", (240, 248, 255)),
    ("antiquewhite", (250, 235, 215)),
    ("aqua", (0, 255, 255)),
    ("aquamarine", (127, 255, 212)),
    ("azure", (240, 255, 255)),
    ("beige", (245, 245, 220)),
    ("bisque", (255, 228, 196)),
    ("black", (0, 0, 0)),
    ("blanchedalmond", (255, 235, 205)),
    ("blue", (0, 0, 255)),
    ("blueviolet", (138, 43, 226)),
    ("brown", (165, 42, 42)),
    ("burlywood", (222, 184, 135)),
    ("cadetblue", (95, 158, 160)),
    ("chartreuse", (127, 255, 0)),
    ("chocolate", (210, 105, 30)),
    ("coral", (255, 127, 80)),
    ("cornflowerblue", (100, 149, 237)),
    ("cornsilk", (255, 248, 220)),
    ("crimson", (220, 20, 60)),
    ("cyan", (0, 255, 255)),
    ("darkblue", (0, 0, 139)),
    ("darkcyan", (0, 139, 139)),
    ("darkgoldenrod", (184, 134, 11)),
    ("darkgray", (169, 169, 169)),
    ("darkgreen", (0, 100, 0)),
    ("darkgrey", (169, 169, 169)),
    ("darkkhaki", (189, 183, 107)),
    ("darkmagenta", (139, 0, 139)),
    ("darkolivegreen", (85, 107, 47)),
    ("darkorange", (255, 140, 0)),
    ("darkorchid", (153, 50, 204)),
    ("darkred", (139, 0, 0)),
    ("darksalmon", (233, 150, 122)),
    ("darkseagreen", (143, 188, 143)),
    ("darkslateblue", (72, 61, 139)),
    ("darkslategray", (47, 79, 79)),
    ("darkslategrey", (47, 79, 79)),
    ("darkturquoise", (0, 206, 209)),
    ("darkviolet", (148, 0, 211)),
    ("deeppink", (255, 20, 147)),
    ("deepskyblue", (0, 191, 255)),
    ("dimgray", (105, 105, 105)),
    ("dimgrey", (105, 105, 105)),
    ("dodgerblue", (30, 144, 255)),
    ("firebrick", (178, 34, 34)),
    ("floralwhite", (255, 250, 240)),
    ("forestgreen", (34, 139, 34)),
    ("fuchsia", (255, 0, 255)),
    ("gainsboro", (220, 220, 220)),
    ("ghostwhite", (248, 248, 255)),
    ("gold", (255, 215, 0)),
    ("goldenrod", (218, 165, 32)),
    ("gray", (128, 128, 128)),
    ("green", (0, 128, 0)),
    ("greenyellow", (173, 255, 47)),
    ("grey", (128, 128, 128)),
    ("honeydew", (240, 255, 240)),
    ("hotpink", (255, 105, 180)),
    ("indianred", (205, 92, 92)),
    ("indigo", (75, 0, 130)),
    ("ivory", (255, 255, 240)),
    ("khaki", (240, 230, 140)),
    ("lavender", (230, 230, 250)),
    ("lavenderblush", (255, 240, 245)),
    ("lawngreen", (124, 252, 0)),
    ("lemonchiffon", (255, 250, 205)),
    ("lightblue", (173, 216, 230)),
    ("lightcoral", (240, 128, 128)),
    ("lightcyan", (224, 255, 255)),
    ("lightgoldenrodyellow", (250, 250, 210)),
    ("lightgray", (211, 211, 211)),
    ("lightgreen", (144, 238, 144)),
    ("lightgrey", (211, 211, 211)),
    ("lightpink", (255, 182, 193)),
    ("lightsalmon", (255, 160, 122)),
    ("lightseagreen", (32, 178, 170)),
    ("lightskyblue", (135, 206, 250)),
    ("lightslategray", (119, 136, 153)),
    ("lightslategrey", (119, 136, 153)),
    ("lightsteelblue", (176, 196, 222)),
    ("lightyellow", (255, 255, 224)),
    ("lime", (0, 255, 0)),
    ("limegreen", (50, 205, 50)),
    ("linen", (250, 240, 230)),
    ("magenta", (255, 0, 255)),
    ("maroon", (128, 0, 0)),
    ("mediumaquamarine", (102, 205, 170)),
    ("mediumblue", (0, 0, 205)),
    ("mediumorchid", (186, 85, 211)),
    ("mediumpurple", (147, 112, 219)),
    ("mediumseagreen", (60, 179, 113)),
    ("mediumslateblue", (123, 104, 238)),
    ("mediumspringgreen", (0, 250, 154)),
    ("mediumturquoise", (72, 209, 204)),
    ("mediumvioletred", (199, 21, 133)),
    ("midnightblue", (25, 25, 112)),
    ("mintcream", (245, 255, 250)),
    ("mistyrose", (255, 228, 225)),
    ("moccasin", (255, 228, 181)),
    ("navajowhite", (255, 222, 173)),
    ("navy", (0, 0, 128)),
    ("oldlace", (253, 245, 230)),
    ("olive", (128, 128, 0)),
    ("olivedrab", (107, 142, 35)),
    ("orange", (255, 165, 0)),
    ("orangered", (255, 69, 0)),
    ("orchid", (218, 112, 214)),
    ("palegoldenrod", (238, 232, 170)),
    ("palegreen", (152, 251, 152)),
    ("paleturquoise", (175, 238, 238)),
    ("palevioletred", (219, 112, 147)),
    ("papayawhip", (255, 239, 213)),
    ("peachpuff", (255, 218, 185)),
    ("peru", (205, 133, 63)),
    ("pink", (255, 192, 203)),
    ("plum", (221, 160, 221)),
    ("powderblue", (176, 224, 230)),
    ("purple", (128, 0, 128)),
    ("red", (255, 0, 0)),
    ("rosybrown", (188, 143, 143)),
    ("royalblue", (65, 105, 225)),
    ("saddlebrown", (139, 69, 19)),
    ("salmon", (250, 128, 114)),
    ("sandybrown", (244, 164, 96)),
    ("seagreen", (46, 139, 87)),
    ("seashell", (255, 245, 238)),
    ("sienna", (160, 82, 45)),
    ("silver", (192, 192, 192)),
    ("skyblue", (135, 206, 235)),
    ("slateblue", (106, 90, 205)),
    ("slategray", (112, 128, 144)),
    ("slategrey", (112, 128, 144)),
    ("snow", (255, 250, 250)),
    ("springgreen", (0, 255, 127)),
    ("steelblue", (70, 130, 180)),
    ("tan", (210, 180, 140)),
    ("teal", (0, 128, 128)),
    ("thistle", (216, 191, 216)),
    ("tomato", (255, 99, 71)),
    ("turquoise", (64, 224, 208)),
    ("violet", (238, 130, 238)),
    ("wheat", (245, 222, 179)),
    ("white", (255, 255, 255)),
    ("whitesmoke", (245, 245, 245)),
    ("yellow", (255, 255, 0)),
    ("yellowgreen", (154, 205, 50)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_short_hex() {
        assert_eq!(Color::parse("#CCFFCC").unwrap().as_hex(), "#cfc");
        assert_eq!(Color::parse("#006600").unwrap().as_hex(), "#060");
        assert_eq!(Color::parse("#99aa99").unwrap().as_hex(), "#9a9");
        assert_eq!(Color::parse("#123456").unwrap().as_hex(), "#123456");
        assert_eq!(Color::parse("white").unwrap().as_hex(), "#fff");
    }

    #[test]
    fn alpha_round_trips() {
        let color = Color::parse("rgba(255, 0, 0, 0.5)").unwrap();
        assert_eq!(color.alpha, Some(0.5));
        assert_eq!(color.as_hex(), "#ff000080");
        assert_eq!(Color::parse("transparent").unwrap().as_hex(), "#0000");
    }
}
