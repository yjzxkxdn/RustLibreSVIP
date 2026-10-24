//! Parser for VSQX/VSQ3 files (`vsqx_parser.py`).

use std::path::Path;

use libresvip_core::core::constants::DEFAULT_ENGLISH_LYRIC;
use libresvip_core::core::tick_counter::{skip_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;

use super::vocaloid::{
    convert_vocaloid_curve_to_param_points, py_round, ControllerCurve, ControllerEvent,
    PitchBendData, VocaloidPitchHandler, DEFAULT_PITCH_BEND_SENSITIVITY,
    MAX_PITCH_BEND_SENSITIVITY,
};
use super::xml::{
    child, child_text, children, first_int, first_text, node_text, parse_int, parse_xml,
};
use super::BPM_RATE;

pub const DEFAULT_CHINESE_PHONEME: &str = "l a";
pub const DEFAULT_JAPANESE_PHONEME: &str = "4 a";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VsqxVersion {
    Vsq3,
    Vsq4,
}

impl VsqxVersion {
    fn from_root_name(name: &str) -> Option<Self> {
        match name {
            "vsq3" => Some(Self::Vsq3),
            "vsq4" => Some(Self::Vsq4),
            _ => None,
        }
    }

    /// `ParameterNames[vsq_name].value` for this file version.
    pub fn param_id<'a>(self, vsq_name: &'a str) -> &'a str {
        match (self, vsq_name) {
            (Self::Vsq4, "PIT") => "P",
            (Self::Vsq4, "PBS") => "S",
            (Self::Vsq4, "DYN") => "D",
            (Self::Vsq4, "BRE") => "B",
            (Self::Vsq4, "BRI") => "R",
            (Self::Vsq4, "CLE") => "C",
            (Self::Vsq4, "GEN") => "G",
            (Self::Vsq4, "POR") => "T",
            (Self::Vsq4, "XSY") => "X",
            (Self::Vsq4, "GWL") => "W",
            (Self::Vsq3, other) => other,
            (Self::Vsq4, other) => other,
        }
    }
}

/// `InputOptions` of the vsqx plugin.
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_instrumental_track: bool,
    pub import_pitch: bool,
    pub import_volume: bool,
    pub import_breath: bool,
    pub import_gender: bool,
    pub import_strength: bool,
    pub combine_syllables: bool,
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
            combine_syllables: false,
        }
    }
}

#[derive(Clone, Debug)]
struct VsqxNote {
    pos_tick: i64,
    dur_tick: i64,
    note_num: i64,
    lyric: String,
    phnms: Option<(String, Option<i64>)>,
    seq_attrs: Vec<SeqAttr>,
}

#[derive(Clone, Debug)]
struct SeqAttr {
    seq_id: String,
    elems: Vec<(i64, i64)>,
}

#[derive(Clone, Debug)]
struct MCtrl {
    pos_tick: i64,
    attr_id: Option<String>,
    value: Option<i64>,
}

#[derive(Clone, Debug)]
struct MusicalPart {
    pos_tick: i64,
    play_time: i64,
    notes: Vec<VsqxNote>,
    m_ctrls: Vec<MCtrl>,
    v_pc: Option<i64>,
}

#[derive(Clone, Debug)]
struct VsTrack {
    track_name: String,
    parts: Vec<MusicalPart>,
}

#[derive(Clone, Debug)]
struct WavPart {
    pos_tick: i64,
    part_name: String,
    file_path: String,
}

#[derive(Clone, Debug)]
struct WavUnit {
    mute: bool,
    solo: bool,
}

fn bracket_names(version: VsqxVersion, v3: &'static str, v4: &'static str) -> &'static str {
    match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    }
}

fn read_time_signatures(
    node: roxmltree::Node<'_, '_>,
    version: VsqxVersion,
) -> Vec<TimeSignature> {
    let pos_name = bracket_names(version, "posMes", "m");
    let nume_name = bracket_names(version, "nume", "nu");
    let denomi_name = bracket_names(version, "denomi", "de");
    children(node, "timeSig")
        .map(|time_sig| TimeSignature {
            bar_index: first_int(time_sig, &[pos_name]).unwrap_or(0),
            numerator: first_int(time_sig, &[nume_name]).unwrap_or(0),
            denominator: first_int(time_sig, &[denomi_name]).unwrap_or(0),
        })
        .collect()
}

