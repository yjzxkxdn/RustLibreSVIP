//! Port of `libresvip.plugins.ccs.cevio_generator`, including the xsdata
//! `pretty_print=True` XML layout.

use libresvip_core::core::lyric_phoneme::japanese::{is_kana, is_romaji};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SongTempo, TimeSignature, Track};
use libresvip_core::options::Options;

use super::interval::float_repr;
use super::pyutil::{escape_attribute, escape_text, uuid_str};
use libresvip_core::utils::audio::audio_track_info;
use super::techno_speech::{generate_for_techno_speech, ParamEvent};

pub const DEFAULT_PHONEME: &str = "r,a";
const DEFAULT_CODE: &str = "7251BC4B6168E7B2992FA620BD3E1E77";

struct SoundOut {
    clock: i64,
    tempo: f64,
}

struct TimeOut {
    clock: i64,
    beats: i64,
    beat_type: i64,
}

struct NoteOut {
    clock: i64,
    pitch_step: i64,
    pitch_octave: i64,
    duration: i64,
    lyric: String,
    phonetic: Option<String>,
}

struct LogF0Out {
    length: i64,
    data: Vec<ParamEvent>,
}

struct SongOut {
    tempo: Option<Vec<SoundOut>>,
    beat: Option<Vec<TimeOut>>,
    notes: Vec<NoteOut>,
    log_f0: Option<LogF0Out>,
}

struct UnitOut {
    group_id: String,
    start_time: f64,
    duration: Option<f64>,
    category: &'static str,
    cast_id: Option<String>,
    file_path: Option<String>,
    language: &'static str,
    song: SongOut,
}

struct GroupOut {
    group_id: String,
    category: &'static str,
    name: String,
    is_solo: bool,
    is_muted: bool,
    cast_id: Option<String>,
    language: &'static str,
}

struct SoundSource {
    id: String,
    name: String,
    version: String,
}

pub fn generate_project(project: &Project, options: &Options) -> Result<String> {
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let first_signature = project
        .time_signature_list
        .first()
        .ok_or_else(|| LibreSvipError::Validation("project has no time signature".to_owned()))?;
    let first_bar_length = first_signature.bar_length() as i64;
    let sounds = generate_tempos(&project.song_tempo_list)?;
    let times = generate_time_signatures(&project.time_signature_list);
    let default_singer_id = options.str("default_singer_id", "A");
    let sound_source = SoundSource {
        id: default_singer_id.clone(),
        name: options.str("default_singer_name", "さとうささら"),
        version: options.str("default_singer_version", "1.0.0"),
    };
    let mut units = Vec::new();
    let mut groups = Vec::new();
    for track in &project.track_list {
        match track {
            Track::Singing(singing) => {
                let group_id = uuid_str();
                let notes = generate_notes(&singing.note_list);
                let mut duration = None;
                if let Some(last_note) = singing.note_list.last() {
                    let max_tick = last_note.end_pos();
                    duration = Some(synchronizer.get_actual_secs_from_ticks(max_tick));
                }
                let log_f0 = generate_pitch(
                    &singing.edited_params.pitch,
                    &project.song_tempo_list,
                    first_bar_length,
                );
                units.push(UnitOut {
                    group_id: group_id.clone(),
                    start_time: 0.0,
                    duration,
                    category: "SingerSong",
                    cast_id: Some(default_singer_id.clone()),
                    file_path: None,
                    language: "Japanese",
                    song: SongOut {
                        tempo: Some(
                            sounds
                                .iter()
                                .map(|sound| SoundOut {
                                    clock: sound.clock,
                                    tempo: sound.tempo,
                                })
                                .collect(),
                        ),
                        beat: Some(
                            times
                                .iter()
                                .map(|time| TimeOut {
                                    clock: time.clock,
                                    beats: time.beats,
                                    beat_type: time.beat_type,
                                })
                                .collect(),
                        ),
                        notes,
                        log_f0,
                    },
                });
                groups.push(GroupOut {
                    group_id,
                    category: "SingerSong",
                    name: singing.title.clone(),
                    is_solo: singing.solo,
                    is_muted: singing.mute,
                    cast_id: Some(default_singer_id.clone()),
                    language: "Japanese",
                });
            }
            Track::Instrumental(instrumental) => {
                let Some(info) = audio_track_info(&instrumental.audio_file_path, true) else {
                    continue;
                };
                let group_id = uuid_str();
                let start_secs = synchronizer.get_actual_secs_from_ticks(instrumental.offset);
                units.push(UnitOut {
                    group_id: group_id.clone(),
                    start_time: start_secs,
                    duration: Some(info.duration),
                    category: "OuterAudio",
                    cast_id: None,
                    file_path: Some(instrumental.audio_file_path.clone()),
                    language: "Japanese",
                    song: SongOut {
                        tempo: None,
                        beat: None,
                        notes: Vec::new(),
                        log_f0: None,
                    },
                });
                groups.push(GroupOut {
                    group_id,
                    category: "OuterAudio",
                    name: instrumental.title.clone(),
                    is_solo: instrumental.solo,
                    is_muted: instrumental.mute,
                    cast_id: None,
                    language: "Japanese",
                });
            }
        }
    }
    Ok(render(
        &sound_source,
        &units,
        &groups,
        &sounds,
    ))
}

