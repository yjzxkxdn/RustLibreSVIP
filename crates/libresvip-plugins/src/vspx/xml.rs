//! XML rendering/parsing for VSPX.
//!
//! `render` reproduces the bytes `vocalsharp_converter.dump` produces: the
//! xsdata serializer with `pretty_print=True, pretty_print_indent="\t"` run
//! through its four post-processing regexes. The resulting document is emitted
//! directly:
//!
//! * one element per line, indented with tabs, trailing newline after the root,
//! * `name`/`Name`/`Singer`/`lyric`/`LSD`/`symbol`/`sample`/`path` as CDATA,
//! * empty elements expanded to `<tag>` / `</tag>` on the same indentation,
//! * parameter points collapsed to `<P><t>…</t><v>…</v></P>`.

use libresvip_core::error::{LibreSvipError, Result};

use super::model::{
    Beat, DefaultParameter, InnerProject, InstrumentalTrack, Note, NoteTrack, Num, ParamKind,
    ParamPoint, Sequence, Syllable, SyllablePart, Tempo, Track, Trill, VspxProject,
};

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Python's `str(float)`.
pub fn py_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        let rendered = value.to_string();
        if rendered.contains(['.', 'e', 'E']) {
            rendered
        } else {
            format!("{rendered}.0")
        }
    }
}

fn num_string(value: Num) -> String {
    match value {
        Num::Int(value) => value.to_string(),
        Num::Float(value) => py_float(value),
    }
}

fn escape_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
    output
}

#[derive(Default)]
struct Writer {
    out: String,
}

impl Writer {
    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push('\t');
        }
    }

    fn open(&mut self, depth: usize, tag: &str) {
        self.out.push('\n');
        self.indent(depth);
        self.out.push('<');
        self.out.push_str(tag);
        self.out.push('>');
    }

    fn close(&mut self, depth: usize, tag: &str) {
        self.out.push('\n');
        self.indent(depth);
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }

    fn text(&mut self, depth: usize, tag: &str, value: &str) {
        self.open(depth, tag);
        self.out.push_str(&escape_text(value));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }

    fn integer(&mut self, depth: usize, tag: &str, value: i64) {
        self.text(depth, tag, &value.to_string());
    }

    fn float(&mut self, depth: usize, tag: &str, value: f64) {
        self.text(depth, tag, &py_float(value));
    }

    fn number(&mut self, depth: usize, tag: &str, value: Num) {
        self.text(depth, tag, &num_string(value));
    }

    fn cdata(&mut self, depth: usize, tag: &str, value: &str) {
        self.open(depth, tag);
        self.out.push_str("<![CDATA[");
        self.out.push_str(value);
        self.out.push_str("]]>");
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
    }
}

/// Renders the document the way `VocalSharpConverter.dump` writes it.
pub fn render(project: &VspxProject) -> String {
    let mut writer = Writer::default();
    // `start_document` writes the declaration and a newline; the root tag then
    // starts at column 0.
    writer.out.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n<VSPX>",
    );
    if let Some(version) = &project.version {
        writer.text(1, "Version", version);
    }
    writer.open(1, "Project");
    render_inner_project(&mut writer, &project.project);
    writer.close(1, "Project");
    writer.close(0, "VSPX");
    writer.out.push('\n');
    writer.out
}

fn render_inner_project(writer: &mut Writer, project: &InnerProject) {
    writer.integer(2, "SamplesPerSec", project.samples_per_sec);
    writer.integer(2, "Resolution", project.resolution);
    if let Some(duration) = project.duration {
        writer.integer(2, "Duration", duration);
    }
    if let Some(trill) = &project.default_trill {
        render_trill(writer, 2, "DefaultTrill", trill);
    }
    for tempo in &project.tempos {
        render_tempo(writer, tempo);
    }
    for beat in &project.beats {
        render_beat(writer, beat);
    }
    for track in &project.tracks {
        match track {
            Track::Note(note_track) => render_note_track(writer, note_track),
            Track::Stereo(track) => render_instrumental(writer, "StereoTrack", track),
            Track::Mono(track) => render_instrumental(writer, "MonoTrack", track),
        }
    }
}

