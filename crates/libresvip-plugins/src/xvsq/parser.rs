//! Parser for Cadencii project files (`cadencii_parser.py`).

use std::path::Path;

use libresvip_core::core::tick_counter::{skip_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};

use crate::vsqx::vocaloid::{
    convert_vocaloid_curve_to_param_points, py_round, ControllerCurve, ControllerEvent,
    PitchBendData, VocaloidPitchHandler,
};
use crate::vsqx::xml::{
    child, child_text, children, first_int, parse_bool, parse_float, parse_int, parse_xml,
};

/// `InputOptions` of the xvsq plugin.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_breath: bool,
    pub import_gender: bool,
    pub import_strength: bool,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_instrumental_track: true,
            import_pitch: true,
            import_volume: true,
            import_breath: true,
            import_gender: true,
            import_strength: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VsqIdType {
    Singer,
    Anote,
    Aicon,
    Unknown,
}

impl VsqIdType {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "Singer" => Ok(Self::Singer),
            "Anote" => Ok(Self::Anote),
            "Aicon" => Ok(Self::Aicon),
            "Unknown" => Ok(Self::Unknown),
            other => Err(LibreSvipError::Parse(format!(
                "invalid VsqIDType value: {other}"
            ))),
        }
    }
}

/// `VibratoBPList.data` decoded through the `points` property.
#[derive(Clone, Debug, Default)]
pub struct BpList {
    pub points: Vec<(f64, i64)>,
}

impl BpList {
    fn parse(data: &str) -> Result<Self> {
        let mut points = Vec::new();
        for item in data.split(',') {
            let (x, _, y) = partition(item, '=');
            if !x.is_empty() && !y.is_empty() {
                let x_value = x.trim().parse::<f64>().map_err(|_| {
                    LibreSvipError::msg(format!("could not convert string to float: {x:?}"))
                })?;
                let y_value = y.trim().parse::<i64>().map_err(|_| {
                    LibreSvipError::msg(format!("invalid literal for int(): {y:?}"))
                })?;
                points.push((x_value, y_value));
            }
        }
        Ok(Self { points })
    }

