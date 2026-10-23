//! Port of `libresvip.plugins.musicxml.musicxml_generator`.

use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature, TICKS_IN_BEAT};
use libresvip_core::options::Options;

use super::music::midi2note;
use super::xml::{decimal_from_float, py_float, Node};

const DEFAULT_TICK_RATE_CEVIO: f64 = 2.0;
const DIVISIONS: i64 = TICKS_IN_BEAT * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoteKind {
    Begin,
    Middle,
    End,
    Single,
}

impl NoteKind {
    fn syllabic(self) -> &'static str {
        match self {
            NoteKind::Begin => "begin",
            NoteKind::Middle => "middle",
            NoteKind::End => "end",
            NoteKind::Single => "single",
        }
    }

    fn tie(self) -> Option<&'static str> {
        match self {
            NoteKind::Begin => Some("start"),
            NoteKind::End => Some("stop"),
            _ => None,
        }
    }
}

struct MeasureContent {
    duration: i64,
    note: Option<Note>,
    note_type: Option<NoteKind>,
    bpm: Option<f64>,
}

impl MeasureContent {
    fn with_rest(duration: i64) -> Self {
        Self {
            duration,
            note: None,
            note_type: None,
            bpm: None,
        }
    }

    fn with_tempo(bpm: f64) -> Self {
        Self {
            duration: 0,
            note: None,
            note_type: None,
            bpm: Some(bpm),
        }
    }

    fn with_note(duration: i64, note: Note, note_type: NoteKind) -> Self {
        Self {
            duration,
            note: Some(note),
            note_type: Some(note_type),
            bpm: None,
        }
    }
}

struct Measure {
    #[allow(dead_code)]
    tick_start: i64,
    #[allow(dead_code)]
    length: i64,
    time_signature: Option<TimeSignature>,
    contents: Vec<MeasureContent>,
}

#[derive(Clone)]
struct KeyTick {
    tick: i64,
    tempo: Option<f64>,
    note_start: Option<Note>,
    note_end: Option<Note>,
}

// ---------------------------------------------------------------------------
// Output model
// ---------------------------------------------------------------------------

#[derive(Default)]
struct OutNotation {
    tied: Vec<&'static str>,
    slurs: Vec<(&'static str, i64)>,
}

struct OutNote {
    duration: i64,
    rest: bool,
    pitch: Option<(String, Option<i64>, i64)>,
    tie: Option<&'static str>,
    voice: i64,
    notations: Vec<OutNotation>,
    syllabic: &'static str,
    lyric: String,
}

enum OutContent {
    Note(usize),
    Rest { duration: i64 },
}

struct OutMeasure {
    number: usize,
    time_signature: Option<(i64, i64)>,
    contents: Vec<OutContent>,
    directions: Vec<Node>,
    sounds: Vec<Node>,
}

struct OutPart {
    id: String,
    title: String,
    measures: Vec<OutMeasure>,
    notes: Vec<OutNote>,
}

pub struct MusicXmlGenerator<'a> {
    #[allow(dead_code)]
    options: &'a Options,
}

