//! Generator for VSQX/VSQ3 files (`vsq3_generator.py` / `vsq4_generator.py`).
//!
//! Both Python generators are identical except for the field/element names of
//! the xsdata models; the shared implementation below switches names through
//! [`VsqxVersion`].

use libresvip_core::core::tick_counter::{shift_beat_list, skip_tempo_list};
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::core::warning_types::show_warning;
use libresvip_core::error::{LibreSvipError, Result};
use libresvip_core::model::base::{
    InstrumentalTrack, Note, ParamCurve, Params, Project, SingingTrack, SongTempo, TimeSignature,
    Track, TICKS_IN_BEAT,
};

use libresvip_core::utils::audio::audio_track_info;
use super::parser::{VsqxVersion, DEFAULT_CHINESE_PHONEME, DEFAULT_JAPANESE_PHONEME};
use super::phonemes::{
    hangul2xsampa, is_legato_char, pinyin2xsampa, romaji2xsampa, to_romaji,
};
use super::vocaloid::{
    convert_param_points_to_vocaloid_curve, py_round, ControllerCurve, ControllerEvent,
    VocaloidPitchHandler,
};
use super::xml::{render, Content, Element, DECLARATION_STANDALONE};
use super::BPM_RATE;

const VSQ3_NS: &str = "http://www.yamaha.co.jp/vocaloid/schema/vsq3/";
const VSQ4_NS: &str = "http://www.yamaha.co.jp/vocaloid/schema/vsq4/";
const STYLE_PLUGIN_ID: &str = "ACA9C502-A04B-42b5-B2EB-5CEA36D16FCE";
const STYLE_PLUGIN_NAME: &str = "VOCALOID2 Compatible Style";
const STYLE_PLUGIN_VERSION: &str = "3.0.0.1";

/// Elements whose text is written as CDATA (`VocaloidXMLWriter.set_data`).
const CDATA_TAGS: &[&str] = &[
    "y",
    "p",
    "id",
    "id2",
    "auxID",
    "compID",
    "stylePluginID",
    "vstPluginID",
    "name",
    "partName",
    "seqName",
    "stylePluginName",
    "trackName",
    "vVoiceName",
    "vstPluginName",
    "filePath",
    "content",
    "phnmStr",
    "vender",
    "comment",
    "version",
];

/// `OutputOptions` of the vsqx plugin.
#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub vsqx_version: VsqxVersion,
    pub pretty_xml: bool,
    pub default_lang_id: i64,
    pub default_comp_id: String,
    pub default_singer_name: String,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            vsqx_version: VsqxVersion::Vsq4,
            pretty_xml: true,
            default_lang_id: 4,
            default_comp_id: "BETDB8W6KWZPYEB9".to_owned(),
            default_singer_name: "Tianyi_CHN".to_owned(),
        }
    }
}

/// `VocaloidStyleTypes().model_dump(by_alias=True)` --order and values are the
/// defaults of the Python model.
fn style_params() -> Vec<(&'static str, Option<i64>)> {
    vec![
        ("accent", Some(50)),
        ("bendDep", Some(0)),
        ("bendLen", Some(0)),
        ("decay", Some(50)),
        ("fallPort", Some(0)),
        ("opening", Some(127)),
        ("risePort", Some(0)),
        ("vibLen", Some(0)),
        ("vibType", Some(0)),
        ("vibDep", None),
        ("vibRate", None),
    ]
}

fn text_element(name: impl Into<String>, value: impl Into<String>) -> Element {
    let name = name.into();
    let value = value.into();
    if CDATA_TAGS.contains(&name.as_str()) {
        // `set_cdata` always attaches a CDATA node, even for empty strings.
        Element::new(name).cdata(value)
    } else if value.is_empty() {
        // `set_data("")` leaves the element without text → `<name/>`.
        Element::new(name)
    } else {
        Element::new(name).text(value)
    }
}

fn int_element(name: impl Into<String>, value: i64) -> Element {
    Element::new(name).text(value.to_string())
}

fn enum_element(name: impl Into<String>, value: i64) -> Element {
    Element::new(name).text(value.to_string())
}