fn render_tempo(writer: &mut Writer, tempo: &Tempo) {
    writer.open(2, "Tempo");
    writer.integer(3, "pos", tempo.pos);
    writer.float(3, "bpm", tempo.bpm);
    writer.close(2, "Tempo");
}

fn render_beat(writer: &mut Writer, beat: &Beat) {
    writer.open(2, "Beat");
    writer.integer(3, "barIndex", beat.bar_index);
    writer.integer(3, "beatPerBar", beat.beat_per_bar);
    writer.integer(3, "barDivide", beat.bar_divide);
    writer.close(2, "Beat");
}

fn render_trill(writer: &mut Writer, depth: usize, tag: &str, trill: &Trill) {
    writer.open(depth, tag);
    writer.number(depth + 1, "pos", trill.pos);
    writer.number(depth + 1, "amplitude", trill.amplitude);
    writer.number(depth + 1, "frequency", trill.frequency);
    writer.number(depth + 1, "phase", trill.phase);
    writer.close(depth, tag);
}

fn render_default_parameter(writer: &mut Writer, depth: usize, parameter: &DefaultParameter) {
    writer.open(depth, "DefaultParameter");
    writer.integer(depth + 1, "dyn", parameter.dyn_value);
    writer.integer(depth + 1, "str", parameter.strength);
    writer.integer(depth + 1, "bri", parameter.bri);
    writer.integer(depth + 1, "voc", parameter.voc);
    writer.integer(depth + 1, "bre", parameter.bre);
    writer.integer(depth + 1, "gen", parameter.gen);
    writer.integer(depth + 1, "gwl", parameter.gwl);
    writer.integer(depth + 1, "xsy", parameter.xsy);
    writer.close(depth, "DefaultParameter");
}

fn render_track_base(
    writer: &mut Writer,
    depth: usize,
    name: &Option<String>,
    pan: Num,
    gain: Num,
    is_mute: &Option<String>,
    is_solo: &Option<String>,
) {
    if let Some(name) = name {
        writer.cdata(depth, "Name", name);
    }
    writer.number(depth, "Pan", pan);
    writer.number(depth, "Gain", gain);
    if let Some(is_mute) = is_mute {
        writer.text(depth, "IsMute", is_mute);
    }
    if let Some(is_solo) = is_solo {
        writer.text(depth, "IsSolo", is_solo);
    }
}

fn render_note_track(writer: &mut Writer, track: &NoteTrack) {
    writer.open(2, "NoteTrack");
    render_track_base(
        writer,
        3,
        &track.name,
        track.pan,
        track.gain,
        &track.is_mute,
        &track.is_solo,
    );
    if let Some(singer) = &track.singer {
        writer.cdata(3, "Singer", singer);
    }
    if let Some(lsd) = &track.lsd {
        writer.cdata(3, "LSD", lsd);
    }
    render_default_parameter(writer, 3, &track.default_parameter);
    writer.number(3, "por", track.por);
    for note in &track.notes {
        render_note(writer, note);
    }
    writer.open(3, "Parameter");
    for point in &track.parameter {
        let tag = point.kind.tag();
        writer.open(4, tag);
        writer.out.push_str("<t>");
        writer.out.push_str(&point.time.to_string());
        writer.out.push_str("</t><v>");
        writer.out.push_str(&point.value.to_string());
        writer.out.push_str("</v></");
        writer.out.push_str(tag);
        writer.out.push('>');
    }
    writer.close(3, "Parameter");
    writer.close(2, "NoteTrack");
}

fn render_note(writer: &mut Writer, note: &Note) {
    writer.open(3, "Note");
    writer.text(4, "pitch", &note.pitch);
    writer.integer(4, "pos", note.pos);
    writer.integer(4, "duration", note.duration);
    if let Some(lyric) = &note.lyric {
        writer.cdata(4, "lyric", lyric);
    }
    if let Some(trill) = &note.trill {
        render_trill(writer, 4, "trill", trill);
    }
    for syllable in &note.syllables {
        render_syllable(writer, syllable);
    }
    writer.close(3, "Note");
}