fn read_tempos(node: roxmltree::Node<'_, '_>, version: VsqxVersion) -> Vec<SongTempo> {
    let pos_name = bracket_names(version, "posTick", "t");
    let bpm_name = bracket_names(version, "bpm", "v");
    children(node, "tempo")
        .map(|tempo| SongTempo {
            position: first_int(tempo, &[pos_name]).unwrap_or(0),
            bpm: first_int(tempo, &[bpm_name]).unwrap_or(0) as f64,
        })
        .collect()
}

fn read_seq_attrs(note: roxmltree::Node<'_, '_>, version: VsqxVersion) -> Vec<SeqAttr> {
    let Some(style) = child(
        note,
        bracket_names(version, "noteStyle", "nStyle"),
    ) else {
        return Vec::new();
    };
    let seq_name = bracket_names(version, "seqAttr", "seq");
    let mut result = Vec::new();
    for seq in children(style, seq_name) {
        let Some(seq_id) = seq.attribute("id").map(str::to_owned) else {
            continue;
        };
        let elems: Vec<(i64, i64)> = if version == VsqxVersion::Vsq3 {
            seq.children()
                .filter(|node| node.is_element())
                .filter_map(|elem| {
                    let pos = first_int(elem, &["posNrm"])?;
                    let elv = first_int(elem, &["elv"]).unwrap_or(0);
                    Some((pos, elv))
                })
                .collect()
        } else {
            children(seq, "cc")
                .map(|elem| {
                    (
                        first_int(elem, &["p"]).unwrap_or(0),
                        first_int(elem, &["v"]).unwrap_or(0),
                    )
                })
                .collect()
        };
        result.push(SeqAttr { seq_id, elems });
    }
    result
}

fn read_notes(part: roxmltree::Node<'_, '_>, version: VsqxVersion) -> Vec<VsqxNote> {
    let pos_name = bracket_names(version, "posTick", "t");
    let dur_name = bracket_names(version, "durTick", "dur");
    let num_name = bracket_names(version, "noteNum", "n");
    let lyric_name = bracket_names(version, "lyric", "y");
    let phnms_name = bracket_names(version, "phnms", "p");
    children(part, "note")
        .map(|note| {
            let phnms = child(note, phnms_name).map(|node| {
                (
                    node_text(node),
                    node.attribute("lock")
                        .and_then(|value| value.trim().parse::<i64>().ok()),
                )
            });
            VsqxNote {
                pos_tick: first_int(note, &[pos_name]).unwrap_or(0),
                dur_tick: first_int(note, &[dur_name]).unwrap_or(0),
                note_num: first_int(note, &[num_name]).unwrap_or(0),
                lyric: child_text(note, lyric_name).unwrap_or_default(),
                phnms,
                seq_attrs: read_seq_attrs(note, version),
            }
        })
        .collect()
}

fn read_m_ctrls(part: roxmltree::Node<'_, '_>, version: VsqxVersion) -> Vec<MCtrl> {
    let (container, version_note) = match version {
        VsqxVersion::Vsq4 => ("cc", true),
        VsqxVersion::Vsq3 => ("mCtrl", false),
    };
    children(part, container)
        .map(|m_ctrl| {
            if version_note {
                let pos_tick = first_int(m_ctrl, &["t"]).unwrap_or(0);
                let attr = child(m_ctrl, "v");
                let attr_id = attr.and_then(|node| node.attribute("id").map(str::to_owned));
                let value = attr.and_then(|node| parse_int(Some(node_text(node))));
                MCtrl {
                    pos_tick,
                    attr_id,
                    value,
                }
            } else {
                let pos_tick = first_int(m_ctrl, &["posTick"]).unwrap_or(0);
                let attr = child(m_ctrl, "attr");
                let attr_id = attr.and_then(|node| node.attribute("id").map(str::to_owned));
                let value = attr.and_then(|node| parse_int(Some(node_text(node))));
                MCtrl {
                    pos_tick,
                    attr_id,
                    value,
                }
            }
        })
        .collect()
}

