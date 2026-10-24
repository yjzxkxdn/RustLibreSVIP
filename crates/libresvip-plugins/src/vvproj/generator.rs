//! Port of `libresvip.plugins.vvproj.voicevox_generator`.

use libresvip_core::core::lyric_phoneme::japanese::is_kana;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, ParamCurve, Project, SongTempo, TimeSignature, Track};
use libresvip_core::utils::text::uuid_str;

use super::model::{
    VoiceVoxNote, VoiceVoxProject, VoiceVoxSong, VoiceVoxTempo, VoiceVoxTimeSignature, VoiceVoxTrack,
};
use super::music::{midi2hz, numeric_range, py_round, PiecewiseIntervalDict};

#[derive(Clone, Debug, Default)]
pub struct OutputOptions;

/// Plugin-scoped `gettext`: the CLI merges the plugin catalog into the core
/// one, the converter itself only sees the core catalog.
fn translate(message: &str) -> String {
    use libresvip_core::core::config::read_settings;
    use libresvip_core::utils::translation::{core_catalog, plugin_catalog, TranslationChain};
    let lang = read_settings(|settings| settings.language.value()).to_owned();
    let mut chain = TranslationChain::new();
    chain.push(core_catalog(&lang));
    chain.push(plugin_catalog("vvproj", &lang));
    chain.gettext(message)
}

struct Generator {
    first_bar_length: i64,
    time_synchronizer: TimeSynchronizer,
}

pub fn generate_project(project: &Project, _options: &OutputOptions) -> Result<VoiceVoxProject> {
    let Some(first_signature) = project.time_signature_list.first() else {
        return Err(LibreSvipError::Validation(
            "project has no time signatures".to_owned(),
        ));
    };
    let generator = Generator {
        first_bar_length: first_signature.bar_length() as i64,
        time_synchronizer: TimeSynchronizer::from_project(&project.song_tempo_list),
    };
    let mut song = VoiceVoxSong {
        tempos: generate_tempos(&project.song_tempo_list),
        time_signatures: generate_time_signatures(&project.time_signature_list),
        ..Default::default()
    };
    for track in &project.track_list {
        if let Track::Singing(track) = track {
            let track_key = uuid_str();
            song.track_order.push(track_key.clone());
            song.tracks.push((track_key, generator.generate_track(track)));
        }
    }
    Ok(VoiceVoxProject {
        app_version: "0.21.1".to_owned(),
        song,
    })
}

fn generate_time_signatures(
    time_signature_list: &[TimeSignature],
) -> Vec<VoiceVoxTimeSignature> {
    time_signature_list
        .iter()
        .map(|time_signature| {
            VoiceVoxTimeSignature::new(
                time_signature.bar_index + 1,
                time_signature.numerator,
                time_signature.denominator,
            )
        })
        .collect()
}

fn generate_tempos(tempo_list: &[SongTempo]) -> Vec<VoiceVoxTempo> {
    tempo_list
        .iter()
        .map(|tempo| VoiceVoxTempo::new(tempo.position, py_round(tempo.bpm)))
        .collect()
}

impl Generator {
    fn generate_track(&self, track: &libresvip_core::model::base::SingingTrack) -> VoiceVoxTrack {
        VoiceVoxTrack {
            name: track.title.clone(),
            solo: track.solo,
            mute: track.mute,
            gain: track.volume,
            notes: track
                .note_list
                .iter()
                .map(Self::generate_note)
                .collect(),
            pitch_edit_data: self.generate_pitch(&track.edited_params.pitch),
            ..Default::default()
        }
    }

    fn generate_note(note: &Note) -> VoiceVoxNote {
        let lyric = if is_kana(&note.lyric) {
            note.lyric.clone()
        } else {
            show_warning_kind(WarningKind::FallbackApplied, format!(
                "{} {}",
                translate("Unsupported lyric: "),
                note.lyric
            ));
            libresvip_core::core::constants::DEFAULT_JAPANESE_LYRIC.to_owned()
        };
        VoiceVoxNote::new(note.start_pos, note.length, note.key_number, lyric)
    }

    fn generate_pitch(&self, pitch: &ParamCurve) -> Vec<f64> {
        let mut frequency_interval_dict = PiecewiseIntervalDict::default();
        let mut max_pitch_secs = 0.0f64;
        let secs_step = 4.0 / 375.0;
        let mut prev_secs: Option<f64> = None;
        let mut prev_freq = -1.0f64;
        for point in &pitch.points {
            if point.y == -100 {
                if let Some(prev) = prev_secs {
                    max_pitch_secs = prev;
                }
                prev_secs = None;
                prev_freq = -1.0;
            } else {
                let secs = self
                    .time_synchronizer
                    .get_actual_secs_from_ticks(point.x - self.first_bar_length);
                let freq = midi2hz(point.y as f64 / 100.0);
                if let Some(prev) = prev_secs {
                    frequency_interval_dict.set_openclosed_linear(
                        prev,
                        secs,
                        (prev, prev_freq),
                        (secs, freq),
                    );
                } else {
                    frequency_interval_dict.set_singleton(secs, freq);
                }
                prev_secs = Some(secs);
                prev_freq = freq;
            }
        }
        numeric_range(0.0, max_pitch_secs, secs_step)
            .into_iter()
            .map(|secs| frequency_interval_dict.get(secs).unwrap_or(-1.0))
            .collect()
    }
}
