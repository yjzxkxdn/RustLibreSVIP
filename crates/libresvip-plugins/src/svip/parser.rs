//! Port of `libresvip.plugins.svip.binsvip_parser`.

use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, SingingTrack, SongTempo,
    TimeSignature, Track, VibratoParam,
};
use libresvip_core::model::point::Point;

use super::models::{note_head_tag_name, reverb_name, singer_name};
use super::xstudio::{class, xs_error, XsClass};

/// `InputOptions` of the plugin (`option_mixins` order does not matter here).
#[derive(Clone, Debug)]
pub struct InputOptions {
    pub import_volume: bool,
    pub import_strength: bool,
    pub import_pitch: bool,
    pub import_instrumental_track: bool,
    pub import_gender: bool,
    pub import_breath: bool,
}

pub struct BinarySvipParser {
    pub options: InputOptions,
}

impl BinarySvipParser {
    pub fn new(options: InputOptions) -> Self {
        Self { options }
    }

    /// `BinarySvipParser.parse_project`.
    pub fn parse_project(&self, version: String, model: &XsClass) -> Result<Project> {
        let mut project = Project {
            version,
            ..Project::default()
        };
        for tempo in model.as_class("tempo_list")?.as_class("buf")?.class_list("items")? {
            project.song_tempo_list.push(parse_song_tempo(tempo)?);
        }
        for beat in model.as_class("beat_list")?.as_class("buf")?.class_list("items")? {
            project.time_signature_list.push(parse_time_signature(beat)?);
        }
        for track in model.as_class("track_list")?.class_list("items")? {
            if let Some(parsed) = self.parse_track(track)? {
                project.track_list.push(parsed);
            }
        }
        Ok(project)
    }

    fn parse_track(&self, track: &XsClass) -> Result<Option<Track>> {
        match track.name {
            class::SINGING_TRACK => {
                let mut result = SingingTrack {
                    ai_singer_name: singer_name(track.string("ai_singer_id")?),
                    reverb_preset: reverb_name(track.as_class("reverb_preset")?.i32("value")?)
                        .unwrap_or("")
                        .to_owned(),
                    ..SingingTrack::default()
                };
                for note in track
                    .as_class("note_list")?
                    .as_class("buf")?
                    .class_list("items")?
                {
                    result.note_list.push(self.parse_note(note)?);
                }
                result.edited_params = self.parse_params(track)?;
                result.title = track.string("name")?.to_owned();
                result.mute = track.bool("mute")?;
                result.solo = track.bool("solo")?;
                result.volume = track.f64("volume")?;
                result.pan = track.f64("pan")?;
                Ok(Some(Track::Singing(result)))
            }
            class::INSTRUMENT_TRACK if self.options.import_instrumental_track => {
                let mut result = InstrumentalTrack {
                    audio_file_path: track.string("instrument_file_path")?.to_owned(),
                    offset: track.i64("offset_in_pos")?,
                    ..InstrumentalTrack::default()
                };
                result.title = track.string("name")?.to_owned();
                result.mute = track.bool("mute")?;
                result.solo = track.bool("solo")?;
                result.volume = track.f64("volume")?;
                result.pan = track.f64("pan")?;
                Ok(Some(Track::Instrumental(result)))
            }
            _ => Ok(None),
        }
    }

    fn parse_params(&self, track: &XsClass) -> Result<Params> {
        let mut params = Params::default();
        if self.options.import_pitch {
            if let Some(line) = track.opt_class_or_missing("edited_pitch_line")? {
                params.pitch = parse_param_curve(line, pitch_from_svip)?;
            }
        }
        if self.options.import_volume {
            if let Some(line) = track.opt_class_or_missing("edited_volume_line")? {
                // Python assigns the volume line to `params.breath` (sic).
                params.breath = parse_param_curve(line, identity)?;
            }
        }
        if self.options.import_breath {
            if let Some(line) = track.opt_class_or_missing("edited_breath_line")? {
                params.breath = parse_param_curve(line, identity)?;
            }
        }
        if self.options.import_gender {
            if let Some(line) = track.opt_class_or_missing("edited_gender_line")? {
                params.gender = parse_param_curve(line, identity)?;
            }
        }
        if self.options.import_strength {
            if let Some(line) = track.opt_class_or_missing("edited_power_line")? {
                params.strength = parse_param_curve(line, identity)?;
            }
        }
        Ok(params)
    }

