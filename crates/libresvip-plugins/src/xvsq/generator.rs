//! Generator for Cadencii project files (`cadencii_generator.py`).

use libresvip_core::core::tick_counter::{shift_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{Note, Project, SingingTrack, SongTempo, TimeSignature, Track};
use libresvip_core::options::format_float;

use super::parser::BpList;
use crate::vsq::midi::bpm2tempo;
use crate::vsqx::vocaloid::{convert_param_points_to_vocaloid_curve, py_round, VocaloidPitchHandler};
use crate::vsqx::xml::{render, Element, DECLARATION};

const PARAM_BPLISTS: [(&str, i64, &str, i64, i64); 23] = [
    ("PIT", 0, "pit", 8191, -8192),
    ("PBS", 2, "pbs", 24, 0),
    ("DYN", 64, "dyn", 127, 0),
    ("BRE", 0, "bre", 127, 0),
    ("BRI", 0, "bri", 127, 0),
    ("CLE", 0, "cle", 127, 0),
    ("reso1FreqBPList", 64, "reso1freq", 127, 0),
    ("reso2FreqBPList", 64, "reso2freq", 127, 0),
    ("reso3FreqBPList", 64, "reso3freq", 127, 0),
    ("reso4FreqBPList", 64, "reso4freq", 127, 0),
    ("reso1BWBPList", 64, "reso1bw", 127, 0),
    ("reso2BWBPList", 64, "reso2bw", 127, 0),
    ("reso3BWBPList", 64, "reso3bw", 127, 0),
    ("reso4BWBPList", 64, "reso4bw", 127, 0),
    ("reso1AmpBPList", 64, "reso1amp", 127, 0),
    ("reso2AmpBPList", 64, "reso2amp", 127, 0),
    ("reso3AmpBPList", 64, "reso3amp", 127, 0),
    ("reso4AmpBPList", 64, "reso4amp", 127, 0),
    ("harmonics", 64, "harmonics", 127, 0),
    ("fx2depth", 64, "fx2depth", 127, 0),
    ("GEN", 64, "gen", 127, 0),
    ("POR", 64, "por", 127, 0),
    ("OPE", 127, "ope", 127, 0),
];

#[derive(Clone, Debug, Default)]
pub struct OutputOptions {
    pub pretty_xml: bool,
}

/// Text element: `None`/empty values are emitted as empty elements
/// (`xsdata` writes the element without text).
fn text_element(name: impl Into<String>, value: impl Into<String>) -> Element {
    let value = value.into();
    if value.is_empty() {
        Element::new(name)
    } else {
        Element::new(name).text(value)
    }
}

fn int_element(name: impl Into<String>, value: i64) -> Element {
    Element::new(name).text(value.to_string())
}

fn bool_element(name: impl Into<String>, value: bool) -> Element {
    Element::new(name).text(if value { "true" } else { "false" })
}

fn data_element(name: &str, value: &str, default: i64, list_name: &str, maximum: i64, minimum: i64) -> Element {
    let mut element = Element::new(name);
    element.push(text_element("Data", value));
    element.push(int_element("Default", default));
    element.push(text_element("Name", list_name));
    element.push(int_element("Maximum", maximum));
    element.push(int_element("Minimum", minimum));
    element
}

fn int_pairs_to_data(points: &[(i64, i64)]) -> String {
    BpList::to_data(
        &points
            .iter()
            .map(|(x, y)| (*x as f64, *y))
            .collect::<Vec<_>>(),
    )
}

struct Generator<'a> {
    options: &'a OutputOptions,
    time_synchronizer: TimeSynchronizer,
    first_bar_length: i64,
    time_signatures: Vec<TimeSignature>,
}

/// `CadenciiGenerator.generate_project`.
pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<String> {
    let first_bar_length = project
        .time_signature_list
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
    let generator = Generator {
        options,
        time_synchronizer: TimeSynchronizer::from_project(&project.song_tempo_list),
        first_bar_length,
        time_signatures: project.time_signature_list.clone(),
    };
    generator.build(project)
}

