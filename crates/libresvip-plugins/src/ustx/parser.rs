//! Port of `libresvip.plugins.ustx.ustx_parser`.

use std::path::Path;

use libresvip_core::core::lyric_phoneme::chinese;
use libresvip_core::core::lyric_phoneme::japanese;
use libresvip_core::core::tick_counter::find_bar_index;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track, DEFAULT_BPM,
};
use libresvip_core::model::point::Point;
use libresvip_core::{LibreSvipError, Result};

use crate::ust::math::db_to_float;

use super::base_pitch::BasePitchGenerator;
use super::model::{UNote, UTimeSignature, UTempo, UTrack, UVoicePart, UWavePart, UstxProject};

/// `PlusHandlingMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlusHandlingMode {
    Auto,
    Monosyllabic,
    Polysyllabic,
}

impl PlusHandlingMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "monosyllabic" => Self::Monosyllabic,
            "polysyllabic" => Self::Polysyllabic,
            _ => Self::Auto,
        }
    }
}

/// `InputOptions`.
pub struct InputOptions {
    pub import_pitch: bool,
    pub import_instrumental_track: bool,
    pub plus_handling_mode: PlusHandlingMode,
    pub breath_lyrics: String,
    pub silence_lyrics: String,
}

impl Default for InputOptions {
    fn default() -> Self {
        Self {
            import_pitch: true,
            import_instrumental_track: true,
            plus_handling_mode: PlusHandlingMode::Auto,
            breath_lyrics: "Asp AP".to_owned(),
            silence_lyrics: "R SP".to_owned(),
        }
    }
}

/// `PHONETIC_HINT_RE = re.compile(r"\[(.*?)\]")`.
fn phonetic_hint(lyric: &str) -> Option<String> {
    let start = lyric.find('[')?;
    let rest = &lyric[start + 1..];
    let end = rest.find(']')?;
    Some(rest[..end].to_owned())
}

/// Port of `UstxParser`.
pub struct UstxParser<'a> {
    options: &'a InputOptions,
    time_signatures: Vec<TimeSignature>,
    breath_lyrics: Vec<String>,
    silence_lyrics: Vec<String>,
    base_pitch_generator: BasePitchGenerator,
}

impl<'a> UstxParser<'a> {
    fn is_monosyllabic_mode(&self, phonemizer: Option<&str>) -> bool {
        match self.options.plus_handling_mode {
            PlusHandlingMode::Monosyllabic => return true,
            PlusHandlingMode::Polysyllabic => return false,
            PlusHandlingMode::Auto => {}
        }
        let Some(phonemizer) = phonemizer else {
            return false;
        };
        let phonemizer = phonemizer.to_lowercase();
        ["chinese", "japanese", "korean", "cantonese", "vietnamese"]
            .iter()
            .any(|language| phonemizer.contains(language))
    }

    /// `parse_project`.
    pub fn parse_project(&mut self, ustx_project: &UstxProject) -> Result<Project> {
        self.breath_lyrics = self
            .options
            .breath_lyrics
            .trim()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        self.silence_lyrics = self
            .options
            .silence_lyrics
            .trim()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let tempos = Self::parse_tempos(&ustx_project.tempos);
        self.time_signatures = Self::parse_time_signatures(&ustx_project.time_signatures);
        let mut tracks =
            self.parse_tracks(&ustx_project.tracks, &ustx_project.voice_parts)?;
        for track in &mut tracks {
            if let Track::Singing(singing_track) = track {
                singing_track.edited_params.pitch.points.push(Point::end_point());
            }
        }
        tracks.extend(self.parse_wave_parts(&ustx_project.tracks, &ustx_project.wave_parts)?);
        Ok(Project {
            warnings: Vec::new(),
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: self.time_signatures.clone(),
            track_list: tracks,
        })
    }

    fn parse_tempos(tempos: &[UTempo]) -> Vec<SongTempo> {
        let mut song_tempo_list: Vec<SongTempo> = tempos
            .iter()
            .map(|tempo| SongTempo {
                position: if tempo.position > 0 {
                    tempo.position + 1920
                } else {
                    tempo.position
                },
                bpm: tempo.bpm,
            })
            .collect();
        if song_tempo_list.is_empty() {
            song_tempo_list.push(SongTempo::new(0, DEFAULT_BPM));
        }
        song_tempo_list
    }