    /// `_set_points`: `",".join(f"{x}={y}")`.
    pub fn to_data(points: &[(f64, i64)]) -> String {
        points
            .iter()
            .map(|(x, y)| format!("{x}={y}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn partition(value: &str, separator: char) -> (&str, &str, &str) {
    match value.find(separator) {
        Some(index) => (
            &value[..index],
            &value[index..index + separator.len_utf8()],
            &value[index + separator.len_utf8()..],
        ),
        None => (value, "", ""),
    }
}

#[derive(Clone, Debug)]
pub struct VsqId {
    pub type_value: VsqIdType,
    pub icon_handle_ids: Option<String>,
    pub note: i64,
    pub length: i64,
    pub lyric_phrase: Option<String>,
    pub phonetic_symbol_protected: bool,
    pub phonetic_symbol: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VsqEvent {
    pub clock: i64,
    pub id: VsqId,
}

#[derive(Clone, Debug)]
pub struct MetaText {
    pub name: String,
    pub events: Vec<VsqEvent>,
    pub pit: BpList,
    pub pbs: BpList,
    pub dyn_list: BpList,
    pub bre: BpList,
    pub bri: BpList,
    pub gen: BpList,
}

#[derive(Clone, Debug)]
pub struct VsqTrack {
    pub meta_text: Option<MetaText>,
}

#[derive(Clone, Debug)]
pub struct TempoTableEntry {
    pub clock: i64,
    pub tempo: i64,
    pub time: i64,
}

#[derive(Clone, Debug)]
pub struct TimeSigTableEntry {
    pub clock: i64,
    pub numerator: i64,
    pub denominator: i64,
    pub bar_count: i64,
}

#[derive(Clone, Debug)]
pub struct BgmFile {
    pub file: String,
    pub mute: i64,
    pub start_after_premeasure: bool,
    pub read_offset_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct MixerEntry {
    pub mute: i64,
    pub solo: i64,
}

#[derive(Clone, Debug)]
pub struct VsqFileEx {
    pub tracks: Vec<VsqTrack>,
    pub tempo_table: Vec<TempoTableEntry>,
    pub timesig_table: Vec<TimeSigTableEntry>,
    pub pre_measure: i64,
    pub mixer_entries: Vec<MixerEntry>,
    pub bgm_files: Vec<BgmFile>,
}

fn parse_bp_list(node: Option<roxmltree::Node<'_, '_>>) -> Result<BpList> {
    let Some(node) = node else {
        return Ok(BpList::default());
    };
    let data = child_text(node, "Data").unwrap_or_default();
    BpList::parse(&data)
}

fn read_icon_handle_ids(node: roxmltree::Node<'_, '_>) -> Option<String> {
    child(node, "IconHandle").map(|handle| child_text(handle, "IDS").unwrap_or_default())
}

fn read_vsq_id(node: roxmltree::Node<'_, '_>) -> Result<VsqId> {
    let type_text = child_text(node, "type").unwrap_or_default();
    let type_value = VsqIdType::parse(&type_text)?;
    let lyric_phrase = child(node, "LyricHandle")
        .and_then(|handle| child(handle, "L0"))
        .and_then(|l0| child_text(l0, "Phrase"));
    let phonetic_symbol = child(node, "LyricHandle")
        .and_then(|handle| child(handle, "L0"))
        .and_then(|l0| child_text(l0, "PhoneticSymbol"));
    let phonetic_symbol_protected = child(node, "LyricHandle")
        .and_then(|handle| child(handle, "L0"))
        .and_then(|l0| child_text(l0, "PhoneticSymbolProtected"))
        .and_then(|value| parse_bool(Some(value)))
        .unwrap_or(false);
    Ok(VsqId {
        type_value,
        icon_handle_ids: read_icon_handle_ids(node),
        note: child_text(node, "Note")
            .and_then(|value| parse_int(Some(value)))
            .unwrap_or(0),
        length: child_text(node, "Length")
            .and_then(|value| parse_int(Some(value)))
            .unwrap_or(0),
        lyric_phrase,
        phonetic_symbol_protected,
        phonetic_symbol,
    })
}

fn read_events(node: roxmltree::Node<'_, '_>) -> Result<Vec<VsqEvent>> {
    // `MetaText/Events/Events/VsqEvent`
    let Some(list) = child(node, "Events") else {
        return Ok(Vec::new());
    };
    let Some(list) = child(list, "Events") else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
    for event in children(list, "VsqEvent") {
        let clock = child_text(event, "Clock")
            .and_then(|value| parse_int(Some(value)))
            .unwrap_or(0);
        let Some(id) = child(event, "ID") else {
            return Err(LibreSvipError::Parse("VsqEvent without ID".to_owned()));
        };
        events.push(VsqEvent {
            clock,
            id: read_vsq_id(id)?,
        });
    }
    Ok(events)
}

fn read_meta_text(node: roxmltree::Node<'_, '_>) -> Result<MetaText> {
    let common = child(node, "Common");
    let name = common
        .and_then(|common| child_text(common, "Name"))
        .unwrap_or_default();
    Ok(MetaText {
        name,
        events: read_events(node)?,
        pit: parse_bp_list(child(node, "PIT"))?,
        pbs: parse_bp_list(child(node, "PBS"))?,
        dyn_list: parse_bp_list(child(node, "DYN"))?,
        bre: parse_bp_list(child(node, "BRE"))?,
        bri: parse_bp_list(child(node, "BRI"))?,
        gen: parse_bp_list(child(node, "GEN"))?,
    })
}

fn read_file(root: roxmltree::Node<'_, '_>) -> Result<VsqFileEx> {
    let mut tracks = Vec::new();
    if let Some(track_list) = child(root, "Track") {
        for track in children(track_list, "VsqTrack") {
            let meta_text = match child(track, "MetaText") {
                Some(meta_text) => Some(read_meta_text(meta_text)?),
                None => None,
            };
            tracks.push(VsqTrack { meta_text });
        }
    }
    let tempo_table = child(root, "TempoTable")
        .map(|table| {
            children(table, "TempoTableEntry")
                .map(|entry| TempoTableEntry {
                    clock: first_int(entry, &["Clock"]).unwrap_or(0),
                    tempo: first_int(entry, &["Tempo"]).unwrap_or(0),
                    time: first_int(entry, &["Time"]).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    let timesig_table = child(root, "TimesigTable")
        .map(|table| {
            children(table, "TimeSigTableEntry")
                .map(|entry| TimeSigTableEntry {
                    clock: first_int(entry, &["Clock"]).unwrap_or(0),
                    numerator: first_int(entry, &["Numerator"]).unwrap_or(0),
                    denominator: first_int(entry, &["Denominator"]).unwrap_or(0),
                    bar_count: first_int(entry, &["BarCount"]).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    let pre_measure = child(root, "Master")
        .and_then(|master| first_int(master, &["PreMeasure"]))
        .unwrap_or(1);
    let mixer_entries = child(root, "Mixer")
        .and_then(|mixer| child(mixer, "Slave"))
        .map(|slave| {
            children(slave, "VsqMixerEntry")
                .map(|entry| MixerEntry {
                    mute: first_int(entry, &["Mute"]).unwrap_or(0),
                    solo: first_int(entry, &["Solo"]).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    let bgm_files = child(root, "BgmFiles")
        .map(|files| {
            children(files, "BgmFile")
                .map(|file| BgmFile {
                    // No explicit XML names in the Python model: the element
                    // names are the field names.
                    file: child_text(file, "file").unwrap_or_default(),
                    mute: first_int(file, &["mute"]).unwrap_or(0),
                    start_after_premeasure: child_text(file, "startAfterPremeasure")
                        .and_then(|value| parse_bool(Some(value)))
                        .unwrap_or(true),
                    read_offset_seconds: child_text(file, "readOffsetSeconds")
                        .and_then(|value| parse_float(Some(value)))
                        .unwrap_or(0.0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(VsqFileEx {
        tracks,
        tempo_table,
        timesig_table,
        pre_measure,
        mixer_entries,
        bgm_files,
    })
}

struct Parser<'a> {
    options: &'a InputOptions,
    time_synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
}

/// `CadenciiParser.parse_project`.
pub fn parse_project(bytes: &[u8], options: &InputOptions) -> Result<Project> {
    let document = parse_xml(bytes)?;
    let root = document.root_element();
    if root.tag_name().name() != "VsqFileEx" {
        return Err(LibreSvipError::UnsupportedProjectVersion(format!(
            "XVXQ root element {}",
            root.tag_name().name()
        )));
    }
    let file = read_file(root)?;
    let (tick_prefix, time_signatures) = parse_time_signatures(&file)?;
    let first_bar_length = time_signatures
        .first()
        .map(|signature| signature.bar_length() as i64)
        .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
    let tempos = parse_tempos(&file, tick_prefix);
    let time_synchronizer = TimeSynchronizer::from_project(&tempos);
    let parser = Parser {
        options,
        time_synchronizer,
        first_bar_length,
        time_signatures,
    };
    let mut track_list: Vec<Track> = Vec::new();
    let track_slice = if file.tracks.len() > 1 {
        &file.tracks[1..]
    } else {
        &[]
    };
    for (track, mixer_entry) in track_slice.iter().zip(file.mixer_entries.iter()) {
        let Some(meta_text) = &track.meta_text else {
            continue;
        };
        let mut singing_track = SingingTrack::new(meta_text.name.clone());
        singing_track.solo = mixer_entry.solo == 1;
        singing_track.mute = mixer_entry.mute == 1;
        let (singer_name, notes) = parse_notes(&meta_text.events, tick_prefix);
        singing_track.ai_singer_name = singer_name;
        singing_track.note_list = notes;
        if parser.options.import_pitch {
            if let Some(pitch) =
                parser.parse_pitch(track, &singing_track.note_list, tick_prefix)?
            {
                singing_track.edited_params.pitch = pitch;
            }
        }
        parser.parse_params(track, &mut singing_track.edited_params, tick_prefix);
        track_list.push(Track::Singing(singing_track));
    }
    track_list.extend(parser.parse_instrumental_tracks(&file.bgm_files, tick_prefix));
    Ok(Project {
        version: String::new(),
        song_tempo_list: tempos,
        time_signature_list: parser.time_signatures,
        track_list,
    })
}

fn parse_time_signatures(file: &VsqFileEx) -> Result<(i64, Vec<TimeSignature>)> {
    let mut bar_index = -file.pre_measure;
    let mut time_signature_list: Vec<TimeSignature> = Vec::new();
    for entry in &file.timesig_table {
        bar_index += entry.bar_count;
        time_signature_list.push(TimeSignature {
            bar_index,
            numerator: entry.numerator,
            denominator: entry.denominator,
        });
    }
    if time_signature_list.is_empty() {
        return Err(LibreSvipError::msg("time_signature_list must not be empty"));
    }
    let mut tick_prefix = 0i64;
    let mut measure = 0i64;
    for time_sig in &time_signature_list {
        let measure_diff = time_sig.bar_index - measure;
        tick_prefix += measure_diff * py_round(time_sig.bar_length());
        measure += time_sig.bar_index;
    }
    let last = time_signature_list
        .last()
        .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
    let measure_diff = file.pre_measure - measure;
    tick_prefix += measure_diff * py_round(last.bar_length());
    Ok((
        tick_prefix,
        skip_beat_list(&time_signature_list, file.pre_measure),
    ))
}

fn parse_tempos(file: &VsqFileEx, tick_prefix: i64) -> Vec<SongTempo> {
    let tempo_list: Vec<SongTempo> = file
        .tempo_table
        .iter()
        .map(|entry| SongTempo::new(entry.time, 60_000_000.0 / entry.tempo as f64))
        .collect();
    skip_tempo_list(&tempo_list, tick_prefix)
}

fn parse_notes(events: &[VsqEvent], tick_prefix: i64) -> (String, Vec<Note>) {
    let mut singer_name = String::new();
    let mut note_list = Vec::new();
    for event in events {
        if event.id.type_value == VsqIdType::Singer && event.id.icon_handle_ids.is_some() {
            singer_name = event.id.icon_handle_ids.clone().unwrap_or_default();
        } else if event.id.type_value == VsqIdType::Anote {
            if event.clock < tick_prefix {
                continue;
            }
            let mut note = Note::new(
                event.clock - tick_prefix,
                event.id.length,
                event.id.note,
                "",
            );
            if let Some(phrase) = &event.id.lyric_phrase {
                note.lyric = phrase.clone();
                if event.id.phonetic_symbol_protected {
                    note.pronunciation = event.id.phonetic_symbol.clone();
                }
            }
            note_list.push(note);
        }
    }
    (singer_name, note_list)
}

impl Parser<'_> {
    fn bp_list_points(node: &BpList, tick_prefix: i64) -> Vec<ControllerEvent> {
        node.points
            .iter()
            .map(|(x, y)| ControllerEvent {
                pos: *x as i64 - tick_prefix,
                value: *y,
            })
            .collect()
    }

    fn parse_pitch(
        &self,
        track: &VsqTrack,
        note_list: &[Note],
        tick_prefix: i64,
    ) -> Result<Option<libresvip_core::model::base::ParamCurve>> {
        let mut pit: Vec<ControllerEvent> = Vec::new();
        let mut pbs: Vec<ControllerEvent> = Vec::new();
        if let Some(meta_text) = &track.meta_text {
            pit.extend(Self::bp_list_points(&meta_text.pit, tick_prefix));
            pbs.extend(Self::bp_list_points(&meta_text.pbs, tick_prefix));
        }
        let pitch_data = PitchBendData {
            pit: ControllerCurve::with_events("pitch_bend", pit, 0, -8192, 8191),
            pbs: ControllerCurve::with_events("pitch_bend_sens", pbs, 2, 1, 24),
        };
        let handler = VocaloidPitchHandler::new(
            self.time_synchronizer.clone(),
            note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        handler.to_absolute_pitch(&[pitch_data], Some(&[0]), None, None, None, None)
    }

    fn extract_curve(&self, meta_text: &MetaText, param_name: &str, tick_prefix: i64) -> Option<ControllerCurve> {
        let (default_value, min_value, max_value) = match param_name {
            "dynamics" => (64, 0, 127),
            "breathiness" => (0, 0, 127),
            "brightness" => (0, 0, 127),
            "gender" => (64, 0, 127),
            _ => {
                let definition = crate::vsqx::vocaloid::get_param_def(param_name)?;
                (definition.default_value, definition.min_value, definition.max_value)
            }
        };
        let bplist = match param_name {
            "dynamics" => &meta_text.dyn_list,
            "breathiness" => &meta_text.bre,
            "brightness" => &meta_text.bri,
            "gender" => &meta_text.gen,
            _ => return None,
        };
        if bplist.points.is_empty() {
            return None;
        }
        let events = bplist
            .points
            .iter()
            .map(|(x, y)| ControllerEvent {
                pos: *x as i64 - tick_prefix,
                value: *y,
            })
            .collect();
        Some(ControllerCurve::with_events(
            param_name,
            events,
            default_value,
            min_value,
            max_value,
        ))
    }

    fn parse_params(
        &self,
        track: &VsqTrack,
        params: &mut libresvip_core::model::base::Params,
        tick_prefix: i64,
    ) {
        let Some(meta_text) = &track.meta_text else {
            return;
        };
        if self.options.import_volume {
            if let Some(curve) = self.extract_curve(meta_text, "dynamics", tick_prefix) {
                params.volume.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    self.first_bar_length,
                    false,
                ));
            }
        }
        if self.options.import_breath {
            if let Some(curve) = self.extract_curve(meta_text, "breathiness", tick_prefix) {
                params.breath.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    self.first_bar_length,
                    false,
                ));
            }
        }
        if self.options.import_gender {
            if let Some(curve) = self.extract_curve(meta_text, "gender", tick_prefix) {
                params.gender.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    self.first_bar_length,
                    true,
                ));
            }
        }
        if self.options.import_strength {
            if let Some(curve) = self.extract_curve(meta_text, "brightness", tick_prefix) {
                params.strength.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    self.first_bar_length,
                    false,
                ));
            }
        }
    }

    fn parse_instrumental_tracks(
        &self,
        bgm_files: &[BgmFile],
        tick_prefix: i64,
    ) -> Vec<Track> {
        let mut tracks = Vec::new();
        if !self.options.import_instrumental_track {
            return tracks;
        }
        for bgm_file in bgm_files {
            if bgm_file.file.is_empty() {
                continue;
            }
            let offset_ticks = self.time_synchronizer.get_actual_ticks_from_secs_offset(
                if bgm_file.start_after_premeasure {
                    0
                } else {
                    -tick_prefix
                },
                bgm_file.read_offset_seconds,
            );
            let title = Path::new(&bgm_file.file)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut track = InstrumentalTrack::new(title, bgm_file.file.clone());
            track.offset = offset_ticks as i64;
            track.mute = bgm_file.mute == 1;
            tracks.push(Track::Instrumental(track));
        }
        tracks
    }
}