fn generate_tempos(tempos: &[SongTempo]) -> Result<Vec<SoundOut>> {
    let first = tempos
        .first()
        .ok_or_else(|| LibreSvipError::Validation("project has no tempo".to_owned()))?;
    let mut result = vec![SoundOut {
        clock: 0,
        tempo: first.bpm,
    }];
    for tempo in &tempos[1..] {
        result.push(SoundOut {
            clock: (tempo.position as f64 * 2.0) as i64,
            tempo: tempo.bpm,
        });
    }
    Ok(result)
}

fn generate_time_signatures(time_signatures: &[TimeSignature]) -> Vec<TimeOut> {
    let Some(first) = time_signatures.first() else {
        return Vec::new();
    };
    let mut result = vec![TimeOut {
        clock: 0,
        beats: first.numerator,
        beat_type: first.denominator,
    }];
    let mut tick = 0.0f64;
    let mut previous = first;
    for time_signature in &time_signatures[1..] {
        if time_signature.bar_index > previous.bar_index {
            tick += (time_signature.bar_index - previous.bar_index) as f64 * previous.bar_length();
        }
        result.push(TimeOut {
            clock: (tick * 2.0) as i64,
            beats: time_signature.numerator,
            beat_type: time_signature.denominator,
        });
        previous = time_signature;
    }
    result
}

fn generate_notes(notes: &[Note]) -> Vec<NoteOut> {
    let mut result = Vec::new();
    for note in notes {
        let lyric = if note.lyric == "-" {
            "\u{30FC}".to_owned()
        } else {
            note.lyric.clone()
        };
        let phonetic = match &note.pronunciation {
            Some(pronunciation) if !pronunciation.is_empty() => Some(pronunciation.clone()),
            _ => {
                if !is_kana(&lyric) && !is_romaji(&lyric) {
                    show_warning(format!("Unsupported lyric:  {lyric}"));
                    Some(DEFAULT_PHONEME.to_owned())
                } else {
                    None
                }
            }
        };
        result.push(NoteOut {
            clock: (note.start_pos as f64 * 2.0) as i64,
            duration: (note.length as f64 * 2.0) as i64,
            lyric,
            phonetic,
            pitch_octave: note.key_number.div_euclid(12) - 1,
            pitch_step: note.key_number.rem_euclid(12),
        });
    }
    result
}

fn generate_pitch(
    pitch: &libresvip_core::model::base::ParamCurve,
    tempo_list: &[SongTempo],
    first_bar_length: i64,
) -> Option<LogF0Out> {
    let data = generate_for_techno_speech(pitch, tempo_list, first_bar_length)?;
    Some(LogF0Out {
        length: data.length,
        data: data.events,
    })
}