impl Generator<'_> {
    fn build(&self, project: &Project) -> Result<String> {
        let tick_prefix = self.first_bar_length;
        let mut root = Element::new("VsqFileEx");

        let mut track_list = Element::new("Track");
        let mut vsq_tracks: Vec<Element> = vec![Element::new("VsqTrack")];
        let singing_tracks: Vec<&SingingTrack> = project
            .track_list
            .iter()
            .filter_map(Track::as_singing)
            .collect();
        let mut mixer_entries: Vec<Element> = Vec::new();
        for track in &singing_tracks {
            vsq_tracks.push(self.generate_singing_track(track, tick_prefix)?);
            mixer_entries.push(
                Element::new("VsqMixerEntry")
                    .child(int_element("Feder", 0))
                    .child(int_element("Panpot", 0))
                    .child(int_element("Mute", i64::from(track.mute)))
                    .child(int_element("Solo", i64::from(track.solo))),
            );
        }
        track_list.children(vsq_tracks);
        root.push(track_list);

        let tempo_table = Element::new("TempoTable").with_children(
            self.generate_tempos(&project.song_tempo_list, tick_prefix)?,
        );
        root.push(tempo_table);
        root.push(Element::new("TimesigTable").with_children(
            self.generate_time_signatures(&project.time_signature_list, 1),
        ));

        // Python evaluates `note.end_pos` for every track before the
        // `isinstance(track, SingingTrack)` filter, so any instrumental track
        // makes it raise `AttributeError: 'InstrumentalTrack' object has no
        // attribute 'note_list'`. The intended value is the maximum over the
        // singing notes, which is what this computes (the generated document is
        // still valid Cadencii XML and loads back identically).
        let total_clocks = singing_tracks
            .iter()
            .flat_map(|track| track.note_list.iter())
            .map(|note| note.end_pos() + tick_prefix)
            .max()
            .unwrap_or(0);
        root.push(int_element("TotalClocks", total_clocks));
        root.push(Element::new("Master").child(int_element("PreMeasure", 1)));

        let mut slave = Element::new("Slave");
        slave.children(mixer_entries);
        root.push(
            Element::new("Mixer")
                .child(int_element("MasterFeder", 0))
                .child(int_element("MasterPanpot", 0))
                .child(int_element("MasterMute", 0))
                .child(int_element("OutputMode", 0))
                .child(slave),
        );
        root.push(Element::new("AttachedCurves").child(Element::new("Curves")));

        let mut bgm_files = Element::new("BgmFiles");
        for track in project.track_list.iter().filter_map(Track::as_instrumental) {
            // `BgmFile` fields have no explicit XML names in the Python model,
            // so xsdata falls back to the (snake_case) field names.
            bgm_files.push(
                Element::new("BgmFile")
                    .child(text_element("file", track.audio_file_path.clone()))
                    .child(int_element("feder", 0))
                    .child(int_element("panpot", 0))
                    .child(int_element("mute", 0))
                    .child(bool_element("startAfterPremeasure", true))
                    .child(
                        Element::new("readOffsetSeconds").text(format_float(
                            self.time_synchronizer
                                .get_actual_secs_from_ticks(track.offset),
                        )),
                    ),
            );
        }
        root.push(bgm_files);
        root.push(Element::new("cacheDir"));
        root.push(
            Element::new("config")
                .child(int_element("SamplingRate", 44100))
                .child(int_element("WaveFileOutputChannel", 2))
                .child(bool_element("WaveFileOutputFromMasterTrack", false))
                .child(int_element("StartMarker", 0))
                .child(bool_element("StartMarkerEnabled", false))
                .child(int_element("EndMarker", 0))
                .child(bool_element("EndMarkerEnabled", false)),
        );

        // `pretty_xml` is a no-op in the Python implementation: `xsdata`'s
        // deprecated `pretty_print_indent` argument always sets
        // `config.indent`, so the output is indented for both option values.
        let _ = self.options.pretty_xml;
        Ok(render(&root, DECLARATION, true))
    }

    fn generate_tempos(&self, tempos: &[SongTempo], tick_prefix: i64) -> Result<Vec<Element>> {
        let mut entries = Vec::new();
        for tempo in skip_tempo_list(tempos, tick_prefix) {
            let time = tempo.position as f64 * 2.0 / self.first_bar_length as f64;
            if time.fract() != 0.0 {
                return Err(LibreSvipError::Validation(format!(
                    "Input should be a valid integer, got a number with a fractional part: {time}"
                )));
            }
            entries.push(
                Element::new("TempoTableEntry")
                    .child(int_element("Clock", tempo.position))
                    .child(int_element("Tempo", bpm2tempo(tempo.bpm) as i64))
                    .child(int_element("Time", time as i64)),
            );
        }
        Ok(entries)
    }

    fn generate_time_signatures(
        &self,
        time_signatures: &[TimeSignature],
        measure_prefix: i64,
    ) -> Vec<Element> {
        let mut entries = Vec::new();
        let mut ticks = 0.0f64;
        let mut prev_time_signature: Option<TimeSignature> = None;
        for time_signature in shift_beat_list(time_signatures, measure_prefix) {
            let bar_index = if let Some(previous) = &prev_time_signature {
                let bar_index = previous.bar_index;
                ticks += previous.bar_length() * (time_signature.bar_index - bar_index) as f64;
                bar_index
            } else {
                0
            };
            entries.push(
                Element::new("TimeSigTableEntry")
                    .child(int_element("Clock", ticks as i64))
                    .child(int_element("Numerator", time_signature.numerator))
                    .child(int_element("Denominator", time_signature.denominator))
                    .child(int_element("BarCount", time_signature.bar_index - bar_index)),
            );
            prev_time_signature = Some(time_signature);
        }
        entries
    }

    fn generate_singing_track(&self, track: &SingingTrack, tick_prefix: i64) -> Result<Element> {
        let mut events: Vec<Element> = Vec::new();
        events.push(vsq_event(
            0,
            0,
            VsQId::singer(track.ai_singer_name.clone()),
        ));
        for (internal_id, note) in track.note_list.iter().enumerate() {
            events.push(vsq_event(
                (internal_id + 1) as i64,
                note.start_pos + tick_prefix,
                VsQId::note(note),
            ));
        }

        let mut meta_text = Element::new("MetaText");
        meta_text.push(
            Element::new("Common")
                .child(text_element("Version", ""))
                .child(text_element("Name", track.title.clone()))
                .child(text_element("Color", "179,181,123"))
                .child(int_element("DynamicsMode", 1))
                .child(int_element("PlayMode", 1))
                .child(int_element("LastPlayMode", 1)),
        );
        meta_text.push(
            Element::new("Events").child(Element::new("Events").with_children(events)),
        );

        // Pitch and parameter curves.
        let mut data_by_name: Vec<(&'static str, Option<String>)> = vec![
            ("PIT", None),
            ("PBS", None),
            ("DYN", None),
            ("BRE", None),
            ("BRI", None),
            ("GEN", None),
        ];
        let pitch_data = self.generate_pitch(track, tick_prefix)?;
        if let Some((pit, pbs)) = pitch_data {
            for (name, data) in data_by_name.iter_mut() {
                if *name == "PIT" {
                    *data = Some(pit.clone());
                } else if *name == "PBS" {
                    *data = Some(pbs.clone());
                }
            }
        }
        for (points, param_name, list_name) in [
            (&track.edited_params.volume.points, "dynamics", "DYN"),
            (&track.edited_params.breath.points, "breathiness", "BRE"),
            (&track.edited_params.gender.points, "gender", "GEN"),
            (&track.edited_params.strength.points, "brightness", "BRI"),
        ] {
            let curve = convert_param_points_to_vocaloid_curve(
                points,
                param_name,
                -self.first_bar_length,
                param_name == "gender",
            );
            if curve.is_empty() {
                continue;
            }
            let points: Vec<(i64, i64)> = curve
                .events
                .iter()
                .map(|event| (event.pos + tick_prefix, event.value))
                .collect();
            if let Some((_, data)) = data_by_name
                .iter_mut()
                .find(|(name, _)| *name == list_name)
            {
                *data = Some(int_pairs_to_data(&points));
            }
        }

        for (element_name, default, list_name, maximum, minimum) in PARAM_BPLISTS {
            let data = data_by_name
                .iter()
                .find(|(name, _)| *name == element_name)
                .and_then(|(_, data)| data.clone())
                .unwrap_or_default();
            meta_text.push(data_element(
                element_name,
                &data,
                default,
                list_name,
                maximum,
                minimum,
            ));
        }
        Ok(Element::new("VsqTrack").child(meta_text))
    }

    fn generate_pitch(
        &self,
        track: &SingingTrack,
        tick_prefix: i64,
    ) -> Result<Option<(String, String)>> {
        let handler = VocaloidPitchHandler::new(
            self.time_synchronizer.clone(),
            &track.note_list,
            &self.time_signatures,
            self.first_bar_length,
        );
        let pitch_data = handler.from_absolute_pitch(&track.edited_params.pitch)?;
        if pitch_data.is_empty() {
            return Ok(None);
        }
        let pbs_points: Vec<(i64, i64)> = pitch_data
            .pbs
            .events
            .iter()
            .map(|event| (event.pos + tick_prefix, event.value))
            .collect();
        let pit_points: Vec<(i64, i64)> = pitch_data
            .pit
            .events
            .iter()
            .map(|event| (event.pos + tick_prefix, event.value))
            .collect();
        Ok(Some((
            int_pairs_to_data(&pit_points),
            int_pairs_to_data(&pbs_points),
        )))
    }
}

