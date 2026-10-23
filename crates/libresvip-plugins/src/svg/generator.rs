//! Port of `libresvip.plugins.svg.coordinate_helper`, `svg_factory` and
//! `svg_generator`, plus the tiny XML writer from the `svg` Python package.
//!
//! The Python plugin hands a tree of `svg` dataclasses to `SVG.as_str()`,
//! which concatenates elements without any indentation or escaping. Attribute
//! order follows dataclass field order, so every element is built with its
//! attributes in that exact order here.

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, Track};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::color::Color;

const PADDING: i64 = 4;
const TICKS_IN_BEAT: i64 = 480;

/// Formats a float the way Python's `str()` does for the values produced here.
pub fn py_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        return format!("{value:.1}");
    }
    let mut rendered = format!("{value}");
    if let Some(index) = rendered.find(['e', 'E']) {
        let (mantissa, exponent) = rendered.split_at(index);
        let exponent = &exponent[1..];
        let (sign, digits) = match exponent.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("+", exponent),
        };
        rendered = format!("{mantissa}e{sign}{digits:0>2}");
    }
    rendered
}

// ---------------------------------------------------------------------------
// XML elements (`svg.elements`)
// ---------------------------------------------------------------------------

pub struct Element {
    name: &'static str,
    attrs: Vec<(&'static str, String)>,
    text: Option<String>,
    children: Vec<Element>,
}

impl Element {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            text: None,
            children: Vec::new(),
        }
    }

    fn attr(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((key, value.into()));
        self
    }

    fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    fn children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        self.write(&mut output);
        output
    }

    fn write(&self, output: &mut String) {
        output.push('<');
        output.push_str(self.name);
        for (key, value) in &self.attrs {
            output.push(' ');
            output.push_str(key);
            output.push_str("=\"");
            output.push_str(value);
            output.push('"');
        }
        if let Some(text) = &self.text {
            output.push('>');
            output.push_str(text);
            output.push_str("</");
            output.push_str(self.name);
            output.push('>');
        } else if !self.children.is_empty() {
            output.push('>');
            for child in &self.children {
                child.write(output);
            }
            output.push_str("</");
            output.push_str(self.name);
            output.push('>');
        } else {
            output.push_str("/>");
        }
    }
}

// ---------------------------------------------------------------------------
// CoordinateHelper
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum TextAlign {
    Start,
    Middle,
    End,
}

