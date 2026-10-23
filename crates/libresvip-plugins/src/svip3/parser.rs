//! Port of `libresvip.plugins.svip3.svip3_parser`.

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::tick_counter::shift_tempo_list;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Phones, Project, SingingTrack, SongTempo,
    TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::constants::{AUDIO_TRACK_URL, SINGING_TRACK_URL};
use super::model::{
    ProtoAny, Svip3AudioTrack, Svip3LineParamNode, Svip3Note, Svip3Project, Svip3SingingPattern,
    Svip3SingingTrack, Svip3SongBeat, Svip3SongTempo,
};
use super::singers::singer_name;
use crate::ppsf::util::py_round;

pub struct Svip3Parser<'a> {
    options: &'a Options,
    first_bar_length: i64,
    song_tempo_list: Vec<SongTempo>,
}

impl<'a> Svip3Parser<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            first_bar_length: 0,
            song_tempo_list: Vec::new(),
        }
    }

    pub fn parse_project(mut self, svip3_project: Svip3Project) -> Result<Project> {
        let time_signature_list = Self::parse_time_signatures(&svip3_project.beat_list);
        let first_time_signature = time_signature_list
            .first()
            .ok_or_else(|| LibreSvipError::Validation("time signature list is empty".to_owned()))?;
        self.first_bar_length = py_round(first_time_signature.bar_length());
        self.song_tempo_list =
            Self::parse_song_tempos(&svip3_project.tempo_list, self.first_bar_length);
        let track_list = self.parse_tracks(&svip3_project.track_list)?;
        Ok(Project {
            version: String::new(),
            song_tempo_list: self.song_tempo_list.clone(),
            time_signature_list,
            track_list,
        })
    }

    /// `parse_time_signatures`.
    pub fn parse_time_signatures(beat_list: &[Svip3SongBeat]) -> Vec<TimeSignature> {
        beat_list
            .iter()
            .map(|beat| {
                let (numerator, denominator) = match &beat.beat_size {
                    Some(beat_size) => (beat_size.numerator as i64, beat_size.denominator as i64),
                    None => (0, 4),
                };
                TimeSignature::new(beat.pos as i64, numerator, denominator)
            })
            .collect()
    }

    /// `parse_song_tempos`.
    pub fn parse_song_tempos(
        tempo_list: &[Svip3SongTempo],
        first_bar_length: i64,
    ) -> Vec<SongTempo> {
        let tempos: Vec<SongTempo> = tempo_list
            .iter()
            .map(|tempo| SongTempo::new(tempo.pos as i64, f64::from(tempo.tempo) / 100.0))
            .collect();
        shift_tempo_list(&tempos, first_bar_length)
    }

    fn parse_tracks(&mut self, track_list: &[ProtoAny]) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        for track in track_list {
            if track.type_url == SINGING_TRACK_URL {
                let singing_track = Svip3SingingTrack::decode(&track.value)?;
                tracks.push(Track::Singing(self.parse_singing_track(&singing_track)?));
            } else if self.options.bool("import_instrumental_track", true)
                && track.type_url == AUDIO_TRACK_URL
            {
                let audio_track = Svip3AudioTrack::decode(&track.value)?;
                if let Some(instrumental) = self.parse_audio_track(&audio_track) {
                    tracks.push(Track::Instrumental(instrumental));
                }
            }
        }
        Ok(tracks)
    }

    fn parse_audio_track(&self, audio_track: &Svip3AudioTrack) -> Option<InstrumentalTrack> {
        let mut audio_file_path = None;
        let mut offset = 0i64;
        if let Some(first_pattern) = audio_track.pattern_list.first() {
            audio_file_path = Some(first_pattern.audio_file_path.clone());
            offset = i64::from(first_pattern.real_pos);
        }
        let audio_file_path = audio_file_path?;
        Some(InstrumentalTrack {
            title: audio_track.name.clone(),
            mute: audio_track.mute,
            solo: audio_track.solo,
            volume: to_linear_volume(f64::from(audio_track.volume)),
            pan: parse_pan(f64::from(audio_track.pan)),
            audio_file_path,
            offset,
            ..Default::default()
        })
    }

    fn parse_singing_track(&self, singing_track: &Svip3SingingTrack) -> Result<SingingTrack> {
        let mut pattern_list: Vec<&Svip3SingingPattern> =
            singing_track.pattern_list.iter().collect();
        // Python `sorted(..., key=attrgetter("pos"))` is stable.
        pattern_list.sort_by_key(|pattern| pattern.pos());
        let note_list = self.parse_notes(&pattern_list);
        let edited_params = self.parse_edited_params(&pattern_list)?;
        Ok(SingingTrack {
            title: singing_track.name.clone(),
            mute: singing_track.mute,
            solo: singing_track.solo,
            volume: to_linear_volume(f64::from(singing_track.volume)),
            pan: parse_pan(f64::from(singing_track.pan)),
            ai_singer_name: singer_name(&singing_track.ai_singer_id).to_owned(),
            note_list,
            edited_params,
            ..Default::default()
        })
    }

    fn parse_notes(&self, pattern_list: &[&Svip3SingingPattern]) -> Vec<Note> {
        let mut note_list = Vec::new();
        for pattern in pattern_list {
            let offset = i64::from(pattern.real_pos);
            let left = i64::from(pattern.play_pos) + offset;
            let right = left + i64::from(pattern.play_dur);
            let visible_notes = pattern.note_list.iter().filter(|note| {
                let start = i64::from(note.start_pos) + offset;
                left <= start && start <= right - i64::from(note.width_pos)
            });
            note_list.extend(visible_notes.map(|note| self.parse_note(note, offset)));
        }
        note_list
    }

    fn parse_edited_params(&self, pattern_list: &[&Svip3SingingPattern]) -> Result<Params> {
        let mut params = Params::default();
        if self.options.bool("import_pitch", true) {
            params.pitch = self.parse_pitch_curve(pattern_list);
        }
        Ok(params)
    }

    fn parse_note(&self, svip3_note: &Svip3Note, offset: i64) -> Note {
        Note {
            start_pos: i64::from(svip3_note.start_pos) + offset,
            length: i64::from(svip3_note.width_pos),
            key_number: i64::from(svip3_note.key_index),
            lyric: svip3_note.lyric.clone(),
            pronunciation: parse_pronunciation(svip3_note),
            head_tag: parse_head_tag(svip3_note),
            edited_phones: self.parse_edited_phones(svip3_note),
            vibrato: None,
        }
    }

    fn parse_edited_phones(&self, svip3_note: &Svip3Note) -> Option<Phones> {
        if svip3_note.consonant_len > 0 {
            let bpm = self.song_tempo_list.first()?.bpm;
            Some(Phones {
                head_length_in_secs: f64::from(svip3_note.consonant_len) / TICKS_IN_BEAT as f64
                    * 60.0
                    / bpm,
                mid_ratio_over_tail: -1.0,
            })
        } else {
            None
        }
    }

    fn parse_pitch_curve(&self, pattern_list: &[&Svip3SingingPattern]) -> ParamCurve {
        let curves: Vec<ParamCurve> = pattern_list
            .iter()
            .map(|pattern| self.parse_pattern_curve(pattern))
            .collect();
        let mut points = vec![Point::start_point()];
        for curve in curves {
            points.extend(curve.points);
        }
        points.push(Point::end_point());
        ParamCurve::new(points)
    }

    fn parse_pattern_curve(&self, pattern: &Svip3SingingPattern) -> ParamCurve {
        let mut curve = ParamCurve::default();
        let (left, right) = get_visible_range(pattern);
        let visible_nodes: Vec<&Svip3LineParamNode> = pattern
            .edited_pitch_line
            .iter()
            .filter(|node| left <= i64::from(node.pos) && i64::from(node.pos) <= right)
            .collect();
        curve.points.push(Point::new(left, -100));
        for node in visible_nodes {
            let pos = i64::from(node.pos) + self.first_bar_length;
            let value = f64::from(node.value);
            let val = if value != -1.0 {
                py_round(value * 100.0 - 50.0)
            } else {
                -100
            };
            curve.points.push(Point::new(pos, val));
        }
        curve.points.push(Point::new(right, -100));
        curve
    }
}