/// The subset of `VsqID` needed by the generator.
struct VsQId;

impl VsQId {
    fn singer(ids: String) -> Element {
        Element::new("ID")
            .child(text_element("type", "Singer"))
            .child(
                Element::new("IconHandle")
                    .child(text_element("Caption", ""))
                    .child(text_element("IconID", ""))
                    .child(text_element("IDS", ids))
                    .child(int_element("Index", 0))
                    .child(int_element("Length", 1))
                    .child(int_element("Original", 0))
                    .child(int_element("Program", 0))
                    .child(int_element("Language", 0)),
            )
            .child(int_element("Note", 0))
            .child(int_element("Dynamics", 64))
            .child(int_element("PMBendDepth", 8))
            .child(int_element("PMBendLength", 0))
            .child(int_element("PMbPortamentoUse", 3))
            .child(int_element("DEMdecGainRate", 50))
            .child(int_element("DEMaccent", 50))
            .child(int_element("VibratoDelay", 0))
            .child(int_element("pMeanOnsetFirstNote", 10))
            .child(int_element("vMeanNoteTransition", 12))
            .child(int_element("d4mean", 24))
            .child(int_element("pMeanEndingNote", 12))
            .child(int_element("Length", 0))
    }

    fn note(note: &Note) -> Element {
        Element::new("ID")
            .child(text_element("type", "Anote"))
            .child(int_element("Note", note.key_number))
            .child(int_element("Dynamics", 64))
            .child(int_element("PMBendDepth", 8))
            .child(int_element("PMBendLength", 0))
            .child(int_element("PMbPortamentoUse", 3))
            .child(int_element("DEMdecGainRate", 50))
            .child(int_element("DEMaccent", 50))
            .child(
                Element::new("LyricHandle")
                    .child(
                        Element::new("L0")
                            .child(text_element("Phrase", note.lyric.clone()))
                            .child(int_element("UnknownFloat", 1))
                            .child(bool_element("PhoneticSymbolProtected", false))
                            .child(int_element("ConsonantAdjustment", 0)),
                    )
                    .child(int_element("Index", 0))
                    .child(Element::new("Trailing")),
            )
            .child(int_element("VibratoDelay", 0))
            .child(int_element("pMeanOnsetFirstNote", 10))
            .child(int_element("vMeanNoteTransition", 12))
            .child(int_element("d4mean", 24))
            .child(int_element("pMeanEndingNote", 12))
            .child(int_element("Length", note.length))
    }
}

fn vsq_event(internal_id: i64, clock: i64, id: Element) -> Element {
    Element::new("VsqEvent")
        .child(int_element("InternalID", internal_id))
        .child(int_element("Clock", clock))
        .child(id)
        .child(
            Element::new("UstEvent")
                .child(int_element("Index", 0))
                .child(int_element("Note", -1))
                .child(int_element("Intensity", 100))
                .child(int_element("PBType", -1))
                .child(int_element("Tempo", -1))
                .child(int_element("PreUtterance", 0))
                .child(int_element("VoiceOverlap", 0))
                .child(int_element("Moduration", 100))
                .child(int_element("StartPoint", 0))
                .child(int_element("Length", 0)),
        )
}