fn render_syllable(writer: &mut Writer, syllable: &Syllable) {
    writer.open(4, "Syllable");
    if let Some(head) = &syllable.head {
        render_syllable_part(writer, 5, "head", head);
    }
    if let Some(cur) = &syllable.cur {
        render_syllable_part(writer, 5, "cur", cur);
    }
    if let Some(tail) = &syllable.tail {
        render_syllable_part(writer, 5, "tail", tail);
    }
    writer.close(4, "Syllable");
}

fn render_syllable_part(writer: &mut Writer, depth: usize, tag: &str, part: &SyllablePart) {
    writer.open(depth, tag);
    for point in &part.points {
        writer.open(depth + 1, "p");
        writer.float(depth + 2, "x", point.x as f64);
        writer.float(depth + 2, "y", point.y as f64);
        writer.close(depth + 1, "p");
    }
    if let Some(sample) = &part.sample {
        writer.cdata(depth, "sample", sample);
    }
    if let Some(symbol) = &part.symbol {
        writer.cdata(depth, "symbol", symbol);
    }
    writer.close(depth, tag);
}

fn render_instrumental(writer: &mut Writer, tag: &str, track: &InstrumentalTrack) {
    writer.open(2, tag);
    render_track_base(
        writer,
        3,
        &track.name,
        track.pan,
        track.gain,
        &track.is_mute,
        &track.is_solo,
    );
    for sequence in &track.sequences {
        render_sequence(writer, sequence);
    }
    writer.close(2, tag);
}

fn render_sequence(writer: &mut Writer, sequence: &Sequence) {
    writer.open(3, "Sequence");
    if let Some(pos) = sequence.pos {
        writer.integer(4, "pos", pos);
    }
    if let Some(name) = &sequence.name {
        writer.cdata(4, "name", name);
    }
    if let Some(path) = &sequence.path {
        writer.cdata(4, "path", path);
    }
    writer.close(3, "Sequence");
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn node_text(node: roxmltree::Node<'_, '_>) -> Option<String> {
    let mut result = String::new();
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                result.push_str(text);
            }
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
}

fn children<'a>(
    node: roxmltree::Node<'a, 'a>,
    name: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    node.children()
        .filter(move |child| child.is_element() && child.tag_name().name() == name)
}

fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    child(node, name).and_then(node_text)
}