/// `get_visible_range`.
pub fn get_visible_range(pattern: &Svip3SingingPattern) -> (i64, i64) {
    let offset = i64::from(pattern.real_pos);
    let left = i64::from(pattern.play_pos) + offset;
    (left, left + i64::from(pattern.play_dur))
}

/// `parse_pronunciation`.
///
/// The reference falls back to the lyric when the file carries no pronouncing,
/// which turns the lyric into a "pronunciation" and lets generators copy it into
/// phoneme fields; keep it empty instead.
pub fn parse_pronunciation(svip3_note: &Svip3Note) -> Option<String> {
    if svip3_note.pronouncing.is_empty() {
        return None;
    }
    Some(svip3_note.pronouncing.clone())
}

/// `parse_head_tag`.
pub fn parse_head_tag(svip3_note: &Svip3Note) -> Option<String> {
    if svip3_note.sil_len > 0 {
        Some("0".to_owned())
    } else if svip3_note.sp_len > 0 {
        Some("V".to_owned())
    } else {
        None
    }
}

/// `parse_pan` - `(pan or 0) / 10.0`.
pub fn parse_pan(pan: f64) -> f64 {
    let pan = if pan == 0.0 { 0.0 } else { pan };
    pan / 10.0
}

/// `to_linear_volume`.
pub fn to_linear_volume(gain: f64) -> f64 {
    if gain >= 0.0 {
        (gain / ratio_to_db(4.0) + 1.0).min(2.0)
    } else {
        db_to_float(gain)
    }
}

/// `ratio_to_db` with the default amplitude flag.
pub fn ratio_to_db(ratio: f64) -> f64 {
    if ratio == 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * ratio.log10()
}

/// `db_to_float` with the default amplitude flag.
pub fn db_to_float(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}