impl TextAlign {
    fn value(self) -> &'static str {
        match self {
            TextAlign::Start => "start",
            TextAlign::Middle => "middle",
            TextAlign::End => "end",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum TextPosition {
    Upper,
    Inner,
    Lower,
    None,
}

pub struct CoordinateHelper {
    pub pixel_per_beat: i64,
    pub note_height: i64,
    pub text_align: TextAlign,
    pub pitch_position_offset: i64,
    pub position_range_start: i64,
    pub position_range_end: i64,
    pub key_range_start: i64,
    pub key_range_end: i64,
}

struct NotePositionParameters {
    point_1: (f64, f64),
    point_2: (f64, f64),
    inner_text: (f64, f64),
    upper_text: (f64, f64),
    lower_text: (f64, f64),
}

impl CoordinateHelper {
    pub fn calculate_range(&mut self, note_list: &[Note], pitch_points: &mut Vec<Point>) {
        if pitch_points.is_empty() {
            *pitch_points = vec![Point::start_point(), Point::end_point()];
        }
        let first_start = note_list
            .first()
            .map(|note| note.start_pos)
            .unwrap_or(0);
        let last_end = note_list.last().map(Note::end_pos).unwrap_or(0);
        self.position_range_start = first_start.min(pitch_points[1].x);
        self.position_range_end = last_end.max(pitch_points[pitch_points.len() - 2].x);
        self.key_range_start = note_list
            .iter()
            .map(|note| note.key_number)
            .min()
            .unwrap_or(0);
        self.key_range_end = note_list
            .iter()
            .map(|note| note.key_number)
            .max()
            .unwrap_or(0);
        for point in &pitch_points[1..pitch_points.len() - 1] {
            if point.y == -100 {
                continue;
            }
            let value = point.y as f64 / 100.0;
            self.key_range_start = self.key_range_start.min(value.floor() as i64);
            self.key_range_end = self.key_range_end.max(value.ceil() as i64);
        }
    }

    fn get_note_position_parameters(&self, note: &Note) -> NotePositionParameters {
        let text_x = match self.text_align {
            TextAlign::Start => {
                ((note.start_pos - self.position_range_start) * self.pixel_per_beat) as f64
                    / TICKS_IN_BEAT as f64
                    + PADDING as f64
            }
            TextAlign::Middle => {
                (((note.start_pos + note.end_pos()) as f64) / 2.0
                    - self.position_range_start as f64)
                    * self.pixel_per_beat as f64
                    / TICKS_IN_BEAT as f64
            }
            TextAlign::End => {
                ((note.end_pos() - self.position_range_start) * self.pixel_per_beat) as f64
                    / TICKS_IN_BEAT as f64
                    - PADDING as f64
            }
        };
        let text_x = text_x as i64;
        let point_1 = (
            (((note.start_pos - self.position_range_start) * self.pixel_per_beat) as f64
                / TICKS_IN_BEAT as f64) as i64 as f64,
            ((self.key_range_end - note.key_number) * self.note_height) as f64,
        );
        let point_2 = (
            (((note.end_pos() - self.position_range_start) * self.pixel_per_beat) as f64
                / TICKS_IN_BEAT as f64) as i64 as f64,
            ((self.key_range_end - note.key_number + 1) * self.note_height) as f64,
        );
        NotePositionParameters {
            point_1,
            point_2,
            inner_text: (
                text_x as f64,
                ((self.key_range_end - note.key_number + 1) * self.note_height) as f64
                    - PADDING as f64 * 1.5,
            ),
            upper_text: (
                text_x as f64,
                ((self.key_range_end - note.key_number) * self.note_height) as f64
                    - PADDING as f64,
            ),
            lower_text: (
                text_x as f64,
                ((self.key_range_end - note.key_number + 2) * self.note_height - PADDING) as f64,
            ),
        }
    }

    fn get_pitch_point(&self, point: Point) -> (f64, f64) {
        (
            ((point.x - self.position_range_start - self.pitch_position_offset)
                * self.pixel_per_beat) as f64
                / TICKS_IN_BEAT as f64,
            (self.key_range_end as f64 - point.y as f64 / 100.0 + 0.5) * self.note_height as f64,
        )
    }

    fn size(&self) -> (f64, i64) {
        (
            ((self.position_range_end - self.position_range_start) * self.pixel_per_beat) as f64
                / TICKS_IN_BEAT as f64,
            (self.key_range_end - self.key_range_start + 1) * self.note_height,
        )
    }

    fn font_size(&self) -> i64 {
        self.note_height - 2 * PADDING
    }

    fn text_anchor(&self) -> &'static str {
        self.text_align.value()
    }
}

// ---------------------------------------------------------------------------
// SvgFactory
// ---------------------------------------------------------------------------

pub struct SvgFactory {
    coordinate_helper: CoordinateHelper,
    #[allow(dead_code)]
    time_synchronizer: TimeSynchronizer,
    pub note_height: i64,
    pub note_round: i64,
    pub note_stroke_width: i64,
    pub pitch_stroke_width: i64,
    pub lyric_position: TextPosition,
    pub pronounciation_position: TextPosition,
    pub note_fill_color: Color,
    pub note_stroke_color: Color,
    pub pitch_stroke_color: Color,
    pub inner_text_color: Color,
    pub side_text_color: Color,
    pub grid_color: Color,
    pub grid_stroke_width: i64,
    pub line_elements: Vec<Element>,
    pub polyline_elements: Vec<Element>,
    pub rect_elements: Vec<Element>,
    pub text_elements: Vec<Element>,
    pub pitch_points_buf: Vec<Point>,
    pub style: String,
}

impl SvgFactory {
    pub fn new(
        coordinate_helper: CoordinateHelper,
        time_synchronizer: TimeSynchronizer,
        options: &Options,
    ) -> Result<Self> {
        let mut factory = Self {
            coordinate_helper,
            time_synchronizer,
            note_height: 0,
            note_round: 0,
            note_stroke_width: 0,
            pitch_stroke_width: 0,
            lyric_position: TextPosition::Lower,
            pronounciation_position: TextPosition::Inner,
            note_fill_color: Color::parse("#CCFFCC")?,
            note_stroke_color: Color::parse("#006600")?,
            pitch_stroke_color: Color::parse("#99aa99")?,
            inner_text_color: Color::parse("#000000")?,
            side_text_color: Color::parse("#000000")?,
            grid_color: Color::parse("#CCCCCC")?,
            grid_stroke_width: 1,
            line_elements: Vec::new(),
            polyline_elements: Vec::new(),
            rect_elements: Vec::new(),
            text_elements: Vec::new(),
            pitch_points_buf: Vec::new(),
            style: String::new(),
        };
        factory.note_height = options.int("note_height", 24);
        factory.note_round = options.int("note_round", 4);
        factory.note_stroke_width = options.int("note_stroke_width", 1);
        factory.pitch_stroke_width = options.int("pitch_stroke_width", 2);
        factory.note_fill_color = Color::parse(&options.str("note_fill_color", "#CCFFCC"))?;
        factory.note_stroke_color = Color::parse(&options.str("note_stroke_color", "#006600"))?;
        factory.pitch_stroke_color = Color::parse(&options.str("pitch_stroke_color", "#99aa99"))?;
        factory.inner_text_color = Color::parse(&options.str("inner_text_color", "#000000"))?;
        factory.side_text_color = Color::parse(&options.str("side_text_color", "#000000"))?;
        factory.grid_color = Color::parse(&options.str("grid_color", "#CCCCCC"))?;
        factory.grid_stroke_width = options.int("grid_stroke_width", 1);
        factory.lyric_position = parse_position(&options.str("lyric_position", "lower"))?;
        factory.pronounciation_position =
            parse_position(&options.str("pronounciation_position", "inner"))?;
        factory.style = format!(
            ".note {{\n    fill: {};\n    stroke: {};\n    stroke-width: {}px;\n}}\n\
.pitch {{\n    fill: none;\n    stroke: {};\n    stroke-width: {}px;\n}}\n\
text {{\n    font-size: {}px;\n    text-anchor: {};\n}}\n\
.inner {{\n    fill: {};\n}}\n\
.side {{\n    fill: {};\n    font-size: {}px;\n}}\n\
.line {{\n    fill: none;\n    stroke: {};\n    stroke-width: {}px;\n}}",
            factory.note_fill_color.as_hex(),
            factory.note_stroke_color.as_hex(),
            factory.note_stroke_width,
            factory.pitch_stroke_color.as_hex(),
            factory.pitch_stroke_width,
            factory.coordinate_helper.font_size(),
            factory.coordinate_helper.text_anchor(),
            factory.inner_text_color.as_hex(),
            factory.side_text_color.as_hex(),
            std::cmp::max(factory.coordinate_helper.font_size() - 4, 10),
            factory.grid_color.as_hex(),
            factory.grid_stroke_width,
        );
        Ok(factory)
    }