    fn parse_time_signatures(time_signatures: &[UTimeSignature]) -> Vec<TimeSignature> {
        let mut time_signature_list: Vec<TimeSignature> = time_signatures
            .iter()
            .map(|time_signature| TimeSignature {
                bar_index: time_signature.bar_position,
                numerator: time_signature.beat_per_bar,
                denominator: time_signature.beat_unit,
            })
            .collect();
        if time_signature_list.is_empty() {
            time_signature_list.push(TimeSignature::default());
        }
        time_signature_list
    }

    fn parse_tracks(&mut self, tracks: &[UTrack], voice_parts: &[UVoicePart]) -> Result<Vec<Track>> {
        let mut track_list: Vec<SingingTrack> = tracks
            .iter()
            .enumerate()
            .map(|(index, ustx_track)| SingingTrack {
                volume: parse_volume(ustx_track.volume),
                solo: ustx_track.solo,
                mute: ustx_track.mute,
                ai_singer_name: ustx_track.singer.clone().unwrap_or_default(),
                title: ustx_track
                    .track_name
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| format!("Track {}", index + 1)),
                ..Default::default()
            })
            .collect();
        if self.options.import_pitch {
            for track in &mut track_list {
                track.edited_params.pitch.points.push(Point::start_point());
            }
        }
        for voice_part in voice_parts {
            let track_index = voice_part.track_no;
            let resolved_index = if track_index < 0 {
                let from_end = track_list.len() as i64 + track_index;
                if from_end < 0 {
                    return Err(LibreSvipError::msg(format!(
                        "list index out of range: {track_index}"
                    )));
                }
                from_end as usize
            } else if (track_index as usize) < track_list.len() {
                track_index as usize
            } else {
                continue;
            };
            let phonemizer = tracks
                .get(resolved_index)
                .ok_or_else(|| {
                    LibreSvipError::msg(format!("list index out of range: {track_index}"))
                })?
                .phonemizer
                .clone();
            let monosyllabic_mode = self.is_monosyllabic_mode(phonemizer.as_deref());
            let notes = self.parse_notes(&voice_part.notes, voice_part.position, monosyllabic_mode)?;
            let singing_track = &mut track_list[resolved_index];
            if singing_track.title.is_empty() {
                singing_track.title = voice_part.name.clone();
            }
            singing_track.note_list.extend(notes);
            if self.options.import_pitch {
                let pitch = self.parse_pitch(voice_part);
                singing_track.edited_params.pitch.points.extend(pitch);
            }
        }
        Ok(track_list
            .into_iter()
            .filter(|track| !track.note_list.is_empty())
            .map(Track::Singing)
            .collect())
    }

    /// `parse_pitch`.
    fn parse_pitch(&self, part: &UVoicePart) -> Vec<Point> {
        let pitch_start = self.base_pitch_generator.pitch_start;
        let pitch_interval = self.base_pitch_generator.pitch_interval;
        let first_bar_length = 1920i64;

        let mut pitches = self.base_pitch_generator.base_pitch(part);

        let mut deviation_applied = false;
        let curve = part
            .curves
            .iter()
            .find(|curve| curve.abbr.as_deref() == Some("pitd"));
        if let Some(curve) = curve {
            if !curve.is_empty() {
                for (index, pitch) in pitches.iter_mut().enumerate() {
                    *pitch += curve.sample(pitch_start + index as i64 * pitch_interval) as f64;
                }
                deviation_applied = true;
            }
        }

        // USTX 0.10 masked curves carry absolute pitch in cents, the same unit
        // the points below use, and only where the user drew them. `pito` is
        // the user's own override and wins; `rpit` holds what the renderer
        // produced and is only a fallback for projects that have no `pitd`.
        let masked = |abbr: &str| {
            part.masked_curves
                .iter()
                .find(|curve| curve.abbr == abbr)
                .filter(|curve| !curve.is_empty())
        };
        let absolute = match masked("pito") {
            Some(curve) => Some(curve),
            None if !deviation_applied => masked("rpit"),
            None => None,
        };
        if let Some(curve) = absolute {
            for (index, pitch) in pitches.iter_mut().enumerate() {
                let tick = (pitch_start + index as i64 * pitch_interval) as f64;
                if let Some(value) = curve.try_sample(tick, pitch_interval) {
                    *pitch = value;
                }
            }
        }

        let mut point_list = vec![Point::new(first_bar_length + part.position, -100)];
        point_list.extend(pitches.iter().enumerate().map(|(index, pitch)| {
            Point::new(
                first_bar_length + part.position + index as i64 * pitch_interval,
                *pitch as i64,
            )
        }));
        point_list.push(Point::new(
            first_bar_length + part.position + pitches.len() as i64 * pitch_interval,
            -100,
        ));
        point_list
    }

    /// `parse_notes`.
    fn parse_notes(
        &self,
        notes: &[UNote],
        tick_prefix: i64,
        monosyllabic_mode: bool,
    ) -> Result<Vec<Note>> {
        let mut note_list: Vec<Note> = Vec::new();
        let mut prev_ustx_note: Option<&UNote> = None;
        for ustx_note in notes {
            let mut note_lyric = ustx_note.lyric.clone();
            if note_lyric.starts_with('+') {
                if monosyllabic_mode || note_lyric == "+~" || note_lyric == "+*" {
                    note_lyric = "-".to_owned();
                } else {
                    note_lyric = "+".to_owned();
                }
            }
            let mut note = Note {
                key_number: ustx_note.tone,
                lyric: note_lyric,
                start_pos: ustx_note.position + tick_prefix,
                length: ustx_note.duration,
                ..Default::default()
            };
            if !ustx_note.lyric.is_empty() {
                if let Some(hint) = phonetic_hint(&ustx_note.lyric) {
                    note.pronunciation = Some(hint);
                } else if japanese::is_kana(&ustx_note.lyric) {
                    // `wanakana.to_romaji` is not ported in the core crate yet.
                    note.pronunciation = japanese::to_romaji(&ustx_note.lyric).or_else(|| {
                        show_warning_kind(WarningKind::FallbackApplied, "Kana->Romaji conversion is not implemented yet");
                        None
                    });
                } else if chinese::contains_chinese(&ustx_note.lyric) {
                    // `pypinyin` data is not ported in the core crate yet.
                    let chinese_char = ustx_note
                        .lyric
                        .chars()
                        .find(|character| chinese::is_chinese_char(*character));
                    if let Some(chinese_char) = chinese_char {
                        match chinese::lazy_pinyin(&chinese_char.to_string()) {
                            Ok(pinyin) => note.pronunciation = Some(pinyin.join(" ")),
                            Err(_) => {
                                show_warning_kind(WarningKind::FallbackApplied,
                                    "Hanzi->Pinyin conversion is not implemented yet",
                                );
                            }
                        }
                    }
                } else if let Some(hint) = ustx_note.lyric.strip_prefix('?') {
                    // Only an explicit `?phonetic-hint` lyric carries a
                    // pronunciation. A plain romanised lyric (`wo`, `la`, …) *is*
                    // the pronunciation, and echoing it into the model made every
                    // generator that prefers `Note.pronunciation` copy pinyin into
                    // a phoneme field that engines cannot parse.
                    note.pronunciation = Some(hint.to_owned());
                }
            }
            if let Some(prev) = prev_ustx_note {
                if prev.end() > ustx_note.position {
                    let bar = find_bar_index(&self.time_signatures, note.start_pos)?;
                    return Err(LibreSvipError::msg(format!(
                        "Notes overlapped near bar {bar}"
                    )));
                } else if prev.end() == ustx_note.position
                    && self.breath_lyrics.contains(&prev.lyric)
                {
                    note.head_tag = Some("V".to_owned());
                }
            }
            if !self.breath_lyrics.contains(&ustx_note.lyric)
                && !self.silence_lyrics.contains(&ustx_note.lyric)
            {
                note_list.push(note);
            }
            prev_ustx_note = Some(ustx_note);
        }
        Ok(note_list)
    }

    /// `parse_wave_parts`.
    fn parse_wave_parts(
        &self,
        tracks: &[UTrack],
        wave_parts: &[UWavePart],
    ) -> Result<Vec<Track>> {
        let mut track_list: Vec<Track> = Vec::new();
        if !self.options.import_instrumental_track {
            return Ok(track_list);
        }
        for wave_part in wave_parts {
            let ustx_track = tracks.get(wave_part.track_no as usize).ok_or_else(|| {
                LibreSvipError::msg(format!("list index out of range: {}", wave_part.track_no))
            })?;
            track_list.push(Track::Instrumental(InstrumentalTrack {
                audio_file_path: wave_part.relative_path.clone(),
                offset: wave_part.position,
                title: wave_part.name.clone(),
                mute: ustx_track.mute,
                solo: ustx_track.solo,
                volume: parse_volume(ustx_track.volume),
                ..Default::default()
            }));
        }
        Ok(track_list)
    }
}

