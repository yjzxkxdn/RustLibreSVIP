//! Port of `libresvip.plugins.aisp.aisingers_parser`.

use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{AisAudioTrack, AisNote, AisProjectBody, AisProjectHead, AisTrack};
use super::pyutil::py_round;

pub struct AisParser {
    import_pitch: bool,
    import_instrumental: bool,
    first_bar_length: i64,
}

impl AisParser {
    pub fn new(options: &Options) -> Self {
        Self {
            import_pitch: options.bool("import_pitch", true),
            import_instrumental: options.bool("import_instrumental_track", true),
            first_bar_length: 0,
        }
    }

    pub fn parse_project(
        &mut self,
        head: &AisProjectHead,
        body: &AisProjectBody,
    ) -> Result<Project> {
        let time_signatures = self.parse_time_signatures(head);
        let first = time_signatures.first().ok_or_else(|| {
            LibreSvipError::Validation("AISingers project has no time signature".to_owned())
        })?;
        self.first_bar_length = py_round(first.bar_length());
        let tempos = self.parse_tempos(head)?;
        let track_list = self.parse_tracks(&body.tracks)?;
        Ok(Project {
            time_signature_list: time_signatures,
            song_tempo_list: tempos,
            track_list,
            ..Default::default()
        })
    }

    fn parse_time_signatures(&self, head: &AisProjectHead) -> Vec<TimeSignature> {
        head.signature
            .iter()
            .map(|signature| TimeSignature {
                bar_index: signature.start_bar,
                numerator: signature.beat_zi,
                denominator: signature.beat_mu,
            })
            .collect()
    }

    fn parse_tempos(&self, head: &AisProjectHead) -> Result<Vec<SongTempo>> {
        head.tempo
            .iter()
            .map(|tempo| {
                let bpm = tempo.tempo_float.ok_or_else(|| {
                    LibreSvipError::Validation("AISingers tempo without a value".to_owned())
                })?;
                Ok(SongTempo {
                    position: tempo.start_128 * 15,
                    bpm,
                })
            })
            .collect()
    }

    fn parse_tracks(&self, tracks: &[AisTrack]) -> Result<Vec<Track>> {
        let mut track_list = Vec::new();
        for ais_track in tracks {
            match ais_track {
                AisTrack::Sing(track) => {
                    let mut note_list = Vec::new();
                    let mut pitch_points = vec![Point::start_point()];
                    for item in &track.items {
                        let tick_prefix = item.start * 15;
                        let (item_notes, item_pitch_points) =
                            self.parse_notes(&item.notes, tick_prefix)?;
                        note_list.extend(item_notes);
                        // Quirk of the reference implementation: the variable
                        // is rebound per item, then extended with itself.
                        let mut item_pitch_points = item_pitch_points;
                        item_pitch_points.extend(item_pitch_points.clone());
                        pitch_points = item_pitch_points;
                    }
                    pitch_points.push(Point::end_point());
                    let mut singing_track = SingingTrack {
                        title: track.base.name.clone().unwrap_or_default(),
                        ai_singer_name: track.singer_namecn.clone().unwrap_or_default(),
                        mute: track.base.mute.unwrap_or(false),
                        solo: track.base.solo.unwrap_or(false),
                        note_list,
                        ..Default::default()
                    };
                    if pitch_points.len() > 2 {
                        singing_track.edited_params.pitch = ParamCurve::new(pitch_points);
                    }
                    track_list.push(Track::Singing(singing_track));
                }
                AisTrack::Audio(track) if self.import_instrumental => {
                    self.parse_audio_track(track, &mut track_list);
                }
                _ => {}
            }
        }
        Ok(track_list)
    }

    fn parse_audio_track(&self, track: &AisAudioTrack, track_list: &mut Vec<Track>) {
        let name = track.base.name.clone().unwrap_or_else(|| "None".to_owned());
        for (index, item) in track.items.iter().enumerate() {
            track_list.push(Track::Instrumental(InstrumentalTrack {
                title: format!("{name} ({})", index + 1),
                mute: track.base.mute.unwrap_or(false),
                solo: track.base.solo.unwrap_or(false),
                audio_file_path: item.path_audio.clone().unwrap_or_default(),
                offset: item.start * 15,
                ..Default::default()
            }));
        }
    }

    fn parse_notes(&self, ais_notes: &[AisNote], tick_prefix: i64) -> Result<(Vec<Note>, Vec<Point>)> {
        let mut note_list = Vec::new();
        let mut pitch_points = Vec::new();
        for ais_note in ais_notes {
            let start_pos = ais_note.start * 15 + tick_prefix;
            let length = ais_note.length * 15;
            let key_number = ais_note.midi_no + 12;
            let mut note = Note::new(
                start_pos,
                length,
                key_number,
                ais_note.lyric.clone().unwrap_or_default(),
            );
            note.pronunciation = ais_note.pinyin.clone();
            if self.import_pitch && !ais_note.pit.is_empty() {
                let tick_step = length as f64 / ais_note.pit.len() as f64;
                pitch_points.push(Point::new(start_pos + self.first_bar_length, -100));
                for (index, sample) in ais_note.pit.iter().enumerate() {
                    let tick = start_pos as f64 + tick_step * index as f64;
                    let x = py_round(tick) + self.first_bar_length;
                    let y = py_round(key_number as f64 * 100.0 + sample * 10.0);
                    pitch_points.push(Point::new(x, y));
                }
                pitch_points.push(Point::new(
                    start_pos + length + self.first_bar_length,
                    -100,
                ));
            }
            note_list.push(note);
        }
        Ok((note_list, pitch_points))
    }
}