    fn parse_note(&self, note: &XsClass) -> Result<Note> {
        let mut result = Note {
            attrs: Default::default(),
            start_pos: note.i64("start_pos")?,
            length: note.i64("width_pos")?,
            key_number: note.i64("key_index")? - 12,
            head_tag: note_head_tag_name(note.as_class("head_tag")?.i32("value")?)
                .map(str::to_owned),
            lyric: cleanse_text(note.string("lyric")?),
            pronunciation: None,
            edited_phones: None,
            vibrato: None,
        };
        let pronouncing = note.string("pronouncing")?;
        if !pronouncing.is_empty() {
            result.pronunciation = Some(pronouncing.to_owned());
        }
        if let Some(phone) = note.opt_class_or_missing("note_phone_info")? {
            result.edited_phones = Some(Phones {
                head_length_in_secs: phone.f64("head_phone_time_in_sec")?,
                mid_ratio_over_tail: phone.f64("mid_part_over_tail_part_ratio")?,
            });
        }
        if let Some(style) = note.opt_class_or_missing("vibrato")? {
            result.vibrato = Some(parse_vibrato(note, style)?);
        }
        Ok(result)
    }
}

fn parse_song_tempo(tempo: &XsClass) -> Result<SongTempo> {
    Ok(SongTempo {
        position: tempo.i64("pos")?,
        bpm: tempo.i64("tempo")? as f64 / 100.0,
    })
}

fn parse_time_signature(beat: &XsClass) -> Result<TimeSignature> {
    let size = beat.as_class("beat_size")?;
    Ok(TimeSignature {
        bar_index: beat.i64("bar_index")?,
        numerator: size.i64("x")?,
        denominator: size.i64("y")?,
    })
}

fn parse_vibrato(note: &XsClass, style: &XsClass) -> Result<VibratoParam> {
    let mut result = VibratoParam::default();
    if let Some(percent) = note.opt_class_or_missing("vibrato_percent_info")? {
        result.start_percent = percent.f64("start_percent")?;
        result.end_percent = percent.f64("end_percent")?;
    } else {
        let percent = note.i64("vibrato_percent")?;
        if percent > 0 {
            result.start_percent = 1.0 - percent as f64 / 100.0;
            result.end_percent = 1.0;
        }
    }
    result.is_anti_phase = style.bool("is_anti_phase")?;
    result.amplitude = parse_param_curve(style.as_class("amp_line")?, identity)?;
    result.frequency = parse_param_curve(style.as_class("freq_line")?, identity)?;
    Ok(result)
}

fn identity(value: i64) -> i64 {
    value
}

/// `op=lambda x: x - 1150 if x > 1050 else -100`.
fn pitch_from_svip(value: i64) -> i64 {
    if value > 1050 {
        value - 1150
    } else {
        -100
    }
}

fn parse_param_curve(line: &XsClass, op: fn(i64) -> i64) -> Result<ParamCurve> {
    let nodes = line_param_nodes(line.bytes("line_param")?)?;
    Ok(ParamCurve::new(
        nodes
            .into_iter()
            .map(|(position, value)| Point::new(i64::from(position), op(i64::from(value))))
            .collect(),
    ))
}

