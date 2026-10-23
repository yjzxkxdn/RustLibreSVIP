//! Port of `libresvip.plugins.vfp.vox_factory_parser`.

use std::path::{Path, PathBuf};

use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature, Track, TICKS_IN_BEAT,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::model::{VoxClip, VoxProject};

pub struct VoxFactoryParser<'a> {
    options: &'a Options,
    path: &'a Path,
    synchronizer: Option<TimeSynchronizer>,
    first_bar_length: i64,
}

impl<'a> VoxFactoryParser<'a> {
    pub fn new(options: &'a Options, path: &'a Path) -> Self {
        Self {
            options,
            path,
            synchronizer: None,
            first_bar_length: 0,
        }
    }

    pub fn parse_project(&mut self, vox_project: &VoxProject) -> Result<Project> {
        let tempos = vec![SongTempo::new(0, vox_project.tempo)];
        self.synchronizer = Some(TimeSynchronizer::new(&tempos, 0, false, 60.0));
        let time_signature = self.parse_time_signature(&vox_project.time_signature)?;
        let track_list = self.parse_tracks(vox_project)?;
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: vec![time_signature],
            track_list,
        })
    }

    fn parse_time_signature(&mut self, time_signatures: &[i64]) -> Result<TimeSignature> {
        if time_signatures.len() < 2 {
            return Err(LibreSvipError::msg("invalid time signature"));
        }
        let time_signature = TimeSignature::new(0, time_signatures[0], time_signatures[1]);
        self.first_bar_length = time_signature.bar_length() as i64;
        Ok(time_signature)
    }

    fn parse_tracks(&self, vox_project: &VoxProject) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        for (track_name, track_data) in sorted_by_order(&vox_project.track_bank, &vox_project.track_order)? {
            if track_data.type_ == "audio" {
                if !self.options.bool("import_instrumental_track", true) {
                    continue;
                }
                for (_, clip_data) in sorted_by_order(&track_data.clip_bank, &track_data.clip_order)? {
                    let Some(source_audio_data_key) = clip_data.source_audio_data_key.as_ref() else {
                        continue;
                    };
                    if source_audio_data_key.is_empty() {
                        continue;
                    }
                    let audio_path = self.audio_path(&clip_data.name, source_audio_data_key);
                    if audio_path.exists() {
                        tracks.push(Track::Instrumental(InstrumentalTrack {
                            title: clip_data.name.clone(),
                            solo: track_data.solo,
                            mute: track_data.mute,
                            pan: track_data.pan,
                            offset: (TICKS_IN_BEAT as f64
                                * (clip_data.start_quarter - clip_data.offset_quarter))
                                as i64,
                            audio_file_path: String::new(),
                            ..Default::default()
                        }));
                    }
                }
            } else {
                let mut note_list = Vec::new();
                let mut pitch_points: Vec<Point> = Vec::new();
                for (_, clip_data) in sorted_by_order(&track_data.clip_bank, &track_data.clip_order)? {
                    let clip_offset = (TICKS_IN_BEAT as f64
                        * (clip_data.start_quarter - clip_data.offset_quarter))
                        as i64;
                    let (clip_notes, clip_pitch_points) =
                        self.parse_notes(clip_data, clip_offset)?;
                    note_list.extend(clip_notes);
                    pitch_points.extend(clip_pitch_points);
                }
                if note_list.is_empty() {
                    continue;
                }
                let mut track = SingingTrack {
                    title: track_name.clone(),
                    solo: track_data.solo,
                    mute: track_data.mute,
                    pan: track_data.pan,
                    note_list,
                    ..Default::default()
                };
                if self.options.bool("import_pitch", true) {
                    pitch_points.insert(0, Point::start_point());
                    pitch_points.push(Point::end_point());
                    track.edited_params.pitch.points = pitch_points;
                }
                tracks.push(Track::Singing(track));
            }
        }
        Ok(tracks)
    }

    fn audio_path(&self, name: &str, source_audio_data_key: &str) -> PathBuf {
        let base = self.path.parent().unwrap_or_else(|| Path::new(""));
        let mut path = base.join(name);
        let suffix = Path::new(source_audio_data_key)
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !name.is_empty() {
            if suffix.is_empty() {
                path.set_extension("");
            } else {
                path.set_extension(suffix);
            }
        }
        path
    }

    fn parse_notes(&self, clip_data: &VoxClip, clip_offset: i64) -> Result<(Vec<Note>, Vec<Point>)> {
        let mut notes: Vec<Note> = Vec::new();
        let mut pitch_points: Vec<Point> = Vec::new();
        let synchronizer = self
            .synchronizer
            .clone()
            .ok_or_else(|| LibreSvipError::msg("missing synchronizer"))?;
        let f0_secs_step = 1024.0 / 44100.0;
        for (_, note_data) in sorted_by_order(&clip_data.note_bank, &clip_data.note_order)? {
            let note = Note {
                start_pos: (note_data.ticks + clip_offset as f64) as i64,
                length: note_data.duration_ticks as i64,
                key_number: note_data.midi,
                lyric: note_data.name.clone(),
                pronunciation: Some(note_data.syllable.clone()),
                ..Default::default()
            };
            if let Some(previous) = notes.last_mut() {
                if note.start_pos < previous.end_pos() {
                    previous.length = note.start_pos - previous.start_pos;
                }
            }
            if self.options.bool("import_pitch", true) && !note_data.pitch_bends.is_empty() {
                let pitch_start =
                    (note_data.ticks + clip_offset as f64 + note_data.pre_bend.unwrap_or(0.0)) as i64;
                let mut note_pitch_points =
                    vec![Point::new(pitch_start + self.first_bar_length, -100)];
                let pitch_start_secs = synchronizer.get_actual_secs_from_ticks(pitch_start);
                let pitch_end_secs =
                    pitch_start_secs + f0_secs_step * note_data.pitch_bends.len() as f64;
                let pitch_end = synchronizer.get_actual_ticks_from_secs(pitch_end_secs);
                let pitch_step = (pitch_end - pitch_start as f64) / note_data.pitch_bends.len() as f64;
                for (i, pitch_bend) in note_data.pitch_bends.iter().enumerate() {
                    note_pitch_points.push(Point::new(
                        (pitch_start as f64 + i as f64 * pitch_step) as i64 + self.first_bar_length,
                        ((note_data.midi as f64 + pitch_bend) * 100.0) as i64,
                    ));
                }
                note_pitch_points.push(Point::new(
                    pitch_end as i64 + self.first_bar_length,
                    -100,
                ));
                pitch_points.extend(note_pitch_points);
            }
            notes.push(note);
        }
        Ok((notes, pitch_points))
    }
}

/// `sorted(mapping.items(), key=lambda item: order.index(item[0]))` — Python
/// raises `ValueError` for unknown keys, which becomes an error here.
fn sorted_by_order<'a, T>(
    mapping: &'a indexmap::IndexMap<String, T>,
    order: &[String],
) -> Result<Vec<(&'a String, &'a T)>> {
    let items: Vec<(&String, &T)> = mapping.iter().collect();
    let mut positions = Vec::with_capacity(items.len());
    for (key, _) in &items {
        let position = order
            .iter()
            .position(|item| item == *key)
            .ok_or_else(|| LibreSvipError::msg(format!("{key} not found in order list")))?;
        positions.push(position);
    }
    let mut indexed: Vec<(usize, (&String, &T))> = positions
        .into_iter()
        .zip(items.into_iter())
        .collect();
    indexed.sort_by_key(|(position, _)| *position);
    Ok(indexed.into_iter().map(|(_, item)| item).collect())
}
