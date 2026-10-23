//! UFData plugin — port of `libresvip.plugins.ufdata`.
//!
//! [UtaFormatix3](https://sdercolin.github.io/utaformatix3/)'s JSON exchange
//! format. The model mirrors `model.py` (pydantic aliases and field order are
//! part of the wire format) and the dump is written with Python's
//! `json.dumps(..., ensure_ascii=False)` separators, i.e. `", "` / `": "`.

use std::io;
use std::path::Path;

use libresvip_core::core::tick_counter::{shift_beat_list, shift_tempo_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{IoContext, LibreSvipError, Result};
use libresvip_core::model::base::{
    Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature,
};
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};
use libresvip_core::options::{enable_pitch_importation, OptionFieldDef};
use libresvip_core::plugin::{PluginInfo, SvsPlugin};
use libresvip_core::utils::text::to_unicode;
use serde::{Deserialize, Serialize};

use crate::mid::pitch::{PitchSimulator, Portamento, RelativePitchCurve};
use crate::plugin_infos;

pub struct UfDataPlugin;

pub static PLUGIN: UfDataPlugin = UfDataPlugin;

#[allow(dead_code)]
fn _info() -> &'static PluginInfo {
    &plugin_infos::INFO_UFDATA
}

// ---------------------------------------------------------------------------
// Model (`libresvip/plugins/ufdata/model.py`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfNote {
    key: i64,
    #[serde(rename = "tickOn", alias = "tick_on")]
    tick_on: i64,
    #[serde(rename = "tickOff", alias = "tick_off")]
    tick_off: i64,
    lyric: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfPitch {
    #[serde(default)]
    ticks: Vec<i64>,
    #[serde(default)]
    values: Vec<Option<f64>>,
    #[serde(rename = "isAbsolute", alias = "is_absolute")]
    is_absolute: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfTempo {
    #[serde(rename = "tickPosition", alias = "tick_position")]
    tick_position: i64,
    bpm: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfTimeSignature {
    #[serde(rename = "measurePosition", alias = "measure_position")]
    measure_position: i64,
    #[serde(default = "default_numerator")]
    numerator: i64,
    #[serde(default = "default_denominator")]
    denominator: i64,
}

fn default_numerator() -> i64 {
    4
}

fn default_denominator() -> i64 {
    4
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfTrack {
    name: String,
    #[serde(default)]
    notes: Vec<UfNote>,
    #[serde(default)]
    pitch: Option<UfPitch>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfProject {
    #[serde(default = "default_name")]
    name: String,
    #[serde(default)]
    tracks: Vec<UfTrack>,
    #[serde(
        rename = "timeSignatures",
        alias = "time_signatures",
        default
    )]
    time_signatures: Vec<UfTimeSignature>,
    #[serde(default)]
    tempos: Vec<UfTempo>,
    #[serde(rename = "measurePrefix", alias = "measure_prefix")]
    measure_prefix: i64,
}

fn default_name() -> String {
    "export".to_owned()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UfData {
    #[serde(
        rename = "formatVersion",
        alias = "format_version",
        default = "default_format_version"
    )]
    format_version: i64,
    project: UfProject,
}

fn default_format_version() -> i64 {
    1
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

impl SvsPlugin for UfDataPlugin {
    fn info(&self) -> &'static PluginInfo {
        &plugin_infos::INFO_UFDATA
    }

    fn version(&self) -> &'static str {
        plugin_infos::INFO_UFDATA_VERSION
    }

    fn input_options(&self) -> Vec<OptionFieldDef> {
        vec![enable_pitch_importation()]
    }

    fn output_options(&self) -> Vec<OptionFieldDef> {
        Vec::new()
    }

    fn load(&self, path: &Path, options: &libresvip_core::options::Options) -> Result<Project> {
        let bytes = libresvip_core::io::read(path).at_path(path)?;
        let text = to_unicode(&bytes);
        let data: UfData = serde_json::from_str(&text)?;
        parse_project(&data, options.bool("import_pitch", true))
    }

    fn dump(
        &self,
        path: &Path,
        project: &Project,
        _options: &libresvip_core::options::Options,
    ) -> Result<()> {
        let data = generate_project(project)?;
        let bytes = to_python_json(&data)?;
        libresvip_core::io::write(path, &bytes).at_path(path)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Parser (`ufdata_parser.py`)
// ---------------------------------------------------------------------------

fn parse_project(data: &UfData, import_pitch: bool) -> Result<Project> {
    let uf_project = &data.project;
    let time_signatures = parse_time_signatures(&uf_project.time_signatures);
    let first_bar_length = round_to_i64(time_signatures[0].bar_length());
    let tick_prefix =
        (time_signatures[0].bar_length() * uf_project.measure_prefix as f64) as i64;
    let song_tempo_list = shift_tempo_list(&parse_tempos(&uf_project.tempos), tick_prefix);
    let synchronizer = TimeSynchronizer::from_project(&song_tempo_list);
    let track_list = parse_tracks(
        &uf_project.tracks,
        tick_prefix,
        first_bar_length,
        &synchronizer,
        &time_signatures,
        import_pitch,
    )?;
    Ok(Project {
        version: String::new(),
        song_tempo_list,
        time_signature_list: shift_beat_list(&time_signatures, uf_project.measure_prefix),
        track_list: track_list.into_iter().map(Into::into).collect(),
    })
}

fn parse_tempos(tempos: &[UfTempo]) -> Vec<SongTempo> {
    tempos
        .iter()
        .map(|tempo| SongTempo::new(tempo.tick_position, tempo.bpm))
        .collect()
}

fn parse_time_signatures(time_signatures: &[UfTimeSignature]) -> Vec<TimeSignature> {
    let mut result: Vec<TimeSignature> = time_signatures
        .iter()
        .map(|time_signature| {
            TimeSignature::new(
                time_signature.measure_position,
                time_signature.numerator,
                time_signature.denominator,
            )
        })
        .collect();
    if result.is_empty() {
        result.push(TimeSignature::new(0, 4, 4));
    }
    result
}

fn parse_tracks(
    tracks: &[UfTrack],
    tick_prefix: i64,
    first_bar_length: i64,
    synchronizer: &TimeSynchronizer,
    time_signatures: &[TimeSignature],
    import_pitch: bool,
) -> Result<Vec<SingingTrack>> {
    let mut track_list = Vec::with_capacity(tracks.len());
    for track in tracks {
        let mut singing_track = SingingTrack::new(track.name.clone());
        singing_track.note_list = parse_notes(&track.notes, tick_prefix);
        if import_pitch {
            if let Some(pitch) = &track.pitch {
                singing_track.edited_params.pitch = parse_pitch(
                    pitch,
                    &singing_track.note_list,
                    tick_prefix,
                    first_bar_length,
                    synchronizer,
                    time_signatures,
                )?;
            }
        }
        track_list.push(singing_track);
    }
    Ok(track_list)
}

fn parse_pitch(
    pitch: &UfPitch,
    note_list: &[Note],
    tick_prefix: i64,
    first_bar_length: i64,
    synchronizer: &TimeSynchronizer,
    time_signatures: &[TimeSignature],
) -> Result<ParamCurve> {
    if pitch.is_absolute {
        let mut pitch_points: Vec<Point> = vec![Point::start_point()];
        let mut prev_point = Point::start_point();
        let mut point: Option<Point> = None;
        for (tick, value) in pitch.ticks.iter().zip(pitch.values.iter()) {
            let value = *value;
            let x = tick + tick_prefix + first_bar_length;
            if value == Some(0.0) && prev_point.y == -100 {
                pitch_points.push(Point::new(x, -100));
            }
            if let Some(value) = value {
                point = Some(Point::new(x, round_to_i64(value * 100.0)));
            }
            if let Some(point) = point {
                pitch_points.push(point);
            }
            if value == Some(0.0) && prev_point.y != -100 {
                pitch_points.push(Point::new(x, -100));
            }
            if let Some(point) = point {
                prev_point = point;
            }
        }
        if prev_point.y == 0 {
            pitch_points.push(Point::new(prev_point.x, -100));
        }
        pitch_points.push(Point::end_point());
        return Ok(ParamCurve::new(pitch_points));
    }
    let rel_pitch_points: Vec<Point> = pitch
        .ticks
        .iter()
        .zip(pitch.values.iter())
        .filter_map(|(tick, value)| {
            value.map(|value| Point::new(tick + tick_prefix, round_to_i64(value * 100.0)))
        })
        .collect();
    let pitch_simulator = PitchSimulator::new(
        synchronizer,
        Portamento::no_portamento(),
        note_list,
        time_signatures,
    )?;
    RelativePitchCurve::new(first_bar_length).to_absolute(&rel_pitch_points, &pitch_simulator)
}

fn parse_notes(notes: &[UfNote], tick_prefix: i64) -> Vec<Note> {
    notes
        .iter()
        .map(|note| {
            Note::new(
                note.tick_on + tick_prefix,
                note.tick_off - note.tick_on,
                note.key,
                note.lyric.clone(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Generator (`ufdata_generator.py`)
// ---------------------------------------------------------------------------

fn generate_project(project: &Project) -> Result<UfData> {
    let Some(first_time_signature) = project.time_signature_list.first() else {
        // Python would raise IndexError on `project.time_signature_list[0]`.
        return Err(LibreSvipError::msg("project has no time signature"));
    };
    let first_bar_length = round_to_i64(first_time_signature.bar_length());
    let tempos = skip_tempo_list(&project.song_tempo_list, first_bar_length)
        .into_iter()
        .map(|tempo| UfTempo {
            tick_position: tempo.position,
            bpm: tempo.bpm,
        })
        .collect();
    let time_signatures = project
        .time_signature_list
        .iter()
        .map(|time_signature| UfTimeSignature {
            measure_position: time_signature.bar_index,
            numerator: time_signature.numerator,
            denominator: time_signature.denominator,
        })
        .collect();
    let tracks = project
        .track_list
        .iter()
        .filter_map(|track| track.as_singing())
        .map(|track| UfTrack {
            name: track.title.clone(),
            notes: generate_notes(&track.note_list),
            pitch: Some(generate_pitch(
                &track.edited_params.pitch,
                &track.note_list,
                first_bar_length,
            )),
        })
        .collect();
    Ok(UfData {
        format_version: 1,
        project: UfProject {
            name: "export".to_owned(),
            tracks,
            time_signatures,
            tempos,
            measure_prefix: 0,
        },
    })
}

fn generate_notes(note_list: &[Note]) -> Vec<UfNote> {
    note_list
        .iter()
        .map(|note| UfNote {
            key: note.key_number,
            tick_on: note.start_pos,
            tick_off: note.end_pos(),
            lyric: note.lyric.clone(),
        })
        .collect()
}

fn generate_pitch(pitch: &ParamCurve, notes: &[Note], first_bar_length: i64) -> UfPitch {
    let mut uf_pitch = UfPitch {
        ticks: Vec::new(),
        values: Vec::new(),
        is_absolute: true,
    };
    if !notes.is_empty() {
        for point in &pitch.points {
            if point.x != START_POINT_X && point.x != END_POINT_X {
                uf_pitch.ticks.push(point.x - first_bar_length);
                uf_pitch.values.push(Some(if point.y == -100 {
                    0.0
                } else {
                    point.y as f64 / 100.0
                }));
            }
        }
    }
    uf_pitch
}

// ---------------------------------------------------------------------------
// Python-compatible JSON
// ---------------------------------------------------------------------------

/// Python's `json.dumps` defaults (`ensure_ascii=False`), i.e. `", "` and `": "`.
struct PythonSpacingFormatter;

impl serde_json::ser::Formatter for PythonSpacingFormatter {
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

fn to_python_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut output, PythonSpacingFormatter);
    value.serialize(&mut serializer)?;
    Ok(output)
}

/// Python's `round()` (ties to even) as `i64`.
fn round_to_i64(value: f64) -> i64 {
    value.round_ties_even() as i64
}