    fn width(&self) -> f64 {
        self.coordinate_helper.size().0
    }

    pub fn draw_grid(&mut self, time_signature_list: &[libresvip_core::model::base::TimeSignature]) {
        let beat_start = self.coordinate_helper.position_range_start;
        let mut prev_pos = 0i64;
        for pair in time_signature_list.windows(2) {
            let previous_time_signature = &pair[0];
            let time_signature = &pair[1];
            let beat_length = (time_signature.bar_index - previous_time_signature.bar_index)
                * previous_time_signature.numerator;
            for beat_offset in 0..beat_length.max(0) {
                let pos = prev_pos + beat_offset * TICKS_IN_BEAT;
                if pos >= beat_start {
                    let x = (((pos - self.coordinate_helper.position_range_start)
                        * self.coordinate_helper.pixel_per_beat) as f64
                        / TICKS_IN_BEAT as f64) as i64;
                    self.line_elements.push(
                        Element::new("line")
                            .attr("class", "line")
                            .attr("x1", x.to_string())
                            .attr("y1", "0")
                            .attr("x2", x.to_string())
                            .attr("y2", self.coordinate_helper.size().1.to_string()),
                    );
                }
            }
            prev_pos += beat_length * TICKS_IN_BEAT;
        }
        if !time_signature_list.is_empty() {
            let mut pos = prev_pos;
            while pos <= self.coordinate_helper.position_range_end {
                pos += TICKS_IN_BEAT;
                if pos >= beat_start {
                    let x = (((pos - beat_start) * self.coordinate_helper.pixel_per_beat) as f64
                        / TICKS_IN_BEAT as f64) as i64;
                    self.line_elements.push(
                        Element::new("line")
                            .attr("class", "line")
                            .attr("x1", x.to_string())
                            .attr("y1", "0")
                            .attr("x2", x.to_string())
                            .attr("y2", self.coordinate_helper.size().1.to_string()),
                    );
                }
            }
        }
        for key in self.coordinate_helper.key_range_start..=self.coordinate_helper.key_range_end {
            let y = (self.coordinate_helper.key_range_end - key) * self.note_height;
            self.line_elements.push(
                Element::new("line")
                    .attr("class", "line")
                    .attr("x1", "0")
                    .attr("y1", y.to_string())
                    .attr("x2", py_float(self.width()))
                    .attr("y2", y.to_string()),
            );
        }
    }