fn child_int(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<i64>> {
    match child_text(node, name) {
        None => Ok(None),
        Some(text) => text.trim().parse::<i64>().map(Some).map_err(|error| {
            LibreSvipError::msg(format!("invalid integer in <{name}>: {error}"))
        }),
    }
}

fn child_float(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<f64>> {
    match child_text(node, name) {
        None => Ok(None),
        Some(text) => text.trim().parse::<f64>().map(Some).map_err(|error| {
            LibreSvipError::msg(format!("invalid float in <{name}>: {error}"))
        }),
    }
}

fn parse_optional_int(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<i64>> {
    let value = child_int(node, name)?;
    Ok(value)
}

fn parse_int_default(node: roxmltree::Node<'_, '_>, name: &str, default: i64) -> Result<i64> {
    Ok(child_int(node, name)?.unwrap_or(default))
}

fn parse_float_default(node: roxmltree::Node<'_, '_>, name: &str, default: f64) -> Result<f64> {
    Ok(child_float(node, name)?.unwrap_or(default))
}

/// Parsed values become `float`, like pydantic validating a `float` field.
fn parse_num(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Option<Num>> {
    Ok(child_float(node, name)?.map(Num::Float))
}

fn parse_trill(node: roxmltree::Node<'_, '_>, default: Trill) -> Result<Trill> {
    Ok(Trill {
        pos: parse_num(node, "pos")?.unwrap_or(default.pos),
        amplitude: parse_num(node, "amplitude")?.unwrap_or(default.amplitude),
        frequency: parse_num(node, "frequency")?.unwrap_or(default.frequency),
        phase: parse_num(node, "phase")?.unwrap_or(default.phase),
    })
}

/// `XmlParser().from_bytes(bytes, VocalSharpProject)`.
pub fn parse(bytes: &[u8]) -> Result<VspxProject> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes)
        .map_err(|error| LibreSvipError::msg(format!("invalid UTF-8 XML: {error}")))?;
    let document = roxmltree::Document::parse(text)
        .map_err(|error| LibreSvipError::msg(format!("invalid XML: {error}")))?;
    let root = document.root_element();
    if root.tag_name().name() != "VSPX" {
        return Err(LibreSvipError::msg(format!(
            "expected root element <VSPX>, found <{}>",
            root.tag_name().name()
        )));
    }
    let version = child_text(root, "Version");
    let project = match child(root, "Project") {
        Some(node) => parse_inner_project(node)?,
        None => InnerProject::default(),
    };
    Ok(VspxProject { version, project })
}

fn parse_inner_project(node: roxmltree::Node<'_, '_>) -> Result<InnerProject> {
    let samples_per_sec = parse_int_default(node, "SamplesPerSec", 44100)?;
    let resolution = parse_int_default(node, "Resolution", 1920)?;
    let duration = parse_optional_int(node, "Duration")?;
    let default_trill = match child(node, "DefaultTrill") {
        Some(trill_node) => Some(parse_trill(trill_node, Trill::default())?),
        None => Some(Trill::default()),
    };
    let mut tempos = Vec::new();
    for tempo_node in children(node, "Tempo") {
        tempos.push(Tempo {
            pos: parse_int_default(tempo_node, "pos", 0)?,
            bpm: parse_float_default(tempo_node, "bpm", 120.0)?,
        });
    }
    let mut beats = Vec::new();
    for beat_node in children(node, "Beat") {
        beats.push(Beat {
            bar_index: parse_int_default(beat_node, "barIndex", 0)?,
            beat_per_bar: parse_int_default(beat_node, "beatPerBar", 4)?,
            bar_divide: parse_int_default(beat_node, "barDivide", 4)?,
        });
    }
    let mut tracks = Vec::new();
    for track_node in node.children().filter(|child| child.is_element()) {
        match track_node.tag_name().name() {
            "NoteTrack" => tracks.push(Track::Note(parse_note_track(track_node)?)),
            "StereoTrack" => tracks.push(Track::Stereo(parse_instrumental(track_node)?)),
            "MonoTrack" => tracks.push(Track::Mono(parse_instrumental(track_node)?)),
            _ => {}
        }
    }
    Ok(InnerProject {
        samples_per_sec,
        resolution,
        duration,
        default_trill,
        tempos,
        beats,
        tracks,
    })
}

fn parse_note_track(node: roxmltree::Node<'_, '_>) -> Result<NoteTrack> {
    let default_parameter = match child(node, "DefaultParameter") {
        Some(parameter_node) => DefaultParameter {
            dyn_value: parse_int_default(parameter_node, "dyn", 128)?,
            strength: parse_int_default(parameter_node, "str", 128)?,
            bri: parse_int_default(parameter_node, "bri", 128)?,
            voc: parse_int_default(parameter_node, "voc", 255)?,
            bre: parse_int_default(parameter_node, "bre", 128)?,
            gen: parse_int_default(parameter_node, "gen", 128)?,
            gwl: parse_int_default(parameter_node, "gwl", 0)?,
            xsy: parse_int_default(parameter_node, "xsy", 0)?,
        },
        None => DefaultParameter::default(),
    };
    let mut track = NoteTrack {
        name: child_text(node, "Name"),
        pan: parse_num(node, "Pan")?.unwrap_or(Num::Int(0)),
        gain: parse_num(node, "Gain")?.unwrap_or(Num::Int(0)),
        is_mute: child_text(node, "IsMute"),
        is_solo: child_text(node, "IsSolo"),
        singer: child_text(node, "Singer"),
        lsd: child_text(node, "LSD"),
        default_parameter,
        por: parse_num(node, "por")?.unwrap_or(Num::Float(0.07)),
        notes: Vec::new(),
        parameter: Vec::new(),
    };
    if track.name.is_none() {
        track.name = Some("神秘轨道_0".to_owned());
    }
    if track.is_mute.is_none() {
        track.is_mute = Some("False".to_owned());
    }
    if track.is_solo.is_none() {
        track.is_solo = Some("False".to_owned());
    }
    if track.singer.is_none() {
        track.singer = Some("神秘歌手".to_owned());
    }
    if track.lsd.is_none() {
        track.lsd = Some("神秘字典".to_owned());
    }
    for note_node in children(node, "Note") {
        track.notes.push(parse_note(note_node)?);
    }
    if let Some(parameter_node) = child(node, "Parameter") {
        for point_node in parameter_node.children().filter(|child| child.is_element()) {
            let Some(kind) = ParamKind::from_tag(point_node.tag_name().name()) else {
                continue;
            };
            track.parameter.push(ParamPoint {
                kind,
                time: parse_int_default(point_node, "t", 0)?,
                value: parse_int_default(point_node, "v", 0)?,
            });
        }
    }
    Ok(track)
}

fn parse_note(node: roxmltree::Node<'_, '_>) -> Result<Note> {
    let pitch = child_text(node, "pitch")
        .map(|text| text.trim().to_owned())
        .ok_or_else(|| LibreSvipError::msg("missing <pitch> in <Note>"))?;
    let trill = child(node, "trill")
        .map(|trill_node| parse_trill(trill_node, Trill::default()))
        .transpose()?;
    let mut syllables = Vec::new();
    for syllable_node in children(node, "Syllable") {
        syllables.push(Syllable {
            head: parse_syllable_part(syllable_node, "head")?,
            cur: parse_syllable_part(syllable_node, "cur")?,
            tail: parse_syllable_part(syllable_node, "tail")?,
        });
    }
    Ok(Note {
        pitch,
        pos: parse_int_default(node, "pos", 0)?,
        duration: parse_int_default(node, "duration", 0)?,
        lyric: child_text(node, "lyric"),
        trill,
        syllables,
    })
}

fn parse_syllable_part(
    node: roxmltree::Node<'_, '_>,
    name: &str,
) -> Result<Option<SyllablePart>> {
    let Some(part_node) = child(node, name) else {
        return Ok(None);
    };
    let mut points = Vec::new();
    for point_node in children(part_node, "p") {
        let x = child_float(point_node, "x")?.unwrap_or(0.0);
        let y = child_float(point_node, "y")?.unwrap_or(0.0);
        points.push(libresvip_core::model::point::Point::new(
            x as i64,
            y as i64,
        ));
    }
    Ok(Some(SyllablePart {
        points,
        sample: child_text(part_node, "sample"),
        symbol: child_text(part_node, "symbol"),
    }))
}

fn parse_instrumental(node: roxmltree::Node<'_, '_>) -> Result<InstrumentalTrack> {
    let mut track = InstrumentalTrack {
        name: child_text(node, "Name"),
        pan: parse_num(node, "Pan")?.unwrap_or(Num::Int(0)),
        gain: parse_num(node, "Gain")?.unwrap_or(Num::Int(0)),
        is_mute: child_text(node, "IsMute"),
        is_solo: child_text(node, "IsSolo"),
        sequences: Vec::new(),
    };
    if track.name.is_none() {
        track.name = Some("神秘轨道_0".to_owned());
    }
    if track.is_mute.is_none() {
        track.is_mute = Some("False".to_owned());
    }
    if track.is_solo.is_none() {
        track.is_solo = Some("False".to_owned());
    }
    for sequence_node in children(node, "Sequence") {
        track.sequences.push(Sequence {
            pos: parse_optional_int(sequence_node, "pos")?,
            name: child_text(sequence_node, "name"),
            path: child_text(sequence_node, "path"),
        });
    }
    Ok(track)
}