/// `parse_volume`.
fn parse_volume(volume: f64) -> f64 {
    db_to_float(volume, false).min(2.0)
}

/// Build a parser and run it over a deserialised project.
pub fn parse_project(ustx_project: &UstxProject, options: &InputOptions) -> Result<Project> {
    let mut parser = UstxParser {
        options,
        time_signatures: Vec::new(),
        breath_lyrics: Vec::new(),
        silence_lyrics: Vec::new(),
        base_pitch_generator: BasePitchGenerator::new(ustx_project)?,
    };
    parser.parse_project(ustx_project)
}

/// Resolve `UWavePart.relative_path` the way pydantic's `audio_path_validator`
/// does: relative paths are resolved against the project directory.
pub fn resolve_audio_paths(project: &mut UstxProject, project_path: &Path) {
    let parent = project_path.parent().map(Path::to_path_buf);
    for wave_part in &mut project.wave_parts {
        let path = std::path::Path::new(&wave_part.relative_path);
        if path.is_absolute() {
            continue;
        }
        let Some(parent) = parent.as_ref() else {
            continue;
        };
        let joined = parent.join(path);
        wave_part.relative_path = normalize_path(&joined).to_string_lossy().into_owned();
    }
}

/// Lexical `.`/`..` normalisation (Python's `Path.resolve` also resolves
/// symlinks, which is not needed for the supported use cases).
fn normalize_path(path: &Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use libresvip_core::model::base::{Project, Track};

    use super::*;
    use crate::ustx::model::{UMaskedCurve, UMaskedRun};

    /// A one-note project; the extra keys mirror what OpenUtau 1.571 writes.
    fn project_yaml(part_extra: &str, note_extra: &str) -> String {
        format!(
            "ustx_version: '0.10'\n\
             tempos:\n\
             - position: 0\n\
             \x20 bpm: 120.0\n\
             time_signatures:\n\
             - bar_position: 0\n\
             \x20 beat_per_bar: 4\n\
             \x20 beat_unit: 4\n\
             tracks:\n\
             - track_name: Track 1\n\
             \x20 singer: Singer\n\
             voice_parts:\n\
             - name: Track 1\n\
             \x20 position: 0\n\
             \x20 track_no: 0\n\
             \x20 notes:\n\
             \x20 - position: 0\n\
             \x20   duration: 480\n\
             \x20   tone: 60\n\
             \x20   lyric: la\n\
             {note_extra}\
             {part_extra}"
        )
    }

    fn parse(yaml: &str) -> Project {
        let project: UstxProject = serde_yaml::from_str(yaml).expect("yaml parses");
        parse_project(&project, &InputOptions::default()).expect("project converts")
    }

    /// Pitch values of the first singing track, without the `-100` edge markers.
    fn pitch_values(project: &Project) -> Vec<i64> {
        let Track::Singing(track) = &project.track_list[0] else {
            panic!("expected a singing track");
        };
        track
            .edited_params
            .pitch
            .points
            .iter()
            .map(|point| point.y)
            .filter(|value| *value != -100)
            .collect()
    }

    #[test]
    fn masked_pitch_overrides_the_base_pitch() {
        let yaml = project_yaml(
            "  masked_curves:\n\
             \x20 - abbr: pito\n\
             \x20   runs:\n\
             \x20   - x: 0\n\
             \x20     ys:\n\
             \x20     - 6000.0\n\
             \x20     - 6200.0\n\
             \x20     - 6300.0\n",
            "",
        );
        let values = pitch_values(&parse(&yaml));
        assert_eq!(values.len(), 97);
        assert_eq!(&values[..3], &[6000, 6200, 6300]);
        // Outside the drawn run the base pitch comes back.
        assert_eq!(values[3], 6000);
    }

    #[test]
    fn pitd_deviation_and_pito_override_combine() {
        let yaml = project_yaml(
            "  curves:\n\
             \x20 - abbr: pitd\n\
             \x20   xs:\n\
             \x20   - 0\n\
             \x20   - 475\n\
             \x20   ys:\n\
             \x20   - 0\n\
             \x20   - 100\n\
             \x20 masked_curves:\n\
             \x20 - abbr: pito\n\
             \x20   runs:\n\
             \x20   - x: 0\n\
             \x20     ys:\n\
             \x20     - 5500.0\n\
             \x20     - 5500.0\n",
            "",
        );
        let values = pitch_values(&parse(&yaml));
        // The masked curve wins where it has values …
        assert_eq!(values[0], 5500);
        assert_eq!(values[1], 5500);
        // … and the deviation keeps applying after its run ends at tick 5.
        assert!(values[2] > 6000, "pitd should still raise the pitch");
    }

    #[test]
    fn rendered_pitch_is_only_a_fallback() {
        let yaml = project_yaml(
            "  curves:\n\
             \x20 - abbr: pitd\n\
             \x20   xs:\n\
             \x20   - 0\n\
             \x20   - 475\n\
             \x20   ys:\n\
             \x20   - 50\n\
             \x20   - 50\n\
             \x20 masked_curves:\n\
             \x20 - abbr: rpit\n\
             \x20   runs:\n\
             \x20   - x: 0\n\
             \x20     ys:\n\
             \x20     - 5000.0\n\
             \x20     - 5000.0\n",
            "",
        );
        // `pitd` has values, so the rendered pitch must not replace it.
        let values = pitch_values(&parse(&yaml));
        assert_eq!(values[0], 6050);
    }

    #[test]
    fn null_part_lists_are_accepted() {
        let yaml = "ustx_version: '0.10'\ntracks: []\nvoice_parts: null\nwave_parts: null\n";
        let project = parse(yaml);
        assert!(project.track_list.is_empty());
    }

    #[test]
    fn note_tuning_shifts_the_pitch() {
        let yaml = project_yaml("", "    tuning: 50\n");
        let model: UstxProject = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            model.voice_parts[0].notes[0].tuning, 50,
            "tuning must survive yaml parsing"
        );
        let values = pitch_values(&parse(&yaml));
        assert_eq!(values[0], 6050);
    }

    #[test]
    fn expression_registry_with_masked_curve_type_is_tolerated() {
        // The shape OpenUtau 1.571 writes: the registry carries a `type` tag
        // (`Curve` / `Numerical` / `Options` / `MaskedCurve`) and the track
        // keeps descriptors, not values. A reader expecting the old `value`
        // field rejects the file outright.
        let yaml = "ustx_version: '0.10'\n\
             tempos:\n\
             - position: 0\n\
             \x20 bpm: 120.0\n\
             expressions:\n\
             \x20 pitd:\n\
             \x20   name: pitch deviation (curve)\n\
             \x20   abbr: pitd\n\
             \x20   type: Curve\n\
             \x20   min: -1200\n\
             \x20   max: 1200\n\
             \x20   default_value: 0\n\
             \x20 pito:\n\
             \x20   name: pitch override (masked curve)\n\
             \x20   abbr: pito\n\
             \x20   type: MaskedCurve\n\
             \x20   min: 2400\n\
             \x20   max: 10800\n\
             \x20   default_value: 6000\n\
             tracks:\n\
             - track_name: Track 1\n\
             \x20 singer: Singer\n\
             \x20 track_expressions:\n\
             \x20 - name: breath\n\
             \x20   abbr: bre\n\
             \x20   type: Numerical\n\
             \x20   min: 0\n\
             \x20   max: 100\n\
             \x20   default_value: 0\n\
             voice_parts:\n\
             - name: Track 1\n\
             \x20 position: 0\n\
             \x20 track_no: 0\n\
             \x20 notes:\n\
             \x20 - position: 0\n\
             \x20   duration: 480\n\
             \x20   tone: 60\n\
             \x20   lyric: la\n";
        let project = parse(yaml);
        assert_eq!(project.track_list.len(), 1);
        let singing = project.track_list[0].as_singing().expect("singing track");
        assert_eq!(singing.title, "Track 1");
        assert_eq!(singing.note_list.len(), 1);
    }

    #[test]
    fn masked_curve_samples_only_inside_its_runs() {
        let curve = UMaskedCurve {
            abbr: "pito".to_owned(),
            runs: vec![
                UMaskedRun {
                    x: 0,
                    ys: vec![1.0, 2.0],
                },
                UMaskedRun {
                    x: 100,
                    ys: vec![5.0],
                },
            ],
        };
        assert_eq!(curve.try_sample(0.0, 5), Some(1.0));
        assert_eq!(curve.try_sample(2.5, 5), Some(1.5));
        assert_eq!(curve.try_sample(5.0, 5), Some(2.0));
        assert_eq!(curve.try_sample(7.5, 5), None);
        assert_eq!(curve.try_sample(100.0, 5), Some(5.0));
    }
}