//! Generator for Vocaloid 2 sequence files (`vsq_generator.py`).

use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{
    ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature, Track,
};
use libresvip_core::model::reset_time_axis::limit_bars;

use super::constants::DEFAULT_PHONEME;
use super::midi::{bpm2tempo, build_midi, BuildEvent, BuildMessage};
use super::parser::{encode_text, encoding_by_name};
use crate::vsqx::phonemes::{is_legato_char, romaji2xsampa, to_romaji};
use crate::vsqx::vocaloid::{
    convert_param_points_to_vocaloid_curve, py_round, VocaloidPitchHandler,
};

/// `OutputOptions` of the vsq plugin.
#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub ticks_per_beat: i64,
    pub lyric_encoding: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            ticks_per_beat: TICKS_IN_BEAT,
            lyric_encoding: "SHIFT_JIS".to_owned(),
        }
    }
}

struct VsqGenerator<'a> {
    options: &'a OutputOptions,
    encoding: &'static encoding_rs::Encoding,
    synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
}

impl VsqGenerator<'_> {
    fn tick_rate(&self) -> f64 {
        TICKS_IN_BEAT as f64 / self.options.ticks_per_beat as f64
    }

    fn generate_project(&self, project: &Project) -> Result<Vec<u8>> {
        let project = limit_bars(project, 4096)?;
        let mut master_track: Vec<BuildMessage> = vec![BuildMessage {
            time: 0,
            event: BuildEvent::TrackName(encode_text("Master Track", self.encoding)),
        }];
        self.generate_tempos(&mut master_track, &project.song_tempo_list);
        self.generate_time_signatures(&mut master_track, &project.time_signature_list);
        let last_time = master_track.last().map(|message| message.time).unwrap_or(0);
        master_track.push(BuildMessage {
            time: last_time,
            event: BuildEvent::EndOfTrack,
        });
        master_track.sort_by_key(|message| message.time);
        let mut tracks: Vec<Vec<BuildMessage>> = vec![master_track];
        tracks.extend(self.generate_tracks(&project.track_list)?);
        build_midi(1, self.options.ticks_per_beat, &tracks)
    }

    fn generate_tempos(&self, master_track: &mut Vec<BuildMessage>, tempos: &[SongTempo]) {
        for tempo in tempos {
            master_track.push(BuildMessage {
                time: py_round(tempo.position as f64 / self.tick_rate()),
                event: BuildEvent::SetTempo(bpm2tempo(tempo.bpm)),
            });
        }
    }

    fn generate_time_signatures(
        &self,
        master_track: &mut Vec<BuildMessage>,
        time_signatures: &[TimeSignature],
    ) {
        let mut ticks = 0i64;
        let mut prev_time_signature: Option<&TimeSignature> = None;
        for time_signature in time_signatures {
            if let Some(previous) = prev_time_signature {
                ticks += py_round(
                    previous.bar_length_with(self.options.ticks_per_beat)
                        * (time_signature.bar_index - previous.bar_index) as f64,
                );
            }
            master_track.push(BuildMessage {
                time: ticks,
                event: BuildEvent::TimeSignature {
                    numerator: time_signature.numerator as u8,
                    denominator: time_signature.denominator,
                    clocks_per_click: 24,
                    notated_32nd_notes_per_quarter: 8,
                },
            });
            prev_time_signature = Some(time_signature);
        }
    }

    fn generate_tracks(&self, tracks: &[Track]) -> Result<Vec<Vec<BuildMessage>>> {
        let singing_tracks: Vec<&SingingTrack> = tracks
            .iter()
            .filter_map(Track::as_singing)
            .filter(|track| !track.note_list.is_empty())
            .collect();
        let mut result = Vec::with_capacity(singing_tracks.len());
        for (index, track) in singing_tracks.iter().enumerate() {
            result.push(self.generate_track(track, index, singing_tracks.len())?);
        }
        Ok(result)
    }

    fn generate_track(
        &self,
        track: &SingingTrack,
        track_index: usize,
        tracks_count: usize,
    ) -> Result<Vec<BuildMessage>> {
        let track_text = self.generate_track_text(track, track_index, tracks_count, 1)?;
        let characters: Vec<char> = track_text.chars().collect();
        let mut messages: Vec<BuildMessage> = Vec::new();
        let mut index = 0usize;
        while index < characters.len() {
            let event_id = messages.len();
            let header = format!("DM:{event_id:04}:");
            let available_length = 0x7F - header.chars().count();
            let end = (index + available_length).min(characters.len());
            let chunk: String = characters[index..end].iter().collect();
            index = end;
            messages.push(BuildMessage {
                time: 0,
                event: BuildEvent::Text(encode_text(
                    &format!("{header}{chunk}"),
                    self.encoding,
                )),
            });
        }
        messages.push(BuildMessage {
            time: 0,
            event: BuildEvent::EndOfTrack,
        });
        Ok(messages)
    }

    fn generate_track_text(
        &self,
        track: &SingingTrack,
        track_index: usize,
        tracks_count: usize,
        measure_prefix: i64,
    ) -> Result<String> {
        let mut notes_lines: Vec<String> = Vec::new();
        let mut lyrics_lines: Vec<String> = Vec::new();
        let tick_prefix = self.first_bar_length;
        let tick_lists: Vec<i64> = track
            .note_list
            .iter()
            .map(|note| note.start_pos + tick_prefix)
            .collect();
        for (index, note) in track.note_list.iter().enumerate() {
            let number = format!("{:04}", index + 1);
            notes_lines.extend([
                format!("[ID#{number}]"),
                "Type=Anote".to_owned(),
                format!("Length={}", note.length),
                format!("Note#={}", note.key_number),
                "Dynamics=64".to_owned(),
                "PMBendDepth=0".to_owned(),
                "PMBendLength=0".to_owned(),
                "PMbPortamentoUse=0".to_owned(),
                "DEMdecGainRate=0".to_owned(),
                "DEMaccent=0".to_owned(),
                format!("LyricHandle=h#{number}"),
            ]);
            let lyric = &note.lyric;
            let xsampa = if is_legato_char(lyric) {
                "-".to_owned()
            } else {
                romaji2xsampa(&to_romaji(lyric))
                    .unwrap_or(DEFAULT_PHONEME)
                    .to_owned()
            };
            lyrics_lines.extend([
                format!("[h#{number}]"),
                format!("L0=\"{lyric}\",\"{xsampa}\",0.000000,64,0,0"),
            ]);
        }
        let mut result: Vec<String> = vec![
            "[Common]".to_owned(),
            "Version=DSB301".to_owned(),
            format!("Name={}", track.title),
            "Color=181,162,123".to_owned(),
            "DynamicsMode=1".to_owned(),
            "PlayMode=1".to_owned(),
        ];
        if track_index == 0 {
            result.extend([
                "[Master]".to_owned(),
                format!("PreMeasure={measure_prefix}"),
                "[Mixer]".to_owned(),
                "MasterFeder=0".to_owned(),
                "MasterPanpot=0".to_owned(),
                "MasterMute=0".to_owned(),
                "OutputMode=0".to_owned(),
                format!("Tracks={tracks_count}"),
            ]);
            for index in 0..tracks_count {
                result.extend([
                    format!("Feder{index}=0"),
                    format!("Panpot{index}=0"),
                    format!("Mute{index}=0"),
                    format!("Solo{index}=0"),
                ]);
            }
        }
        result.extend(["[EventList]".to_owned(), "0=ID#0000".to_owned()]);
        for (index, tick) in tick_lists.iter().enumerate() {
            result.push(format!("{tick}=ID#{:04}", index + 1));
        }
        let last_end = track
            .note_list
            .last()
            .map(|note| note.end_pos())
            .unwrap_or(0);
        result.extend([
            format!("{}=EOS", last_end + tick_prefix),
            "[ID#0000]".to_owned(),
            "Type=Singer".to_owned(),
            "IconHandle=h#0000".to_owned(),
        ]);
        result.extend(notes_lines);
        result.extend([
            "[h#0000]".to_owned(),
            "IconID=$07010000".to_owned(),
            format!("IDS={}", track.ai_singer_name),
            "Original=0".to_owned(),
            "Caption=".to_owned(),
            "Length=1".to_owned(),
            "Language=0".to_owned(),
            "Program=0".to_owned(),
        ]);
        result.extend(lyrics_lines);
        result.extend(self.generate_pitch_text(
            &track.edited_params.pitch,
            tick_prefix,
            &track.note_list,
        )?);
        result.extend(self.generate_params_text(&track.edited_params, tick_prefix));
        Ok(result.join("\n"))
    }

    fn generate_pitch_text(
        &self,
        pitch: &ParamCurve,
        tick_prefix: i64,
        note_list: &[libresvip_core::model::base::Note],
    ) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let handler = VocaloidPitchHandler::new(
            self.synchronizer.clone(),
            note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        let pitch_data = handler.from_absolute_pitch(pitch)?;
        if !pitch_data.is_empty() {
            if !pitch_data.pit.events.is_empty() {
                result.push("[PitchBendBPList]".to_owned());
                result.extend(
                    pitch_data
                        .pit
                        .events
                        .iter()
                        .map(|event| format!("{}={}", event.pos + tick_prefix, event.value)),
                );
            }
            if !pitch_data.pbs.events.is_empty() {
                result.push("[PitchBendSensBPList]".to_owned());
                result.extend(
                    pitch_data
                        .pbs
                        .events
                        .iter()
                        .map(|event| format!("{}={}", event.pos + tick_prefix, event.value)),
                );
            }
        }
        Ok(result)
    }

    fn generate_params_text(&self, params: &Params, tick_prefix: i64) -> Vec<String> {
        let mut result = Vec::new();
        for (points, param_name, section) in [
            (&params.volume.points, "dynamics", "[DynamicsBPList]"),
            (&params.breath.points, "breathiness", "[BreathinessBPList]"),
            (&params.gender.points, "gender", "[GenderFactorBPList]"),
            (&params.strength.points, "brightness", "[BrightnessBPList]"),
        ] {
            let curve = convert_param_points_to_vocaloid_curve(
                points,
                param_name,
                -self.first_bar_length,
                param_name == "gender",
            );
            if !curve.is_empty() {
                result.push(section.to_owned());
                result.extend(
                    curve
                        .events
                        .iter()
                        .map(|event| format!("{}={}", event.pos + tick_prefix, event.value)),
                );
            }
        }
        result
    }
}

/// `VsqGenerator.generate_project`.
pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<Vec<u8>> {
    let encoding = encoding_by_name(&options.lyric_encoding)?;
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let first_bar_length = py_round(
        project
            .time_signature_list
            .first()
            .map(|signature| signature.bar_length_with(options.ticks_per_beat))
            .unwrap_or(0.0),
    );
    let time_signatures = project.time_signature_list.clone();
    let generator = VsqGenerator {
        options,
        encoding,
        synchronizer,
        first_bar_length,
        time_signatures,
    };
    generator.generate_project(project)
}