struct ParsedFile {
    version: VsqxVersion,
    time_signatures: Vec<TimeSignature>,
    pre_measure: i64,
    tempos: Vec<SongTempo>,
    voices: Vec<(Option<i64>, Option<String>)>,
    vs_tracks: Vec<VsTrack>,
    vs_units: Vec<WavUnit>,
    wav_parts: Vec<WavPart>,
    wav_units: Vec<WavUnit>,
}

fn read_file(root: roxmltree::Node<'_, '_>) -> Result<ParsedFile> {
    let root_name = root.tag_name().name();
    let version = VsqxVersion::from_root_name(root_name).ok_or_else(|| {
        LibreSvipError::UnsupportedProjectVersion(format!("VSQX root element {root_name}"))
    })?;
    let master_track = child(root, "masterTrack").ok_or_else(|| {
        LibreSvipError::Parse("missing masterTrack element".to_owned())
    })?;
    let pre_measure = first_int(master_track, &["preMeasure"]).unwrap_or(1);
    let time_signatures = read_time_signatures(master_track, version);
    let tempos = read_tempos(master_track, version);

    let voices = child(root, "vVoiceTable")
        .map(|table| {
            children(table, "vVoice")
                .map(|voice| {
                    let pc = first_int(voice, &[bracket_names(version, "vPC", "pc")]);
                    let name = first_text(voice, &[bracket_names(version, "vVoiceName", "name")]);
                    (pc, name)
                })
                .collect()
        })
        .unwrap_or_default();

    let mixer = child(root, "mixer");
    let vs_units: Vec<WavUnit> = mixer
        .map(|mixer| {
            children(mixer, "vsUnit")
                .map(|unit| WavUnit {
                    mute: first_int(unit, &[bracket_names(version, "mute", "m")]).unwrap_or(0) != 0,
                    solo: first_int(unit, &[bracket_names(version, "solo", "s")]).unwrap_or(0) != 0,
                })
                .collect()
        })
        .unwrap_or_default();

    let vs_tracks: Vec<VsTrack> = children(root, "vsTrack")
        .map(|track| {
            let track_name =
                first_text(track, &[bracket_names(version, "trackName", "name")])
                    .unwrap_or_default();
            let part_name = bracket_names(version, "musicalPart", "vsPart");
            let parts = children(track, part_name)
                .map(|part| MusicalPart {
                    pos_tick: first_int(part, &[bracket_names(version, "posTick", "t")])
                        .unwrap_or(0),
                    play_time: first_int(part, &["playTime"]).unwrap_or(0),
                    notes: read_notes(part, version),
                    m_ctrls: read_m_ctrls(part, version),
                    v_pc: child(part, "singer")
                        .and_then(|singer| first_int(singer, &[bracket_names(version, "vPC", "pc")])),
                })
                .collect();
            VsTrack { track_name, parts }
        })
        .collect();

    let mono_unit = mixer
        .and_then(|mixer| {
            child(
                mixer,
                bracket_names(version, "seUnit", "monoUnit"),
            )
        })
        .map(|unit| WavUnit {
            mute: first_int(unit, &[bracket_names(version, "mute", "m")]).unwrap_or(0) != 0,
            solo: first_int(unit, &[bracket_names(version, "solo", "s")]).unwrap_or(0) != 0,
        })
        .unwrap_or(WavUnit {
            mute: false,
            solo: false,
        });
    let stereo_unit = mixer
        .and_then(|mixer| {
            child(
                mixer,
                bracket_names(version, "karaokeUnit", "stUnit"),
            )
        })
        .map(|unit| WavUnit {
            mute: first_int(unit, &[bracket_names(version, "mute", "m")]).unwrap_or(0) != 0,
            solo: first_int(unit, &[bracket_names(version, "solo", "s")]).unwrap_or(0) != 0,
        })
        .unwrap_or(WavUnit {
            mute: false,
            solo: false,
        });

    let mut wav_parts: Vec<WavPart> = Vec::new();
    let mut wav_units: Vec<WavUnit> = Vec::new();
    if let Some(mono_track) = child(
        root,
        bracket_names(version, "seTrack", "monoTrack"),
    ) {
        let parts: Vec<WavPart> = children(mono_track, "wavPart")
            .map(|part| WavPart {
                pos_tick: first_int(part, &[bracket_names(version, "posTick", "t")]).unwrap_or(0),
                part_name: first_text(part, &[bracket_names(version, "partName", "name")])
                    .unwrap_or_default(),
                file_path: child_text(part, "filePath").unwrap_or_default(),
            })
            .collect();
        let count = parts.len();
        wav_parts.extend(parts);
        wav_units.extend(std::iter::repeat_n(mono_unit.clone(), count));
    }
    if let Some(stereo_track) = child(
        root,
        bracket_names(version, "karaokeTrack", "stTrack"),
    ) {
        let parts: Vec<WavPart> = children(stereo_track, "wavPart")
            .map(|part| WavPart {
                pos_tick: first_int(part, &[bracket_names(version, "posTick", "t")]).unwrap_or(0),
                part_name: first_text(part, &[bracket_names(version, "partName", "name")])
                    .unwrap_or_default(),
                file_path: child_text(part, "filePath").unwrap_or_default(),
            })
            .collect();
        let count = parts.len();
        wav_parts.extend(parts);
        wav_units.extend(std::iter::repeat_n(stereo_unit.clone(), count));
    }

    Ok(ParsedFile {
        version,
        time_signatures,
        pre_measure,
        tempos,
        voices,
        vs_tracks,
        vs_units,
        wav_parts,
        wav_units,
    })
}