/// `XSLineParam.__post_init__` — the `LineParam` byte blob is a count followed by
/// `count` `(pos, value)` `Int32` pairs.
pub fn line_param_nodes(line_param: &[u8]) -> Result<Vec<(i32, i32)>> {
    if line_param.len() < 4 {
        return Ok(Vec::new());
    }
    let node_count = i32::from_le_bytes(
        line_param[..4]
            .try_into()
            .map_err(|_| xs_error("LineParam header is truncated"))?,
    );
    if node_count < 0 {
        return Err(xs_error("LineParam has a negative node count"));
    }
    let needed = usize::try_from(node_count)
        .ok()
        .and_then(|count| count.checked_mul(8))
        .ok_or_else(|| xs_error("LineParam node count overflows"))?;
    let body = line_param
        .get(4..4 + needed)
        .ok_or_else(|| xs_error("LineParam node data is truncated"))?;
    let mut nodes = Vec::with_capacity(node_count as usize);
    for pair in body.chunks_exact(8) {
        let position = i32::from_le_bytes(
            pair[..4]
                .try_into()
                .map_err(|_| xs_error("LineParam node data is truncated"))?,
        );
        let value = i32::from_le_bytes(
            pair[4..]
                .try_into()
                .map_err(|_| xs_error("LineParam node data is truncated"))?,
        );
        nodes.push((position, value));
    }
    Ok(nodes)
}

/// `unsupported_symbols.cleanse_text`.
///
/// The Python regex is `[!,\\.\\?。！，？](?=$)` with `match_substrings=True`:
/// one of the listed symbols is removed only when it is the last character (or
/// right before a trailing newline — that is what Python's `$` means).
pub fn cleanse_text(text: &str) -> String {
    const UNSAFE: [char; 8] = [',', '，', '.', '。', '?', '？', '!', '！'];
    let characters: Vec<char> = text.chars().collect();
    let Some(&last) = characters.last() else {
        return String::new();
    };
    if UNSAFE.contains(&last) {
        return characters[..characters.len() - 1].iter().collect();
    }
    if last == '\n' && characters.len() >= 2 && UNSAFE.contains(&characters[characters.len() - 2]) {
        let mut cleaned = characters.clone();
        cleaned.remove(cleaned.len() - 2);
        return cleaned.into_iter().collect();
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanses_like_the_python_regex() {
        // Values checked against `CustomBoundriesBlacklist.cleanse_text`
        // (`[!,\.\?。！，？](?=$)` with `match_substrings=True`).
        assert_eq!(cleanse_text("la,"), "la");
        assert_eq!(cleanse_text("la,la"), "la,la");
        assert_eq!(cleanse_text("la, "), "la, ");
        assert_eq!(cleanse_text("la。"), "la");
        assert_eq!(cleanse_text("abc!"), "abc");
        assert_eq!(cleanse_text("x?\n"), "x\n");
        assert_eq!(cleanse_text("..."), "..");
        assert_eq!(cleanse_text("l?a"), "l?a");
        assert_eq!(cleanse_text("la，"), "la");
        assert_eq!(cleanse_text(""), "");
        // `$` also matches before a final `\n` only, not before `\r\n`.
        assert_eq!(cleanse_text("x?\r\n"), "x?\r\n");
    }

    #[test]
    fn parses_line_param_nodes() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&2i32.to_le_bytes());
        blob.extend_from_slice(&(-192000i32).to_le_bytes());
        blob.extend_from_slice(&0i32.to_le_bytes());
        blob.extend_from_slice(&1073741823i32.to_le_bytes());
        blob.extend_from_slice(&(-100i32).to_le_bytes());
        blob.resize(64, 0);
        assert_eq!(
            line_param_nodes(&blob).unwrap(),
            vec![(-192000, 0), (1073741823, -100)]
        );
        // `len(line_param) < 4` yields no nodes (Python's `__post_init__`).
        assert!(line_param_nodes(&[1, 2]).unwrap().is_empty());
        // A truncated body is an error, like `struct.unpack`.
        assert!(line_param_nodes(&blob[..12]).is_err());
        // A negative node count is invalid.
        let negative = (-1i32).to_le_bytes();
        assert!(line_param_nodes(&negative).is_err());
    }

    #[test]
    fn pitch_conversion_matches_python() {
        assert_eq!(pitch_from_svip(0), -100);
        assert_eq!(pitch_from_svip(1050), -100);
        assert_eq!(pitch_from_svip(1051), -99);
        assert_eq!(pitch_from_svip(1150), 0);
        assert_eq!(pitch_from_svip(1160), 10);
    }
}