    fn font_size(&self) -> i64 {
        self.coordinate_helper.font_size()
    }

    fn draw_text(
        &mut self,
        position: TextPosition,
        text: Option<&str>,
        parameters: &NotePositionParameters,
        is_phoneme: bool,
    ) {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return;
        };
        let insert_pos = match position {
            TextPosition::Upper => parameters.upper_text,
            TextPosition::Lower => parameters.lower_text,
            TextPosition::Inner => parameters.inner_text,
            TextPosition::None => return,
        };
        let mut class_name = if position == TextPosition::Inner {
            "inner".to_owned()
        } else {
            "side".to_owned()
        };
        if is_phoneme {
            class_name.push_str(" pinyin");
        }
        self.text_elements.push(
            Element::new("text")
                .attr("font-size", self.font_size().to_string())
                .attr("class", class_name)
                .attr("x", py_float(insert_pos.0))
                .attr("y", py_float(insert_pos.1))
                .text(text.to_owned()),
        );
    }

    pub fn draw_note(&mut self, note: &Note) {
        let parameters = self.coordinate_helper.get_note_position_parameters(note);
        self.rect_elements.push(
            Element::new("rect")
                .attr("class", "note")
                .attr("x", py_float(parameters.point_1.0))
                .attr("y", py_float(parameters.point_1.1))
                .attr("width", py_float(parameters.point_2.0 - parameters.point_1.0))
                .attr(
                    "height",
                    py_float(parameters.point_2.1 - parameters.point_1.1),
                )
                .attr("rx", self.note_round.to_string())
                .attr("ry", self.note_round.to_string()),
        );
        self.draw_text(
            self.lyric_position,
            Some(note.lyric.as_str()),
            &parameters,
            false,
        );
        self.draw_text(
            self.pronounciation_position,
            note.pronunciation.as_deref(),
            &parameters,
            true,
        );
    }