/// Serialise `project` into a VSQX document.
pub fn generate_project(project: &Project, options: &OutputOptions) -> Result<String> {
    let version = options.vsqx_version;
    let first_bar_length = project
        .time_signature_list
        .first()
        .map(|signature| py_round(signature.bar_length()))
        .ok_or_else(|| LibreSvipError::msg("time_signature_list must not be empty"))?;
    let tick_prefix = project
        .time_signature_list
        .first()
        .map(|signature| signature.bar_length() as i64)
        .unwrap_or(0);
    let time_synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);
    let time_signatures = &project.time_signature_list;
    let style = style_params();

    let ns = match version {
        VsqxVersion::Vsq3 => VSQ3_NS,
        VsqxVersion::Vsq4 => VSQ4_NS,
    };
    let n = |v3: &'static str, v4: &'static str| match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    };

    let mut root = Element::new(n("vsq3", "vsq4"))
        .attr("xmlns", ns)
        .attr("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance")
        .attr(
            "xsi:schemaLocation",
            format!("{ns} vsq{}.xsd", if version == VsqxVersion::Vsq3 { 3 } else { 4 }),
        );

    root.push(text_element("vender", "Yamaha Corporation"));
    root.push(text_element(
        "version",
        if version == VsqxVersion::Vsq3 {
            "3.0.0.0"
        } else {
            "4.0.0.3"
        },
    ));

    // vVoiceTable
    let voice_param = Element::new(n("vVoiceParam", "vPrm"))
        .child(int_element("bre", 0))
        .child(int_element("bri", 0))
        .child(int_element("cle", 0))
        .child(int_element("gen", 0))
        .child(int_element("ope", 0));
    // `id2`/`vPrm2` (VSQ4 only) default to None and are omitted.
    let voice = Element::new("vVoice")
        // `VVoice.v_bs` defaults to `SIMPLIFIED_CHINESE`; the option only
        // affects the part `singer` and the note phonemes.
        .child(int_element(n("vBS", "bs"), 4))
        .child(int_element(n("vPC", "pc"), 0))
        .child(text_element(
            n("compID", "id"),
            options.default_comp_id.clone(),
        ))
        .child(text_element(
            n("vVoiceName", "name"),
            options.default_singer_name.clone(),
        ))
        .child(voice_param);
    let voice_table = Element::new("vVoiceTable").child(voice);

    // mixer
    let master_unit = Element::new("masterUnit")
        .child(int_element(n("outDev", "oDev"), 0))
        .child(int_element(n("retLevel", "rLvl"), 0))
        .child(int_element("vol", 0));

    let mut vs_unit_list: Vec<Element> = Vec::new();
    let mut singing_tracks: Vec<&SingingTrack> = project
        .track_list
        .iter()
        .filter_map(Track::as_singing)
        .collect();
    let mut default_track;
    if singing_tracks.is_empty() {
        default_track = SingingTrack::new("Track");
        default_track.title = "Track".to_owned();
        singing_tracks.push(&default_track);
    } else if !matches!(options.default_lang_id, 0 | 2 | 4) {
        show_warning(format!(
            "Phonemes of all notes were set to \"la\". Please use \"Lyrics\" -> \"Convert Phonemes\" in the menu of VOCALOID{} to reset them.",
            if version == VsqxVersion::Vsq3 { 3 } else { 4 }
        ));
    }

    let mut vs_track_elements: Vec<Element> = Vec::new();
    for (track_index, track) in singing_tracks.iter().enumerate() {
        let mut vs_track = Element::new("vsTrack")
            .child(int_element(n("vsTrackNo", "tNo"), track_index as i64))
            .child(text_element(n("trackName", "name"), track.title.clone()))
            .child(text_element("comment", "Track"));
        if !track.note_list.is_empty() {
            let mut part = Element::new(n("musicalPart", "vsPart"))
                .child(int_element(n("posTick", "t"), tick_prefix))
                .child(int_element("playTime", track.note_list.last().map(Note::end_pos).unwrap_or(0)))
                .child(text_element(n("partName", "name"), "New Part"))
                .child(text_element("comment", "New Musical Part"));
            let style_plugin = Element::new(n("stylePlugin", "sPlug"))
                .child(text_element(n("stylePluginID", "id"), STYLE_PLUGIN_ID))
                .child(text_element(
                    n("stylePluginName", "name"),
                    STYLE_PLUGIN_NAME,
                ))
                .child(text_element("version", STYLE_PLUGIN_VERSION));
            part.push(style_plugin);
            let attr_name = n("attr", "v");
            let part_style = Element::new(n("partStyle", "pStyle")).with_children(
                style
                    .iter()
                    .filter_map(|(name, value)| value.map(|value| (name, value)))
                    .map(|(name, value)| Element::new(attr_name).attr("id", *name).text(value.to_string())),
            );
            part.push(part_style);
            if version == VsqxVersion::Vsq3 {
                // `phraseStyle` is an empty list.
            }
            part.push(
                Element::new("singer")
                    .child(int_element(n("posTick", "t"), 0))
                    .child(int_element(n("vBS", "bs"), options.default_lang_id))
                    .child(int_element(n("vPC", "pc"), 0)),
            );

            let mut m_ctrls: Vec<Element> = Vec::new();
            if let Some(pitch) = generate_pitch(
                &track.edited_params.pitch,
                &track.note_list,
                &time_synchronizer,
                time_signatures,
                first_bar_length,
                version,
            )? {
                m_ctrls.extend(pitch);
            }
            m_ctrls.extend(generate_params(
                &track.edited_params,
                first_bar_length,
                version,
            ));
            part.children(m_ctrls);
            part.children(generate_notes(
                &track.note_list,
                options,
                version,
                &style,
            )?);
            vs_track.push(part);
        }
        vs_track_elements.push(vs_track);
        vs_unit_list.push(
            Element::new("vsUnit")
                .child(int_element(n("vsTrackNo", "tNo"), track_index as i64))
                .child(int_element(n("inGain", "iGin"), 0))
                .child(int_element(n("sendLevel", "sLvl"), -898))
                .child(int_element(n("sendEnable", "sEnable"), 0))
                .child(int_element(n("mute", "m"), i64::from(track.mute)))
                .child(int_element(n("solo", "s"), i64::from(track.solo)))
                .child(int_element("pan", 64))
                .child(int_element("vol", 0)),
        );
    }

    let mut mono_unit = Element::new(n("seUnit", "monoUnit"))
        .child(int_element(n("inGain", "iGin"), 0))
        .child(int_element(n("sendLevel", "sLvl"), -898))
        .child(int_element(n("sendEnable", "sEnable"), 0))
        .child(int_element(n("mute", "m"), 0))
        .child(int_element(n("solo", "s"), 0))
        .child(int_element("pan", 64))
        .child(int_element("vol", 0));
    let mut stereo_unit = Element::new(n("karaokeUnit", "stUnit"))
        .child(int_element(n("inGain", "iGin"), 0))
        .child(int_element(n("mute", "m"), 0))
        .child(int_element(n("solo", "s"), 0))
        .child(int_element("vol", 0));
    let mut mono_track = Element::new(n("seTrack", "monoTrack"));
    let mut stereo_track = Element::new(n("karaokeTrack", "stTrack"));

    if let Some(track) = project
        .track_list
        .iter()
        .find_map(Track::as_instrumental)
    {
        generate_instrumental_track(
            track,
            &time_synchronizer,
            tick_prefix,
            version,
            &mut mono_unit,
            &mut stereo_unit,
            &mut mono_track,
            &mut stereo_track,
        );
    }

    let mut mixer = Element::new("mixer");
    mixer.push(master_unit);
    mixer.children(vs_unit_list);
    mixer.push(mono_unit);
    mixer.push(stereo_unit);
    root.push(voice_table);
    root.push(mixer);

    // masterTrack
    let mut master_track = Element::new("masterTrack")
        .child(text_element("seqName", "Untitled0"))
        .child(text_element("comment", "New VSQ File"))
        .child(int_element("resolution", TICKS_IN_BEAT))
        .child(int_element("preMeasure", 1));
    master_track.children(generate_time_signatures(
        time_signatures,
        1,
        version,
    ));
    master_track.children(generate_tempos(&project.song_tempo_list, tick_prefix, version));
    root.push(master_track);
    root.children(vs_track_elements);
    root.push(mono_track);
    root.push(stereo_track);

    let declaration = DECLARATION_STANDALONE;
    // `pretty_xml` is a no-op in the Python implementation: xsdata's deprecated
    // `pretty_print_indent` argument always sets `config.indent`, so lxml
    // indents the tree for both option values. Reproduce that behaviour.
    let _ = options.pretty_xml;
    Ok(render(&root, declaration, true))
}

