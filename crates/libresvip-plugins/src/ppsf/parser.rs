//! Port of `libresvip.plugins.ppsf.piapro_studio_nt_parser`.

use libresvip_core::core::lyric_phoneme::japanese::is_legato_char;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    InstrumentalTrack, Note, Project, SingingTrack, SongTempo, TimeSignature,
};
use libresvip_core::model::point::Point;
use libresvip_core::options::Options;

use super::interval_dict::ppsf_key_interval_dict;
use super::model::{
    PpsfAudioTrackItem, PpsfCurveType, PpsfDvlTrackEvent, PpsfDvlTrackItem, PpsfEventTrack,
    PpsfMeters, PpsfProject, PpsfTempos,
};
use super::util::py_round;

pub struct PiaproStudioNtParser<'a> {
    options: &'a Options,
    first_bar_ticks: i64,
}

impl<'a> PiaproStudioNtParser<'a> {
    pub fn new(options: &'a Options) -> Self {
        Self {
            options,
            first_bar_ticks: 0,
        }
    }

    pub fn parse_project(mut self, ppsf_project: PpsfProject) -> Result<Project> {
        let time_signatures = parse_time_signatures(&ppsf_project.ppsf.project.meter);
        self.first_bar_ticks = py_round(
            time_signatures
                .first()
                .map(TimeSignature::bar_length)
                .unwrap_or_default(),
        );
        let tempos = parse_tempos(&ppsf_project.ppsf.project.tempo);
        let singing_tracks = self.parse_singing_tracks(
            ppsf_project.ppsf.project.dvl_track.as_deref(),
            &ppsf_project.ppsf.gui_settings.track_editor.event_tracks,
        )?;
        let instrumental_tracks =
            self.parse_instrumental_tracks(&ppsf_project.ppsf.project.audio_track);
        let mut track_list: Vec<libresvip_core::model::base::Track> = singing_tracks
            .into_iter()
            .map(libresvip_core::model::base::Track::Singing)
            .collect();
        track_list.extend(
            instrumental_tracks
                .into_iter()
                .map(libresvip_core::model::base::Track::Instrumental),
        );
        Ok(Project {
            version: String::new(),
            song_tempo_list: tempos,
            time_signature_list: time_signatures,
            track_list,
        })
    }

    fn parse_instrumental_tracks(
        &self,
        ppsf_audio_tracks: &[PpsfAudioTrackItem],
    ) -> Vec<InstrumentalTrack> {
        let mut tracks = Vec::new();
        if self.options.bool("import_instrumental_track", true) {
            for track in ppsf_audio_tracks {
                for (index, event) in track.events.iter().enumerate() {
                    tracks.push(InstrumentalTrack {
                        title: format!("{} {}", track.name, index + 1),
                        audio_file_path: event.file_audio_data.file_path.clone(),
                        offset: event.tick_pos,
                        ..Default::default()
                    });
                }
            }
        }
        tracks
    }

    fn parse_singing_tracks(
        &self,
        ppsf_dvl_tracks: Option<&[PpsfDvlTrackItem]>,
        event_tracks: &[PpsfEventTrack],
    ) -> Result<Vec<SingingTrack>> {
        let mut tracks = Vec::new();
        if let Some(dvl_tracks) = ppsf_dvl_tracks {
            for (track, event_track) in dvl_tracks.iter().zip(event_tracks.iter()) {
                let mut singing_track = SingingTrack {
                    title: track.name.clone(),
                    ai_singer_name: track.singer.singer_name.clone(),
                    note_list: parse_notes(&track.events),
                    solo: event_track.mute_solo > 0,
                    mute: event_track.mute_solo < 0,
                    ..Default::default()
                };
                for parameter in &track.parameters {
                    let Some(base_sequence) = parameter.base_sequence.as_ref() else {
                        continue;
                    };
                    if self.options.bool("import_pitch", true)
                        && base_sequence.name == "pitch_bend"
                    {
                        let key_interval_dict =
                            ppsf_key_interval_dict(&track.events, &event_track.notes);
                        for point in &base_sequence.sequence {
                            if point.curve_type == PpsfCurveType::BORDER && point.value == 0 {
                                singing_track.edited_params.pitch.points.push(Point::new(
                                    point.pos + self.first_bar_ticks,
                                    -100,
                                ));
                            } else if let Some(base_key) = key_interval_dict.get(point.pos) {
                                singing_track.edited_params.pitch.points.push(Point::new(
                                    point.pos + self.first_bar_ticks,
                                    point.value + py_round(base_key * 100.0),
                                ));
                            }
                        }
                    }
                }
                tracks.push(singing_track);
            }
        }
        Ok(tracks)
    }
}

/// `parse_time_signatures`.
pub fn parse_time_signatures(ppsf_meters: &PpsfMeters) -> Vec<TimeSignature> {
    let mut time_signatures: Vec<TimeSignature> = Vec::new();
    let first_time_signature = TimeSignature::new(
        0,
        ppsf_meters.const_meter.nume,
        ppsf_meters.const_meter.denomi,
    );
    if ppsf_meters.use_sequence {
        time_signatures.extend(ppsf_meters.sequence.iter().map(|meter| {
            TimeSignature::new(meter.measure, meter.nume, meter.denomi)
        }));
    }
    if time_signatures.first().map(|signature| signature.bar_index) != Some(0) {
        time_signatures.insert(0, first_time_signature);
    }
    time_signatures
}

/// `parse_tempos`.
pub fn parse_tempos(ppsf_tempos: &PpsfTempos) -> Vec<SongTempo> {
    let mut tempos: Vec<SongTempo> = Vec::new();
    let first_tempo = SongTempo::new(0, ppsf_tempos.const_tempo as f64 / 10_000.0);
    if ppsf_tempos.use_sequence {
        tempos.extend(ppsf_tempos.sequence.iter().map(|tempo| {
            SongTempo::new(tempo.tick, tempo.value as f64 / 10_000.0)
        }));
    }
    if tempos.first().map(|tempo| tempo.position) != Some(0) {
        tempos.insert(0, first_tempo);
    }
    tempos
}

/// `parse_notes`.
pub fn parse_notes(ppsf_dvl_track_events: &[PpsfDvlTrackEvent]) -> Vec<Note> {
    ppsf_dvl_track_events
        .iter()
        .filter(|event| event.enabled.unwrap_or(true))
        .map(|event| Note {
            key_number: event.note_number,
            start_pos: event.pos,
            length: event.length,
            lyric: if is_legato_char(&event.lyric) {
                "-".to_owned()
            } else {
                event.lyric.clone()
            },
            pronunciation: Some(event.symbols.clone()),
            ..Default::default()
        })
        .collect()
}