impl<'a> MusicXmlGenerator<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self { options }
    }

    pub fn generate_project(&self, project: &Project) -> Node {
        let (tempos, tracks) = apply_tick_rate(project);
        let mut measures_by_track: Vec<Option<Vec<Measure>>> =
            (0..tracks.len()).map(|_| None).collect();
        for (index, track) in tracks.iter().enumerate() {
            let key_ticks = get_key_ticks(index, track, &tempos);
            if key_ticks.is_empty() {
                continue;
            }
            let measures = get_measures(&key_ticks, &project.time_signature_list);
            measures_by_track[index] = Some(measures);
        }

        let mut parts: Vec<OutPart> = Vec::new();
        for (index, measures) in measures_by_track.iter().enumerate() {
            let Some(measures) = measures else {
                continue;
            };
            let title = &tracks[index].title;
            parts.push(self.generate_part(measures, index, Some(title.as_str())));
        }

        let mut identification = Node::new("identification");
        identification.children.push(
            Node::new("encoding").child(Node::new("software").text("LibreSVIP")),
        );
        let mut part_list = Node::new("part-list");
        for part in &parts {
            part_list.children.push(
                Node::new("score-part")
                    .attr("id", part.id.clone())
                    .child(Node::new("part-name").text(part.title.clone())),
            );
        }
        let mut root = Node::new("score-partwise")
            .attr("version", "4.0")
            .child(identification)
            .child(part_list);
        for part in &parts {
            root.children.push(render_part(part));
        }
        root
    }

    fn generate_part(
        &self,
        measures: &[Measure],
        track_index: usize,
        track_title: Option<&str>,
    ) -> OutPart {
        let part_id = format!("P{}", track_index + 1);
        let title = track_title
            .map(str::to_owned)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| format!("Track {}", track_index + 1));
        let mut out_measures: Vec<OutMeasure> = Vec::new();
        let mut notes: Vec<OutNote> = Vec::new();
        let mut prev_note: Option<usize> = None;
        let mut prev_is_slur = false;
        for (measure_index, measure) in measures.iter().enumerate() {
            let mut slur_number = 1i64;
            let mut out_measure = OutMeasure {
                number: measure_index + 1,
                time_signature: measure
                    .time_signature
                    .as_ref()
                    .map(|signature| (signature.numerator, signature.denominator)),
                contents: Vec::new(),
                directions: Vec::new(),
                sounds: Vec::new(),
            };
            for content in &measure.contents {
                if let Some(bpm) = content.bpm {
                    let (sound_node, direction_node) = generate_nodes_for_tempo(bpm);
                    out_measure.sounds.push(sound_node);
                    out_measure.directions.push(direction_node);
                    continue;
                }
                let Some(note_type) = content.note_type else {
                    // Rest (or tempo-less filler).
                    if let Some(previous) = prev_note {
                        if prev_is_slur {
                            let notation = ensure_notation(&mut notes, previous);
                            let number = notation
                                .slurs
                                .first()
                                .map(|(_, number)| *number)
                                .unwrap_or(slur_number);
                            notation.slurs.push(("stop", number));
                            prev_note = None;
                            prev_is_slur = false;
                        }
                    }
                    out_measure.contents.push(OutContent::Rest {
                        duration: content.duration,
                    });
                    continue;
                };
                let Some(note) = content.note.clone() else {
                    continue;
                };
                let is_slur = note.lyric == "-";
                if let Some(previous) = prev_note {
                    let notation = ensure_notation(&mut notes, previous);
                    if is_slur && !prev_is_slur {
                        notation.slurs.push(("start", slur_number));
                    } else if prev_is_slur {
                        let prev_slur_number = notation
                            .slurs
                            .first()
                            .map(|(_, number)| *number)
                            .unwrap_or(slur_number);
                        if is_slur {
                            notation.slurs.push(("continue", prev_slur_number));
                        } else {
                            notation.slurs.push(("stop", prev_slur_number));
                            slur_number += 1;
                        }
                    }
                }
                if let Some(mut note_node) = generate_note_node(&note, note_type, content.duration) {
                    note_node.voice = track_index as i64 + 2;
                    notes.push(note_node);
                    out_measure.contents.push(OutContent::Note(notes.len() - 1));
                    prev_note = Some(notes.len() - 1);
                    prev_is_slur = is_slur;
                }
            }
            out_measures.push(out_measure);
        }
        if let Some(previous) = prev_note {
            if prev_is_slur {
                let notation = ensure_notation(&mut notes, previous);
                let number = notation
                    .slurs
                    .first()
                    .map(|(_, number)| *number)
                    .unwrap_or(1);
                notation.slurs.push(("stop", number));
            }
        }
        OutPart {
            id: part_id,
            title,
            measures: out_measures,
            notes,
        }
    }
}

fn ensure_notation(notes: &mut [OutNote], note_index: usize) -> &mut OutNotation {
    if notes[note_index].notations.is_empty() {
        notes[note_index].notations.push(OutNotation::default());
    }
    &mut notes[note_index].notations[0]
}

fn generate_note_node(note: &Note, note_type: NoteKind, duration: i64) -> Option<OutNote> {
    let key_str = midi2note(note.key_number as f64);
    let octave_digits: String = key_str
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if octave_digits.is_empty() {
        return None;
    }
    let octave: i64 = octave_digits.parse().ok()?;
    let step = key_str.chars().next()?;
    if !('A'..='G').contains(&step) {
        return None;
    }
    let alter = if key_str.contains('#') { Some(1) } else { None };
    let tie = note_type.tie();
    let mut notations = Vec::new();
    if let Some(tie) = tie {
        let mut notation = OutNotation::default();
        notation.tied.push(tie);
        notations.push(notation);
    }
    Some(OutNote {
        duration,
        rest: false,
        pitch: Some((step.to_string(), alter, octave)),
        tie,
        voice: 0,
        notations,
        syllabic: note_type.syllabic(),
        lyric: note.lyric.clone(),
    })
}