// ---------------------------------------------------------------------------
// XML rendering (xsdata `pretty_print=True`)
// ---------------------------------------------------------------------------

struct Xml {
    out: String,
}

impl Xml {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }
}

fn attribute(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape_attribute(value));
    out.push('"');
}

fn xml_time(secs: f64) -> String {
    let total = if secs.is_finite() {
        secs.rem_euclid(86_400.0)
    } else {
        0.0
    };
    let hour = (total / 3600.0).floor() as i64;
    let remaining = total - hour as f64 * 3600.0;
    let minute = (remaining / 60.0).floor() as i64;
    let seconds = remaining - minute as f64 * 60.0;
    let whole = seconds.floor();
    let mut microseconds = ((seconds - whole) * 1_000_000.0).round_ties_even() as i64;
    let mut second = whole as i64;
    if microseconds >= 1_000_000 {
        microseconds = 0;
        second += 1;
    }
    let mut rendered = format!("{hour:02}:{minute:02}:{second:02}");
    if microseconds > 0 {
        if microseconds % 1000 == 0 {
            rendered.push_str(&format!(".{:03}", microseconds / 1000));
        } else {
            rendered.push_str(&format!(".{microseconds:06}"));
        }
    }
    rendered
}

fn render(
    sound_source: &SoundSource,
    units: &[UnitOut],
    groups: &[GroupOut],
    sounds: &[SoundOut],
) -> String {
    let mut xml = Xml {
        out: String::with_capacity(4096),
    };
    xml.line(0, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let mut scenario = String::from("<Scenario");
    attribute(&mut scenario, "Code", DEFAULT_CODE);
    scenario.push('>');
    xml.line(0, &scenario);
    xml.line(1, "<Generation>");
    xml.line(2, "<Author Version=\"3.2.21.2\"/>");
    xml.line(2, "<TTS Version=\"3.1.0\">");
    xml.line(3, "<Dictionary Version=\"1.0.0\"/>");
    xml.line(3, "<SoundSources/>");
    xml.line(2, "</TTS>");
    xml.line(2, "<SVSS Version=\"3.0.5\">");
    xml.line(3, "<Dictionary Version=\"1.0.0\"/>");
    xml.line(3, "<SoundSources>");
    let mut source = String::from("<SoundSource");
    attribute(&mut source, "Version", &sound_source.version);
    attribute(&mut source, "Id", &sound_source.id);
    attribute(&mut source, "Name", &sound_source.name);
    source.push_str("/>");
    xml.line(4, &source);
    xml.line(3, "</SoundSources>");
    xml.line(2, "</SVSS>");
    xml.line(1, "</Generation>");
    xml.line(1, "<Sequence Id=\"\">");
    xml.line(2, "<Scene Id=\"\">");
    if units.is_empty() {
        xml.line(3, "<Units/>");
    } else {
        xml.line(3, "<Units>");
        for unit in units {
            write_unit(&mut xml, unit);
        }
        xml.line(3, "</Units>");
    }
    if groups.is_empty() {
        xml.line(3, "<Groups/>");
    } else {
        xml.line(3, "<Groups>");
        for group in groups {
            let mut line = String::from("<Group");
            attribute(&mut line, "Version", "1.0");
            attribute(&mut line, "Id", &group.group_id);
            attribute(&mut line, "Category", group.category);
            attribute(&mut line, "Name", &group.name);
            attribute(&mut line, "IsSolo", if group.is_solo { "true" } else { "false" });
            attribute(&mut line, "IsMuted", if group.is_muted { "true" } else { "false" });
            if let Some(cast_id) = &group.cast_id {
                attribute(&mut line, "CastId", cast_id);
            }
            attribute(&mut line, "Language", group.language);
            line.push_str("/>");
            xml.line(4, &line);
        }
        xml.line(3, "</Groups>");
    }
    let _ = sounds;
    xml.line(3, "<SoundSetting Rhythm=\"4/4\" Tempo=\"120.0\"/>");
    xml.line(2, "</Scene>");
    xml.line(1, "</Sequence>");
    xml.line(0, "</Scenario>");
    xml.out
}

fn write_unit(xml: &mut Xml, unit: &UnitOut) {
    let mut line = String::from("<Unit");
    attribute(&mut line, "Version", "1.0");
    attribute(&mut line, "Id", "");
    attribute(&mut line, "Group", &unit.group_id);
    attribute(&mut line, "StartTime", &xml_time(unit.start_time));
    if let Some(duration) = unit.duration {
        attribute(&mut line, "Duration", &xml_time(duration));
    }
    attribute(&mut line, "Category", unit.category);
    if let Some(cast_id) = &unit.cast_id {
        attribute(&mut line, "CastId", cast_id);
    }
    if let Some(file_path) = &unit.file_path {
        attribute(&mut line, "FilePath", file_path);
    }
    attribute(&mut line, "Language", unit.language);
    line.push('>');
    xml.line(4, &line);
    let mut song = String::from("<Song");
    attribute(&mut song, "Version", "1.02");
    song.push('>');
    xml.line(5, &song);
    if let Some(tempos) = &unit.song.tempo {
        xml.line(6, "<Tempo>");
        for tempo in tempos {
            let mut sound = String::from("<Sound");
            attribute(&mut sound, "Clock", &tempo.clock.to_string());
            attribute(&mut sound, "Tempo", &float_repr(tempo.tempo));
            sound.push_str("/>");
            xml.line(7, &sound);
        }
        xml.line(6, "</Tempo>");
    }
    if let Some(times) = &unit.song.beat {
        xml.line(6, "<Beat>");
        for time in times {
            let mut line = String::from("<Time");
            attribute(&mut line, "Clock", &time.clock.to_string());
            attribute(&mut line, "Beats", &time.beats.to_string());
            attribute(&mut line, "BeatType", &time.beat_type.to_string());
            line.push_str("/>");
            xml.line(7, &line);
        }
        xml.line(6, "</Beat>");
    }
    if unit.song.notes.is_empty() {
        xml.line(6, "<Score/>");
    } else {
        xml.line(6, "<Score>");
        for note in &unit.song.notes {
            let mut line = String::from("<Note");
            attribute(&mut line, "Clock", &note.clock.to_string());
            attribute(&mut line, "PitchStep", &note.pitch_step.to_string());
            attribute(&mut line, "PitchOctave", &note.pitch_octave.to_string());
            attribute(&mut line, "Duration", &note.duration.to_string());
            attribute(&mut line, "Lyric", &note.lyric);
            if let Some(phonetic) = &note.phonetic {
                attribute(&mut line, "Phonetic", phonetic);
            }
            line.push_str("/>");
            xml.line(7, &line);
        }
        xml.line(6, "</Score>");
    }
    match &unit.song.log_f0 {
        None => xml.line(6, "<Parameter/>"),
        Some(log_f0) => {
            xml.line(6, "<Parameter>");
            let mut line = String::from("<LogF0");
            attribute(&mut line, "Length", &log_f0.length.to_string());
            line.push('>');
            xml.line(7, &line);
            for data in &log_f0.data {
                let mut line = String::from("<Data");
                if let Some(index) = data.idx {
                    attribute(&mut line, "Index", &index.to_string());
                }
                if let Some(repeat) = data.repeat {
                    attribute(&mut line, "Repeat", &repeat.to_string());
                }
                line.push('>');
                line.push_str(&escape_text(&float_repr(data.value)));
                line.push_str("</Data>");
                xml.line(8, &line);
            }
            xml.line(7, "</LogF0>");
            xml.line(6, "</Parameter>");
        }
    }
    xml.line(5, "</Song>");
    xml.line(4, "</Unit>");
}