struct Parser<'a> {
    options: &'a InputOptions,
    version: VsqxVersion,
    time_signatures: Vec<TimeSignature>,
    first_bar_length: i64,
    synchronizer: TimeSynchronizer,
    pc2voice: Vec<(i64, Option<String>)>,
}

/// Parse a VSQX document into the internal project model.
pub fn parse_project(
    bytes: &[u8],
    options: &InputOptions,
    src_path: &Path,
) -> Result<Project> {
    let document = parse_xml(bytes)?;
    let root = document.root_element();
    let parsed = read_file(root)?;

    let (tick_prefix, time_signatures) =
        parse_time_signatures(&parsed.time_signatures, parsed.pre_measure)?;
    let first_bar_length = parsed
        .time_signatures
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
    let mut tempo_list = Vec::with_capacity(parsed.tempos.len());
    for tempo in &parsed.tempos {
        tempo_list.push(SongTempo::new(tempo.position, tempo.bpm / BPM_RATE));
    }
    let tempos = skip_tempo_list(&tempo_list, tick_prefix);
    let synchronizer = TimeSynchronizer::from_project(&tempos);

    let mut pc2voice: Vec<(i64, Option<String>)> = Vec::new();
    for (pc, name) in &parsed.voices {
        if let Some(pc) = pc {
            if let Some(entry) = pc2voice.iter_mut().find(|entry| entry.0 == *pc) {
                entry.1 = name.clone();
            } else {
                pc2voice.push((*pc, name.clone()));
            }
        }
    }

    let parser = Parser {
        options,
        version: parsed.version,
        time_signatures,
        first_bar_length,
        synchronizer,
        pc2voice,
    };

    let mut tracks: Vec<Track> = Vec::new();
    for (vs_track, vs_unit) in parsed.vs_tracks.iter().zip(parsed.vs_units.iter()) {
        let mut singing_track = SingingTrack::new(vs_track.track_name.clone());
        singing_track.mute = vs_unit.mute;
        singing_track.solo = vs_unit.solo;
        for part in &vs_track.parts {
            let (note_list, vibrato_rate_dict, vibrato_depth_dict) =
                parser.parse_notes(&part.notes, part.pos_tick - tick_prefix)?;
            singing_track.note_list.extend(note_list.iter().cloned());
            if singing_track.ai_singer_name.is_empty() {
                if let Some(v_pc) = part.v_pc {
                    if let Some((_, Some(name))) =
                        parser.pc2voice.iter().find(|entry| entry.0 == v_pc)
                    {
                        singing_track.ai_singer_name = name.clone();
                    }
                }
            }
            if parser.options.import_pitch {
                if let Some(pitch) = parser.parse_pitch(
                    part,
                    &note_list,
                    vibrato_rate_dict,
                    vibrato_depth_dict,
                    tick_prefix,
                )? {
                    singing_track
                        .edited_params
                        .pitch
                        .points
                        .extend(pitch.points);
                }
            }
            parser.parse_params(part, &mut singing_track.edited_params, tick_prefix);
        }
        if !singing_track.edited_params.pitch.points.is_empty() {
            singing_track
                .edited_params
                .pitch
                .points
                .insert(0, Point::start_point());
            singing_track
                .edited_params
                .pitch
                .points
                .push(Point::end_point());
        }
        tracks.push(Track::Singing(singing_track));
    }
    tracks.extend(parser.parse_instrumental_tracks(
        &parsed.wav_parts,
        &parsed.wav_units,
        tick_prefix,
        src_path,
    ));

    Ok(Project {
        warnings: Vec::new(),
        version: String::new(),
        song_tempo_list: tempos,
        time_signature_list: parser.time_signatures,
        track_list: tracks,
    })
}