    pub fn draw_pitch(&mut self, point: Point) {
        if point.y == -100 {
            if !self.pitch_points_buf.is_empty() {
                let points = std::mem::take(&mut self.pitch_points_buf);
                // `chain.from_iterable(get_pitch_point(p) ...)` flattens the
                // (x, y) pairs into a list of floats, which `svg` joins with
                // spaces.
                let rendered = points
                    .iter()
                    .flat_map(|point| {
                        let (x, y) = self.coordinate_helper.get_pitch_point(*point);
                        [py_float(x), py_float(y)]
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                self.polyline_elements.push(
                    Element::new("polyline")
                        .attr("class", "pitch")
                        .attr("points", rendered),
                );
            }
        } else {
            self.pitch_points_buf.push(point);
        }
    }
}

pub fn parse_position(value: &str) -> Result<TextPosition> {
    match value {
        "upper" => Ok(TextPosition::Upper),
        "inner" => Ok(TextPosition::Inner),
        "lower" => Ok(TextPosition::Lower),
        "none" => Ok(TextPosition::None),
        _ => Err(LibreSvipError::Validation(
            "Input should be 'upper', 'inner', 'lower' or 'none'".to_owned(),
        )),
    }
}

pub fn parse_align(value: &str) -> Result<TextAlign> {
    match value {
        "start" => Ok(TextAlign::Start),
        "middle" => Ok(TextAlign::Middle),
        "end" => Ok(TextAlign::End),
        _ => Err(LibreSvipError::Validation(
            "Input should be 'start', 'middle' or 'end'".to_owned(),
        )),
    }
}

// ---------------------------------------------------------------------------
// SvgGenerator
// ---------------------------------------------------------------------------

pub fn generate_project(project: &Project, options: &Options) -> Result<String> {
    let pixel_per_beat = options.int("pixel_per_beat", 48);
    let note_height = options.int("note_height", 24);
    let text_align = parse_align(&options.str("text_align", "start"))?;
    let show_grid = options.bool("show_grid", false);
    let track_index = options.int("track_index", -1);

    let first_time_signature = project
        .time_signature_list
        .first()
        .ok_or_else(|| LibreSvipError::Message("Project has no tracks".to_owned()))?;
    let pitch_position_offset = first_time_signature.bar_length() as i64;

    let track = if track_index < 0 {
        project
            .track_list
            .iter()
            .find_map(|track| match track {
                Track::Singing(singing) if !singing.note_list.is_empty() => Some(singing),
                _ => None,
            })
    } else {
        let index = usize::try_from(track_index).map_err(|_| LibreSvipError::NoTrack)?;
        project
            .track_list
            .get(index)
            .and_then(Track::as_singing)
    }
    .ok_or_else(|| LibreSvipError::Message("Project has no tracks".to_owned()))?;

    let mut pitch_points = track.edited_params.pitch.points.clone();
    let mut coordinate_helper = CoordinateHelper {
        pixel_per_beat,
        note_height,
        text_align,
        pitch_position_offset,
        position_range_start: 0,
        position_range_end: 0,
        key_range_start: 0,
        key_range_end: 0,
    };
    if track.note_list.is_empty() {
        return Err(LibreSvipError::Message(
            "Project has no tracks".to_owned(),
        ));
    }
    if !pitch_points.is_empty() && pitch_points.len() < 2 {
        return Err(LibreSvipError::Message(
            "Project has no tracks".to_owned(),
        ));
    }
    coordinate_helper.calculate_range(&track.note_list, &mut pitch_points);

    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let mut factory = SvgFactory::new(coordinate_helper, synchronizer, options)?;
    if show_grid {
        factory.draw_grid(&project.time_signature_list);
    }
    for note in &track.note_list {
        factory.draw_note(note);
    }
    for point in &pitch_points[1..pitch_points.len() - 1] {
        factory.draw_pitch(*point);
    }

    let (width, height) = factory.coordinate_helper.size();
    let mut children = Vec::new();
    children.push(Element::new("style").text(factory.style.clone()));
    children.extend(std::mem::take(&mut factory.line_elements));
    children.extend(std::mem::take(&mut factory.rect_elements));
    children.extend(std::mem::take(&mut factory.polyline_elements));
    children.extend(std::mem::take(&mut factory.text_elements));
    let svg = Element::new("svg")
        .attr("xmlns", "http://www.w3.org/2000/svg")
        .attr("width", py_float(width))
        .attr("height", height.to_string())
        .children(children);
    Ok(svg.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_render_like_python() {
        assert_eq!(py_float(48.0), "48.0");
        assert_eq!(py_float(120.5), "120.5");
        assert_eq!(py_float(-0.0), "-0.0");
    }
}