#[allow(clippy::too_many_arguments)]
fn generate_instrumental_track(
    track: &InstrumentalTrack,
    synchronizer: &TimeSynchronizer,
    tick_prefix: i64,
    version: VsqxVersion,
    mono_unit: &mut Element,
    stereo_unit: &mut Element,
    mono_track: &mut Element,
    stereo_track: &mut Element,
) {
    let Some(info) = audio_track_info(&track.audio_file_path, true) else {
        return;
    };
    if info.sample_rate != 44100 || info.bit_depth != Some(16) {
        return;
    }
    let n = |v3: &'static str, v4: &'static str| match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    };
    let play_time = py_round(
        synchronizer.get_actual_ticks_from_secs_offset(track.offset, info.duration),
    ) - track.offset;
    let mut wav_part = Element::new("wavPart")
        .child(int_element(n("posTick", "t"), tick_prefix + track.offset))
        .child(int_element("playTime", play_time))
        .child(text_element(n("partName", "name"), track.title.clone()));
    if version == VsqxVersion::Vsq4 {
        wav_part.push(text_element("comment", "New Wav Part"));
    }
    wav_part.push(enum_element(n("sampleRate", "fs"), info.sample_rate as i64));
    wav_part.push(enum_element(
        n("sampleReso", "rs"),
        info.bit_depth.unwrap_or(0) as i64,
    ));
    // VSQ3 reuses the field name for `channels`; VSQ4 names it `nCh`.
    wav_part.push(int_element(n("channels", "nCh"), info.channels as i64));
    wav_part.push(text_element("filePath", track.audio_file_path.clone()));
    if info.channels == 1 {
        *mono_unit = Element::new(n("seUnit", "monoUnit"))
            .child(int_element(n("inGain", "iGin"), 0))
            .child(int_element(n("sendLevel", "sLvl"), -898))
            .child(int_element(n("sendEnable", "sEnable"), 0))
            .child(int_element(n("mute", "m"), i64::from(track.mute)))
            .child(int_element(n("solo", "s"), i64::from(track.solo)))
            .child(int_element("pan", 64))
            .child(int_element("vol", 0));
        mono_track.push(wav_part);
    } else if info.channels == 2 {
        *stereo_unit = Element::new(n("karaokeUnit", "stUnit"))
            .child(int_element(n("inGain", "iGin"), 0))
            .child(int_element(n("mute", "m"), i64::from(track.mute)))
            .child(int_element(n("solo", "s"), i64::from(track.solo)))
            .child(int_element("vol", 0));
        stereo_track.push(wav_part);
    }
}