fn parse_time_signatures(
    time_signatures: &[TimeSignature],
    measure_prefix: i64,
) -> Result<(i64, Vec<TimeSignature>)> {
    if time_signatures.is_empty() {
        return Err(LibreSvipError::msg("time_signature_list must not be empty"));
    }
    let time_signature_list: Vec<TimeSignature> = time_signatures.to_vec();
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
    let measure_diff = measure_prefix - measure;
    tick_prefix += measure_diff * py_round(last.bar_length());
    Ok((
        tick_prefix,
        skip_beat_list(&time_signature_list, measure_prefix),
    ))
}

impl Parser<'_> {
    fn parse_notes(
        &self,
        notes: &[VsqxNote],
        tick_offset: i64,
    ) -> Result<(
        Vec<Note>,
        super::vocaloid::PiecewiseIntervalDict,
        super::vocaloid::PiecewiseIntervalDict,
    )> {
        let mut prev_vsqx_note: Option<&VsqxNote> = None;
        let mut note_list: Vec<Note> = Vec::new();
        let mut vibrato_depth_interval_dict = super::vocaloid::PiecewiseIntervalDict::default();
        let mut vibrato_rate_interval_dict = super::vocaloid::PiecewiseIntervalDict::default();
        for vsqx_note in notes {
            if prev_vsqx_note.is_some() && vsqx_note.lyric.starts_with("EVEC(") {
                if let Some(last) = note_list.last_mut() {
                    last.length += vsqx_note.dur_tick;
                }
                continue;
            } else {
                let phnms_value = vsqx_note.phnms.as_ref().map(|(value, _)| value.clone());
                // `elif vsqx_note.phnms is None or phnms.value not in [...]`
                let skip = match &phnms_value {
                    None => false,
                    Some(value) => matches!(value.as_str(), "Asp" | "Sil" | "?"),
                };
                if !skip {
                    let lyric = if vsqx_note.lyric.is_empty() {
                        DEFAULT_ENGLISH_LYRIC.to_owned()
                    } else {
                        vsqx_note.lyric.clone()
                    }
                    .to_lowercase();
                    let pronunciation = vsqx_note
                        .phnms
                        .as_ref()
                        .filter(|(_, lock)| *lock == Some(1))
                        .map(|(value, _)| value.clone());
                    let mut note = Note {
                        start_pos: vsqx_note.pos_tick + tick_offset,
                        length: vsqx_note.dur_tick,
                        lyric,
                        pronunciation,
                        key_number: vsqx_note.note_num,
                        ..Default::default()
                    };
                    if !vsqx_note.seq_attrs.is_empty() {
                        let start_secs = self
                            .synchronizer
                            .get_actual_secs_from_ticks(note.start_pos);
                        let duration_secs = self.synchronizer.get_duration_secs_from_ticks(
                            note.start_pos,
                            note.end_pos(),
                        );
                        for seq_attr in &vsqx_note.seq_attrs {
                            if seq_attr.seq_id == "vibRate" {
                                let mut phase = 0.0f64;
                                for (index, prev_elem) in seq_attr.elems.iter().enumerate() {
                                    let prev_start = start_secs
                                        + duration_secs * prev_elem.0 as f64 / 65536.0;
                                    let omega = prev_elem.1 as f64 / 2.0;
                                    let next = seq_attr.elems.get(index + 1);
                                    match next {
                                        None => {
                                            let prev_end = start_secs + duration_secs;
                                            vibrato_rate_interval_dict.set_vibrato_rate(
                                                prev_start, prev_end, true, prev_start, omega, phase,
                                            );
                                            phase += (prev_end - prev_start) * omega;
                                        }
                                        Some(next) => {
                                            let prev_end = start_secs
                                                + duration_secs * next.0 as f64 / 65536.0;
                                            vibrato_rate_interval_dict.set_vibrato_rate(
                                                prev_start, prev_end, false, prev_start, omega, phase,
                                            );
                                            phase += (prev_end - prev_start) * omega;
                                        }
                                    }
                                }
                            } else if seq_attr.seq_id == "vibDep" {
                                for (index, prev_elem) in seq_attr.elems.iter().enumerate() {
                                    let prev_start = start_secs
                                        + duration_secs * prev_elem.0 as f64 / 65536.0;
                                    let next = seq_attr.elems.get(index + 1);
                                    match next {
                                        None => {
                                            let prev_end = start_secs + duration_secs;
                                            vibrato_depth_interval_dict.set_closed(
                                                prev_start,
                                                prev_end,
                                                prev_elem.1 as f64,
                                            );
                                        }
                                        Some(next) => {
                                            let prev_end = start_secs
                                                + duration_secs * next.0 as f64 / 65536.0;
                                            vibrato_depth_interval_dict.set_closedopen(
                                                prev_start,
                                                prev_end,
                                                prev_elem.1 as f64,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(prev) = prev_vsqx_note {
                        if prev.pos_tick + prev.dur_tick == vsqx_note.pos_tick
                            && prev
                                .phnms
                                .as_ref()
                                .map(|(value, _)| value == "Sil")
                                .unwrap_or(false)
                        {
                            note.head_tag = Some("0".to_owned());
                        }
                    }
                    note_list.push(note);
                }
            }
            prev_vsqx_note = Some(vsqx_note);
        }
        if self.options.combine_syllables {
            // `more_itertools.split_when`: start a new group when the previous
            // lyric does not end with "-".
            let mut groups: Vec<Vec<usize>> = Vec::new();
            for index in 0..note_list.len() {
                if index > 0 && !note_list[index - 1].lyric.ends_with('-') {
                    groups.push(vec![index]);
                } else if let Some(last) = groups.last_mut() {
                    last.push(index);
                } else {
                    groups.push(vec![index]);
                }
            }
            for group in groups {
                if group.len() > 1 {
                    let merged: String = group
                        .iter()
                        .map(|index| note_list[*index].lyric.trim_end_matches('-'))
                        .collect();
                    note_list[group[0]].lyric = merged;
                    for index in &group[1..] {
                        if note_list[*index].lyric != "-" {
                            note_list[*index].lyric = "+".to_owned();
                        }
                    }
                }
            }
        }
        Ok((
            note_list,
            vibrato_rate_interval_dict,
            vibrato_depth_interval_dict,
        ))
    }

    fn extract_curve(&self, part: &MusicalPart, param_name: &str) -> Option<ControllerCurve> {
        let param_def = super::vocaloid::get_param_def(param_name)?;
        let vsqx_param_id = self.version.param_id(param_def.vsq_name?);
        let events: Vec<ControllerEvent> = part
            .m_ctrls
            .iter()
            .filter(|m_ctrl| {
                m_ctrl.attr_id.as_deref() == Some(vsqx_param_id) && m_ctrl.value.is_some()
            })
            .map(|m_ctrl| ControllerEvent {
                pos: m_ctrl.pos_tick,
                value: m_ctrl.value.unwrap_or(0),
            })
            .collect();
        if events.is_empty() {
            return None;
        }
        Some(ControllerCurve::with_events(
            param_name,
            events,
            param_def.default_value,
            param_def.min_value,
            param_def.max_value,
        ))
    }

    fn parse_pitch(
        &self,
        part: &MusicalPart,
        note_list: &[Note],
        vibrato_rate_interval_dict: super::vocaloid::PiecewiseIntervalDict,
        vibrato_depth_interval_dict: super::vocaloid::PiecewiseIntervalDict,
        tick_offset: i64,
    ) -> Result<Option<libresvip_core::model::base::ParamCurve>> {
        let pit_curve = self.extract_curve(part, "pitch_bend");
        let pbs_curve = self.extract_curve(part, "pitch_bend_sens");
        let Some(pit_curve) = pit_curve else {
            return Ok(None);
        };
        let pbs_curve = pbs_curve.unwrap_or_else(|| {
            ControllerCurve::with_events(
                "pitch_bend_sens",
                Vec::new(),
                DEFAULT_PITCH_BEND_SENSITIVITY,
                1,
                MAX_PITCH_BEND_SENSITIVITY,
            )
        });
        let pitch_data = PitchBendData {
            pit: pit_curve,
            pbs: pbs_curve,
        };
        let handler = VocaloidPitchHandler::new(
            self.synchronizer.clone(),
            note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        let offset = part.pos_tick - tick_offset;
        let mut vibrato_rate_interval_dict = vibrato_rate_interval_dict;
        let mut vibrato_depth_interval_dict = vibrato_depth_interval_dict;
        handler.to_absolute_pitch(
            &[pitch_data],
            Some(&[offset]),
            Some(&mut vibrato_rate_interval_dict),
            Some(&mut vibrato_depth_interval_dict),
            Some(&[offset]),
            Some(&[part.pos_tick + part.play_time - tick_offset]),
        )
    }

    fn parse_params(
        &self,
        part: &MusicalPart,
        params: &mut libresvip_core::model::base::Params,
        tick_prefix: i64,
    ) {
        let offset = part.pos_tick - tick_prefix;
        if self.options.import_volume {
            if let Some(curve) = self.extract_curve(part, "dynamics") {
                params.volume.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    offset + self.first_bar_length,
                    false,
                ));
            }
        }
        if self.options.import_breath {
            if let Some(curve) = self.extract_curve(part, "breathiness") {
                params.breath.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    offset + self.first_bar_length,
                    false,
                ));
            }
        }
        if self.options.import_gender {
            if let Some(curve) = self.extract_curve(part, "gender") {
                params.gender.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    offset + self.first_bar_length,
                    true,
                ));
            }
        }
        if self.options.import_strength {
            if let Some(curve) = self.extract_curve(part, "brightness") {
                params.strength.points.extend(convert_vocaloid_curve_to_param_points(
                    &curve,
                    offset + self.first_bar_length,
                    false,
                ));
            }
        }
    }

    fn parse_instrumental_tracks(
        &self,
        wav_parts: &[WavPart],
        wav_units: &[WavUnit],
        tick_prefix: i64,
        src_path: &Path,
    ) -> Vec<Track> {
        let mut tracks = Vec::new();
        if !self.options.import_instrumental_track {
            return tracks;
        }
        for (wav_part, wav_unit) in wav_parts.iter().zip(wav_units.iter()) {
            let path = Path::new(&wav_part.file_path);
            let wav_path_str = if path.is_absolute() {
                wav_part.file_path.clone()
            } else {
                let mut base = src_path.to_path_buf();
                base.set_extension("wavparts");
                let candidate = base.join(&wav_part.file_path);
                if candidate.exists() {
                    candidate.to_string_lossy().into_owned()
                } else {
                    continue;
                }
            };
            let mut track = InstrumentalTrack::new(wav_part.part_name.clone(), wav_path_str);
            track.offset = wav_part.pos_tick - tick_prefix;
            track.mute = wav_unit.mute;
            track.solo = wav_unit.solo;
            tracks.push(Track::Instrumental(track));
        }
        tracks
    }
}