fn generate_nodes_for_tempo(bpm: f64) -> (Node, Node) {
    let sound = Node::new("sound").attr("tempo", decimal_from_float(bpm));
    let metronome = Node::new("metronome")
        .child(Node::new("beat-unit").text("quarter"))
        .child(Node::new("per-minute").text(py_float(bpm)));
    let direction = Node::new("direction").child(Node::new("direction-type").child(metronome));
    (sound, direction)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_part(part: &OutPart) -> Node {
    let mut part_node = Node::new("part").attr("id", part.id.clone());
    for measure in &part.measures {
        let mut measure_node = Node::new("measure").attr("number", measure.number.to_string());
        for content in &measure.contents {
            measure_node.children.push(match content {
                OutContent::Note(index) => render_note(&part.notes[*index]),
                OutContent::Rest { duration } => Node::new("note")
                    .child(Node::new("rest"))
                    .child(Node::new("duration").text(duration.to_string())),
            });
        }
        for direction in &measure.directions {
            measure_node.children.push(direction.clone());
        }
        measure_node.children.push(
            Node::new("attributes")
                .child(Node::new("divisions").text(DIVISIONS.to_string())),
        );
        if let Some((beats, beat_type)) = measure.time_signature {
            measure_node.children.push(
                Node::new("attributes").child(
                    Node::new("time")
                        .child(Node::new("beats").text(beats.to_string()))
                        .child(Node::new("beat-type").text(beat_type.to_string())),
                ),
            );
        }
        for sound in &measure.sounds {
            measure_node.children.push(sound.clone());
        }
        part_node.children.push(measure_node);
    }
    part_node
}

fn render_note(note: &OutNote) -> Node {
    let mut node = Node::new("note");
    if note.rest {
        node.children.push(Node::new("rest"));
        node.children
            .push(Node::new("duration").text(note.duration.to_string()));
        return node;
    }
    if let Some((step, alter, octave)) = &note.pitch {
        let mut pitch = Node::new("pitch").child(Node::new("step").text(step.clone()));
        if let Some(alter) = alter {
            pitch
                .children
                .push(Node::new("alter").text(alter.to_string()));
        }
        pitch
            .children
            .push(Node::new("octave").text(octave.to_string()));
        node.children.push(pitch);
    }
    if let Some(tie) = note.tie {
        node.children.push(Node::new("tie").attr("type", tie));
    }
    node.children
        .push(Node::new("duration").text(note.duration.to_string()));
    node.children
        .push(Node::new("voice").text(note.voice.to_string()));
    let mut notations = Node::new("notations");
    for notation in &note.notations {
        for tied in &notation.tied {
            notations
                .children
                .push(Node::new("tied").attr("type", *tied));
        }
        for (slur_type, number) in &notation.slurs {
            notations.children.push(
                Node::new("slur")
                    .attr("type", *slur_type)
                    .attr("number", number.to_string()),
            );
        }
    }
    if !note.notations.is_empty() {
        node.children.push(notations);
    }
    node.children.push(
        Node::new("lyric")
            .child(Node::new("syllabic").text(note.syllabic))
            .child(Node::new("text").text(note.lyric.clone())),
    );
    node
}

// ---------------------------------------------------------------------------
// Tick-rate application and measure construction
// ---------------------------------------------------------------------------

/// `apply_tick_rate` — doubles tempo positions and note times, dropping
/// instrumental tracks.
fn apply_tick_rate(project: &Project) -> (Vec<libresvip_core::model::base::SongTempo>, Vec<SingingTrack>) {
    let tempos = project
        .song_tempo_list
        .iter()
        .map(|tempo| libresvip_core::model::base::SongTempo::new(
            (tempo.position as f64 * DEFAULT_TICK_RATE_CEVIO) as i64,
            tempo.bpm,
        ))
        .collect();
    let tracks = project
        .singing_tracks()
        .map(|track| {
            let mut track = track.clone();
            for note in &mut track.note_list {
                note.start_pos = (note.start_pos as f64 * DEFAULT_TICK_RATE_CEVIO) as i64;
                note.length = (note.length as f64 * DEFAULT_TICK_RATE_CEVIO) as i64;
            }
            track
        })
        .collect();
    (tempos, tracks)
}

fn get_key_ticks(
    track_index: usize,
    track: &SingingTrack,
    tempos: &[libresvip_core::model::base::SongTempo],
) -> Vec<KeyTick> {
    let mut ticks: Vec<KeyTick> = Vec::new();
    for note in &track.note_list {
        ticks.push(KeyTick {
            tick: note.end_pos(),
            tempo: None,
            note_start: None,
            note_end: Some(note.clone()),
        });
    }
    if track_index == 0 {
        for tempo in tempos {
            ticks.push(KeyTick {
                tick: tempo.position,
                tempo: Some(tempo.bpm),
                note_start: None,
                note_end: None,
            });
        }
    }
    for note in &track.note_list {
        ticks.push(KeyTick {
            tick: note.start_pos,
            tempo: None,
            note_start: Some(note.clone()),
            note_end: None,
        });
    }
    ticks.sort_by_key(|tick| tick.tick);
    ticks
}

fn bar_length_ticks(signature: &TimeSignature, ticks_in_beat: i64) -> i64 {
    signature.bar_length_with(ticks_in_beat).round_ties_even() as i64
}

fn get_measures(key_ticks: &[KeyTick], time_signatures: &[TimeSignature]) -> Vec<Measure> {
    let ticks_in_beat = DIVISIONS;
    let mut measure_border_ticks: Vec<i64> = vec![0];
    let mut measure = 0.0f64;
    let mut tick = 0.0f64;
    let mut prev_time_signature = TimeSignature::default();
    for time_signature in time_signatures {
        let previous_measure = measure as i64;
        let ticks_in_measure = bar_length_ticks(&prev_time_signature, ticks_in_beat);
        tick += ticks_in_measure as f64 * (time_signature.bar_index as f64 - measure);
        measure = time_signature.bar_index as f64;
        let current_measure = measure as i64;
        for _ in 0..(current_measure - previous_measure).max(0) {
            let last_border = *measure_border_ticks.last().unwrap_or(&0);
            measure_border_ticks.push(last_border + ticks_in_measure);
        }
        prev_time_signature = time_signature.clone();
    }
    let Some(last_key_tick) = key_ticks.last().map(|tick| tick.tick) else {
        return Vec::new();
    };
    let ticks_in_measure = bar_length_ticks(&prev_time_signature, ticks_in_beat);
    if last_key_tick as f64 >= tick + ticks_in_measure as f64 {
        let previous_measure = measure as i64;
        let tick_diff = last_key_tick as f64 - tick;
        measure += tick_diff / ticks_in_measure as f64;
        let current_measure = measure as i64;
        for _ in 0..(current_measure - previous_measure).max(0) {
            let last_border = *measure_border_ticks.last().unwrap_or(&0);
            measure_border_ticks.push(last_border + ticks_in_measure);
        }
    }
    let last_border = *measure_border_ticks.last().unwrap_or(&0);
    measure_border_ticks.push(last_border + ticks_in_measure);

    let mut measures: Vec<Measure> = Vec::new();
    let mut ongoing_note: Option<(Note, i64)> = None;
    for (index, pair) in measure_border_ticks.windows(2).enumerate() {
        let (left, right) = (pair[0], pair[1]);
        let group: Vec<&KeyTick> = key_ticks
            .iter()
            .filter(|key_tick| {
                if key_tick.note_end.is_some() {
                    left < key_tick.tick && key_tick.tick <= right
                } else {
                    left <= key_tick.tick && key_tick.tick < right
                }
            })
            .collect();
        let mut current_tick_in_measure = 0i64;
        let mut contents: Vec<MeasureContent> = Vec::new();
        for key_tick in group {
            let relative = key_tick.tick - left;
            if relative > current_tick_in_measure {
                if ongoing_note.is_none() {
                    contents.push(MeasureContent::with_rest(
                        relative - current_tick_in_measure,
                    ));
                }
                current_tick_in_measure = relative;
            }
            if let Some(bpm) = key_tick.tempo {
                if let Some((note, head)) = ongoing_note.clone() {
                    let note_type = if note.start_pos == head {
                        NoteKind::Begin
                    } else {
                        NoteKind::Middle
                    };
                    contents.push(MeasureContent::with_note(
                        key_tick.tick - head,
                        note.clone(),
                        note_type,
                    ));
                    ongoing_note = Some((note, key_tick.tick));
                }
                contents.push(MeasureContent::with_tempo(bpm));
            } else if let Some(note_start) = &key_tick.note_start {
                ongoing_note = Some((note_start.clone(), key_tick.tick));
            } else if let (Some(note_end), Some((ongoing, head))) =
                (&key_tick.note_end, ongoing_note.clone())
            {
                let note_type = if ongoing.start_pos == head {
                    NoteKind::Single
                } else {
                    NoteKind::End
                };
                contents.push(MeasureContent::with_note(
                    note_end.end_pos() - head,
                    note_end.clone(),
                    note_type,
                ));
                ongoing_note = None;
            }
        }
        let rest_length = right - left - current_tick_in_measure;
        if rest_length > 0 {
            match &ongoing_note {
                None => contents.push(MeasureContent::with_rest(rest_length)),
                Some((note, head)) => {
                    let note_type = if note.start_pos == *head {
                        NoteKind::Begin
                    } else {
                        NoteKind::Middle
                    };
                    contents.push(MeasureContent::with_note(
                        right - *head,
                        note.clone(),
                        note_type,
                    ));
                    ongoing_note = Some((note.clone(), right));
                }
            }
        }
        measures.push(Measure {
            tick_start: left,
            length: right - left,
            time_signature: time_signatures
                .iter()
                .find(|signature| signature.bar_index == index as i64)
                .cloned(),
            contents,
        });
    }
    measures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_names() {
        assert_eq!(midi2note(60.0), "C4");
        assert_eq!(midi2note(64.0), "E4");
        assert_eq!(midi2note(73.0), "C#5");
    }
}