fn generate_tempos(
    tempos: &[SongTempo],
    tick_prefix: i64,
    version: VsqxVersion,
) -> Vec<Element> {
    let n = |v3: &'static str, v4: &'static str| match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    };
    skip_tempo_list(tempos, tick_prefix)
        .into_iter()
        .map(|tempo| {
            Element::new("tempo")
                .child(int_element(n("posTick", "t"), tempo.position))
                .child(int_element(n("bpm", "v"), (tempo.bpm * BPM_RATE) as i64))
        })
        .collect()
}

fn generate_time_signatures(
    time_signatures: &[TimeSignature],
    measure_prefix: i64,
    version: VsqxVersion,
) -> Vec<Element> {
    let n = |v3: &'static str, v4: &'static str| match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    };
    shift_beat_list(time_signatures, measure_prefix)
        .into_iter()
        .map(|signature| {
            Element::new("timeSig")
                .child(int_element(n("posMes", "m"), signature.bar_index))
                .child(int_element(n("nume", "nu"), signature.numerator))
                .child(int_element(n("denomi", "de"), signature.denominator))
        })
        .collect()
}

fn generate_notes(
    notes: &[Note],
    options: &OutputOptions,
    version: VsqxVersion,
    style: &[(&'static str, Option<i64>)],
) -> Result<Vec<Element>> {
    let n = |v3: &'static str, v4: &'static str| match version {
        VsqxVersion::Vsq3 => v3,
        VsqxVersion::Vsq4 => v4,
    };
    let mut note_list = Vec::with_capacity(notes.len());
    for note in notes {
        let mut lyric = note.lyric.clone();
        let phonemes: String;
        if is_legato_char(&note.lyric) {
            phonemes = "-".to_owned();
        } else if options.default_lang_id == 4 {
            lyric = libresvip_core::core::lyric_phoneme::chinese::get_pinyin_series_with(
                &[note.lyric.as_str()],
                libresvip_core::core::lyric_phoneme::chinese::PinyinSeriesOptions {
                    ignore_hyphens: true,
                    reverse_letters: true,
                    filter_non_chinese: false,
                },
            )?
            .join(" ");
            phonemes = pinyin2xsampa(&lyric)
                .unwrap_or(DEFAULT_CHINESE_PHONEME)
                .to_owned();
        } else if options.default_lang_id == 0 {
            phonemes = romaji2xsampa(&to_romaji(&lyric))
                .unwrap_or(DEFAULT_JAPANESE_PHONEME)
                .to_owned();
        } else if options.default_lang_id == 2 {
            phonemes = hangul2xsampa(&lyric);
        } else {
            phonemes = DEFAULT_CHINESE_PHONEME.to_owned();
        }
        let attr_name = n("attr", "v");
        let note_style = Element::new(n("noteStyle", "nStyle")).with_children(
            style
                .iter()
                .filter_map(|(name, value)| value.map(|value| (name, value)))
                .map(|(name, value)| {
                    Element::new(attr_name)
                        .attr("id", *name)
                        .text(value.to_string())
                }),
        );
        let mut element = Element::new("note")
            .child(int_element(n("posTick", "t"), note.start_pos))
            .child(int_element(n("durTick", "dur"), note.length))
            .child(int_element(n("noteNum", "n"), note.key_number))
            .child(int_element(n("velocity", "v"), 64))
            .child(text_element(n("lyric", "y"), lyric));
        let mut phoneme_element = Element::new(n("phnms", "p"));
        if version == VsqxVersion::Vsq4 {
            phoneme_element = phoneme_element.cdata(phonemes);
        } else {
            phoneme_element = phoneme_element.text(phonemes);
        }
        element.push(phoneme_element);
        element.push(note_style);
        note_list.push(element);
    }
    Ok(note_list)
}

fn generate_pitch(
    pitch: &ParamCurve,
    notes: &[Note],
    synchronizer: &TimeSynchronizer,
    time_signatures: &[TimeSignature],
    first_bar_length: i64,
    version: VsqxVersion,
) -> Result<Option<Vec<Element>>> {
    let handler = VocaloidPitchHandler::new(
        synchronizer.clone(),
        notes,
        time_signatures,
        first_bar_length,
    );
    let pitch_data = handler.from_absolute_pitch(pitch)?;
    if pitch_data.is_empty() {
        return Ok(None);
    }
    let pbs_id = version.param_id("PBS");
    let pit_id = version.param_id("PIT");
    let mut music_controls: Vec<Element> = pitch_data
        .pbs
        .events
        .iter()
        .map(|event: &ControllerEvent| m_ctrl_element(event.pos, pbs_id, event.value, version))
        .collect();
    music_controls.extend(
        pitch_data
            .pit
            .events
            .iter()
            .map(|event| m_ctrl_element(event.pos, pit_id, event.value, version)),
    );
    music_controls.sort_by_key(m_ctrl_pos);
    Ok(Some(music_controls))
}

fn m_ctrl_pos(element: &Element) -> i64 {
    // The first child holds the position (`t` for VSQ4, `posTick` for VSQ3).
    match &element.content {
        Content::Children(children) => children
            .first()
            .and_then(|child| match &child.content {
                Content::Text(text) => text.parse::<i64>().ok(),
                _ => None,
            })
            .unwrap_or(0),
        _ => 0,
    }
}

fn m_ctrl_element(pos: i64, param_id: &str, value: i64, version: VsqxVersion) -> Element {
    let (container, pos_name, attr_name) = match version {
        VsqxVersion::Vsq4 => ("cc", "t", "v"),
        VsqxVersion::Vsq3 => ("mCtrl", "posTick", "attr"),
    };
    Element::new(container)
        .child(int_element(pos_name, pos))
        .child(
            Element::new(attr_name)
                .attr("id", param_id)
                .text(value.to_string()),
        )
}

fn generate_params(params: &Params, first_bar_length: i64, version: VsqxVersion) -> Vec<Element> {
    let mut music_controls: Vec<Element> = Vec::new();
    for (points, param_name) in [
        (&params.volume.points, "dynamics"),
        (&params.breath.points, "breathiness"),
        (&params.gender.points, "gender"),
        (&params.strength.points, "brightness"),
    ] {
        let curve = convert_param_points_to_vocaloid_curve(
            points,
            param_name,
            -first_bar_length,
            param_name == "gender",
        );
        music_controls.extend(create_mctrl_list(&curve, version));
    }
    music_controls.sort_by_key(m_ctrl_pos);
    music_controls
}

fn create_mctrl_list(curve: &ControllerCurve, version: VsqxVersion) -> Vec<Element> {
    if curve.is_empty() {
        return Vec::new();
    }
    // The Python implementation reads a non-existent `vsqx_name` attribute here
    // (an AttributeError for any non-empty parameter curve); the intended
    // parameter id is the `vsq_name` mapped through the version's name table.
    let param_id = super::vocaloid::get_param_def(&curve.name)
        .and_then(|definition| definition.vsq_name)
        .map(|name| version.param_id(name))
        .unwrap_or(curve.name.as_str());
    curve
        .events
        .iter()
        .map(|event| m_ctrl_element(event.pos, param_id, event.value, version))
        .collect()
}
