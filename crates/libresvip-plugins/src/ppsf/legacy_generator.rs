//! Port of `piapro_studio_legacy_generator.py` + `template_builder.py`.
//!
//! The Python reference assembles plain dicts and measures every substructure
//! with construct before stitching the chunks together; here each builder
//! returns the encoded payload so the surrounding frame can prefix the exact
//! size, which comes out byte-for-byte the same. The quirky index bookkeeping
//! of the upstream template is reproduced as-is: automation clip indices and
//! plugin `self_clip_idx` are shifted by the number of note clips, while the
//! plugin `index` fields and the track plugin references stay at their
//! template values.

use libresvip_core::core::compat::py_round;
use libresvip_core::core::constants::TICKS_IN_BEAT;
use libresvip_core::core::time_sync::TimeSynchronizer;
use libresvip_core::error::Result;
use libresvip_core::model::base::{Note, Project, SingingTrack, TimeSignature};

use crate::vpr::pitch::VocaloidPitchHandler;

use super::legacy::{write_varint, V3_PARAM_PBS_INDEX, V3_PARAM_PIT_INDEX};

const VERSION_STRING: &str = "2.0.0";
const MAX_PIT_ENTRIES: usize = 8000;
const NUM_AUDIO_OUT_TRACKS: usize = 16;
/// `_v3_block_base(i) == NUM_AUDIO_OUT_TRACKS * 3 + i * 6` — its fixed part.
const V3_BLOCK_BASE: usize = NUM_AUDIO_OUT_TRACKS * 3;

/// `DEFAULT_VIBRATO_DEPTH_DATA` — extracted verbatim from the Python bytes
/// object (127 bytes).
const DEFAULT_VIBRATO_DEPTH_DATA: &[u8] = include_bytes!("vibrato_depth.bin");

// vfxc blob payloads (the `PpsfVfxcBlob` data fields of the template
// plugins), extracted verbatim from the Python template bytes.
const MIXER_VFXC: &[u8] = include_bytes!("vfxc_mixer.bin");
const SEND_VFXC: &[u8] = include_bytes!("vfxc_send.bin");
const EQ_VFXC: &[u8] = include_bytes!("vfxc_eq.bin");
const V3_PARAM_VFXC: &[u8] = include_bytes!("vfxc_v3_param.bin");
const PROG_CHANGE_VFXC: &[u8] = include_bytes!("vfxc_prog_change.bin");
const TEMPO_VFXC: &[u8] = include_bytes!("vfxc_tempo.bin");
const KEY_VFXC: &[u8] = include_bytes!("vfxc_key.bin");
const METER_VFXC: &[u8] = include_bytes!("vfxc_meter.bin");

/// `V3_SYNTH_PLUGIN["data"]` — the template body of the VOCALOID3 synth
/// plugin, extracted verbatim from the Python bytes object.
const V3_SYNTH_DATA: &[u8] = include_bytes!("v3_synth_plugin.bin");

const FOOTER_REST: &[u8] = &[
    0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn u8v(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn u16v(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn u32v(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn pstring8(out: &mut Vec<u8>, value: &str) {
    out.push(value.len() as u8);
    out.extend_from_slice(value.as_bytes());
}

fn varint_string(out: &mut Vec<u8>, value: &str) {
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

/// `PpsfVfxcBlob`: varint(4 + 4 + len) + "vfxc" + u32 len + data.
fn vfxc_blob(out: &mut Vec<u8>, data: &[u8]) {
    write_varint(out, (4 + 4 + data.len()) as u64);
    out.extend_from_slice(b"vfxc");
    u32v(out, data.len() as u32);
    out.extend_from_slice(data);
}

/// `PpsfPlugin` frame: magic + size(len + 2) + index + body.
fn plugin_frame(magic: &[u8; 4], index: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + body.len());
    out.extend_from_slice(magic);
    u32v(&mut out, body.len() as u32 + 2);
    u16v(&mut out, index);
    out.extend_from_slice(body);
    out
}

/// `PpsfEventControlPluginBody` wrapped in a `PpsfPlugin` frame.
#[allow(clippy::too_many_arguments)]
fn event_control_plugin(
    magic: &[u8; 4],
    index: u16,
    name: &str,
    params: &[(u32, u32)],
    self_clip_idx: u16,
    vfxc: &[u8],
    trailing: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    varint_string(&mut body, name);
    body.extend_from_slice(&[0u8; 10]); // reserved
    u16v(&mut body, 1); // flag
    u8v(&mut body, params.len() as u8);
    for &(value, marker) in params {
        u32v(&mut body, value);
        u32v(&mut body, marker);
    }
    u16v(&mut body, self_clip_idx);
    vfxc_blob(&mut body, vfxc);
    body.extend_from_slice(trailing);
    plugin_frame(magic, index, &body)
}

/// `PpsfV3SynthPluginBody` wrapped in a `PpsfPlugin` frame.
fn v3_synth_plugin(index: u16) -> Vec<u8> {
    let mut body = Vec::new();
    varint_string(&mut body, "VOCALOID3");
    varint_string(&mut body, "required");
    body.extend_from_slice(V3_SYNTH_DATA);
    plugin_frame(b"V3CP", index, &body)
}

/// `build_audio_event_control_plugins`.
fn audio_event_control_plugins(num_v3: usize, index_offset: u16) -> Vec<Vec<u8>> {
    let mut plugins = Vec::new();
    let add_triplet = |plugins: &mut Vec<Vec<u8>>, base: usize| {
        let base = base as u16;
        plugins.push(event_control_plugin(
            b"EAPG",
            base,
            "Audio Mixer",
            &[(4000, 1), (0, 1)],
            base + index_offset,
            MIXER_VFXC,
            &[0x00],
        ));
        plugins.push(event_control_plugin(
            b"EAPG",
            base + 1,
            "Audio Send",
            &[(0, 1); 8],
            base + 1 + index_offset,
            SEND_VFXC,
            &[0x00],
        ));
        plugins.push(event_control_plugin(
            b"EAPG",
            base + 2,
            "Audio Eq",
            &[
                (1000, 1),
                (20, 1),
                (4000, 1),
                (10000, 1),
                (1000, 1),
                (1000, 1),
                (1000, 1),
                (500, 1),
                (500, 1),
                (500, 1),
            ],
            base + 2 + index_offset,
            EQ_VFXC,
            &[0x00],
        ));
    };
    for track_idx in 0..NUM_AUDIO_OUT_TRACKS {
        add_triplet(&mut plugins, track_idx * 3);
    }
    for i in 0..num_v3 {
        add_triplet(&mut plugins, V3_BLOCK_BASE + i * 6);
    }
    plugins
}

/// `build_vocaloid3_event_control_plugins`.
fn v3_event_control_plugins(num_v3: usize, index_offset: u16) -> Vec<Vec<u8>> {
    let mut plugins = Vec::new();
    for i in 0..num_v3 {
        let base = (V3_BLOCK_BASE + i * 6) as u16;
        plugins.push(event_control_plugin(
            b"E3PG",
            base + 3,
            "Vocaloid3 Parameter",
            &[
                (64, 1),
                (0, 1),
                (64, 1),
                (0, 1),
                (64, 1),
                (64, 1),
                (0, 1),
                (1, 1),
                (0, 1),
                (0, 1),
                (0, 1),
            ],
            base + 3 + index_offset,
            V3_PARAM_VFXC,
            &[0x00; 5],
        ));
        plugins.push(event_control_plugin(
            b"E3PG",
            base + 4,
            "Vocaloid3 Program Change",
            &[(0, 0)],
            base + 4 + index_offset,
            PROG_CHANGE_VFXC,
            &[0x00; 5],
        ));
    }
    plugins
}

/// `build_music_param_event_control_plugins`.
fn music_param_plugins(num_v3: usize, index_offset: u16) -> Vec<Vec<u8>> {
    let base = (V3_BLOCK_BASE + num_v3 * 6) as u16;
    vec![
        event_control_plugin(
            b"EMPP",
            base,
            "Tempo",
            &[(1_200_000, 1)],
            base + index_offset,
            TEMPO_VFXC,
            &[0x00],
        ),
        event_control_plugin(
            b"EMPP",
            base + 1,
            "Key",
            &[(64, 0)],
            base + 1 + index_offset,
            KEY_VFXC,
            &[0x00],
        ),
        event_control_plugin(
            b"EMPP",
            base + 2,
            "Meter",
            &[(1028, 1)],
            base + 2 + index_offset,
            METER_VFXC,
            &[0x00],
        ),
    ]
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// `PpsfVocaloid3NoteEvent` payload (without the magic/size frame).
fn v3_note_event_payload(note: &Note) -> Vec<u8> {
    let mut out = Vec::new();
    u32v(&mut out, note.start_pos as u32);
    u8v(&mut out, note.key_number as u8);
    u32v(&mut out, note.length as u32);
    u8v(&mut out, 64); // velocity
    u8v(&mut out, 0); // byte_25
    u8v(&mut out, 0); // byte_26
    u8v(&mut out, 0); // byte_27
    u8v(&mut out, 50); // byte_28
    u8v(&mut out, 50); // byte_29
    u8v(&mut out, 127); // byte_30
    varint_string(
        &mut out,
        if note.lyric.is_empty() { "la" } else { &note.lyric },
    );
    u8v(&mut out, 0); // protected
    let phoneme = note
        .pronunciation
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("4 a");
    varint_string(&mut out, phoneme);
    // PpsfVibratoHandle.
    u16v(&mut out, 0); // handle_id
    write_varint(&mut out, DEFAULT_VIBRATO_DEPTH_DATA.len() as u64);
    out.extend_from_slice(DEFAULT_VIBRATO_DEPTH_DATA);
    u8v(&mut out, 0); // flag
    write_varint(&mut out, 0); // rate_data
    u16v(&mut out, 0); // start_position
    write_varint(&mut out, 0); // amplitude_bp
    write_varint(&mut out, 0); // frequency_bp
    u16v(&mut out, 0); // trailing
    out
}

fn automation_event_payload(tick: u32, value: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    u32v(&mut out, tick);
    u32v(&mut out, value);
    u8v(&mut out, 0); // interpolation_type
    out
}

/// `PpsfEvent` frame: magic byte + u16 size + payload.
fn framed_event(magic: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + payload.len());
    u8v(&mut out, magic);
    u16v(&mut out, payload.len() as u16);
    out.extend_from_slice(payload);
    out
}

// ---------------------------------------------------------------------------
// Editor datas
// ---------------------------------------------------------------------------

/// `PpsfEditorData` frame (magic + size + payload).
fn editor_data(magic: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(magic);
    u32v(&mut out, payload.len() as u32);
    out.extend_from_slice(payload);
    out
}

/// `PpsfVSQSyllableData` payload.
fn vsqs_payload(lyric: &str, pronunciation: &str) -> Vec<u8> {
    let mut out = Vec::new();
    u32v(&mut out, 15);
    u32v(&mut out, 4294901764);
    u32v(&mut out, u32::MAX);
    for syllable in ["", lyric, pronunciation, ""] {
        pstring8(&mut out, syllable);
    }
    out
}

/// `PpsfVocaloidEVECData` payload.
fn vsqa_payload(type_name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    pstring8(&mut out, "-1");
    u32v(&mut out, u32::MAX);
    u8v(&mut out, 0);
    pstring8(&mut out, type_name);
    out
}

/// `PpsfEditorNoteData` framed as an `ENOT` editor-data entry.
fn enot_entry(note: &Note, note_index: usize) -> Vec<u8> {
    let mut payload = Vec::new();
    u32v(&mut payload, note.start_pos as u32); // pos_tick
    u32v(&mut payload, note.length as u32); // dur_tick
    u32v(&mut payload, note.length as u32); // dur_tick_copy
    u16v(&mut payload, 0xFFFE); // unknown_field
    u16v(&mut payload, 4); // phoneme_flag
    u8v(&mut payload, 0); // attack_flag
    u8v(&mut payload, 0); // release_flag
    u32v(&mut payload, 0); // vibrato_start
    u32v(&mut payload, u32::MAX); // vibrato_duration
    // sub_array_1: [VSQS].
    let lyric = if note.lyric.is_empty() { "la" } else { &note.lyric };
    let pronunciation = note
        .pronunciation
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("4 a");
    let vsqs = editor_data(b"VSQS", &vsqs_payload(lyric, pronunciation));
    write_varint(&mut payload, 1);
    payload.extend_from_slice(&vsqs);
    // int32_array — only written on "new" versions, which the generator's
    // 2.0.0 version tuple satisfies.
    write_varint(&mut payload, 3);
    u32v(&mut payload, note_index as u32);
    u32v(&mut payload, u32::MAX);
    u32v(&mut payload, u32::MAX);
    // sub_array_2: five VSQA entries.
    write_varint(&mut payload, 5);
    for type_name in ["CVV", "VSil", "CTop", "EndC", "ALT"] {
        let vsqa = editor_data(b"VSQA", &vsqa_payload(type_name));
        payload.extend_from_slice(&vsqa);
    }
    editor_data(b"ENOT", &payload)
}

/// `_build_ecls_for_notes` — one ECLS clip entry holding all notes of a track.
fn ecls_entry(notes: &[Note]) -> Option<Vec<u8>> {
    if notes.is_empty() {
        return None;
    }
    let mut inner = Vec::new();
    u16v(&mut inner, 0); // clip_type
    u16v(&mut inner, 0); // start_pos
    u16v(&mut inner, 6); // duration
    pstring8(&mut inner, "0"); // track_name
    u16v(&mut inner, 0); // color_index
    u16v(&mut inner, 0xFFFF); // x_position
    u16v(&mut inner, 0xFFFF); // y_position
    u16v(&mut inner, 0xFFFF); // clip_flags
    u32v(&mut inner, u32::MAX); // clip_id
    write_varint(&mut inner, 0); // sub_array
    let enots: Vec<Vec<u8>> = notes
        .iter()
        .enumerate()
        .map(|(note_index, note)| enot_entry(note, note_index))
        .collect();
    write_varint(&mut inner, enots.len() as u64);
    for entry in &enots {
        inner.extend_from_slice(entry);
    }
    Some(editor_data(b"ECLS", &inner))
}

/// `_build_editor_track_data` inner struct (`PpsfEditorTrackData.data`).
fn editor_track_inner(track_type: u16, display_order: u16, notes: &[Note]) -> Vec<u8> {
    let mut out = Vec::new();
    u16v(&mut out, track_type);
    u16v(&mut out, if track_type == 0 { 0 } else { display_order });
    if track_type == 0 {
        for _ in 0..10 {
            u16v(&mut out, 0);
        }
    } else {
        u16v(&mut out, 64); // total_height
        u16v(&mut out, 55); // component_height
        u16v(&mut out, 55); // track_color
        u16v(&mut out, 0xFFFF); // track_height
        u16v(&mut out, 0xFFFF); // selected_event_index
        u16v(&mut out, 0x8000); // selected_clip_index
        u16v(&mut out, 0xFFFF); // selected_sub_index
        u16v(&mut out, 3); // state_flags
        u16v(&mut out, 1); // track_width
        u16v(&mut out, 0); // horizontal_scroll
    }
    pstring8(&mut out, "0"); // track_name
    u16v(&mut out, if track_type == 0 { 0 } else { 6 }); // zoom_level
    pstring8(&mut out, if track_type == 0 { "0" } else { "0.452778" });
    u16v(&mut out, 0); // collapse_state
    for _ in 0..4 {
        u32v(&mut out, if track_type == 0 { 0 } else { u32::MAX });
    }
    pstring8(&mut out, ""); // raw_track_name
    pstring8(&mut out, ""); // raw_second_name
    write_varint(&mut out, 0); // editor_datas_1
    write_varint(&mut out, 0); // editor_datas_2
    write_varint(&mut out, 0); // editor_datas_3
    match ecls_entry(notes) {
        Some(ecls) => {
            write_varint(&mut out, 1);
            out.extend_from_slice(&ecls);
        }
        None => write_varint(&mut out, 0), // clip_datas
    }
    write_varint(&mut out, 0); // event_datas
    out
}

fn etrs_entry(track_type: u16, display_order: u16, notes: &[Note]) -> Vec<u8> {
    let inner = editor_track_inner(track_type, display_order, notes);
    editor_data(b"ETRS", &inner)
}

/// `_build_editor_datas`.
fn editor_datas(v3_count: usize, note_lists: &[&[Note]]) -> Vec<u8> {
    let mut out = Vec::new();
    u32v(&mut out, 0x0000_0101); // version_flag
    // window_rect
    out.extend_from_slice(b"RECT");
    u32v(&mut out, 16);
    u32v(&mut out, 10);
    u32v(&mut out, 10);
    u32v(&mut out, 1034);
    u32v(&mut out, 778);
    // header
    u32v(&mut out, 0x0000_0100); // version_code
    u32v(&mut out, 0xFFFF_0004); // flags
    u32v(&mut out, 0); // reserved_08
    u8v(&mut out, 9); // version_major
    out.extend_from_slice(b"0.0433824"); // version_str (9 raw bytes)
    u8v(&mut out, 1); // version_minor
    u8v(&mut out, 0x30); // version_minor_ch
    u8v(&mut out, 1); // version_revision
    u8v(&mut out, 0x31); // version_revision_ch
    u32v(&mut out, 0); // editor_hscroll
    u16v(&mut out, 574); // sampling_related
    u16v(&mut out, 0); // reserved_32
    u32v(&mut out, 0); // reserved_34
    u32v(&mut out, 12); // editor_track_count
    for _ in 0..4 {
        u32v(&mut out, u32::MAX); // sentinel_42..54
    }
    u32v(&mut out, 0); // time_position
    for _ in 0..2 {
        u32v(&mut out, u32::MAX); // sentinel_62, sentinel_66
    }
    u16v(&mut out, 0); // trailing_zero
    u8v(&mut out, 3); // trailing_flag
    // track_datas: one per v3 track, then the audio-out editor rows.
    for i in 0..v3_count {
        let notes = if i < note_lists.len() {
            note_lists[i]
        } else {
            &[]
        };
        out.extend_from_slice(&etrs_entry(3, i as u16, notes));
    }
    for i in 0..2u16 {
        out.extend_from_slice(&etrs_entry(0, i, &[]));
    }
    u8v(&mut out, 0); // footer_rect1_pad
    for _ in 0..2 {
        out.extend_from_slice(b"RECT");
        u32v(&mut out, 16);
        out.extend_from_slice(&[0u8; 16]);
    }
    out.extend_from_slice(FOOTER_REST);
    out
}

// ---------------------------------------------------------------------------
// Chunks
// ---------------------------------------------------------------------------

/// `PpsfTypedTracks`: magic + size + varint-prefixed array of track frames.
fn typed_tracks(magic: &[u8; 4], tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    write_varint(&mut payload, tracks.len() as u64);
    for track in tracks {
        payload.extend_from_slice(track);
    }
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(magic);
    u32v(&mut out, payload.len() as u32);
    out.extend_from_slice(&payload);
    out
}

/// `PpsfTrack` frame: magic + size(len + 2) + index + data.
fn track_frame(magic: &[u8; 4], index: u16, data: &[u8]) -> Vec<u8> {
    plugin_frame(magic, index, data)
}

/// `PpsfTrackBase` fields shared by every template track.
fn track_base(
    out: &mut Vec<u8>,
    v3_plugin_indices: &[u16],
    audio_plugin_indices: &[u16],
    synth_plugin_index: u16,
    source_track_indices: &[u16],
    byte_236: u8,
) {
    u16v(out, 1); // field_0
    u8v(out, 0); // flag_2
    write_varint(out, v3_plugin_indices.len() as u64);
    for value in v3_plugin_indices {
        u16v(out, *value);
    }
    write_varint(out, audio_plugin_indices.len() as u64);
    for value in audio_plugin_indices {
        u16v(out, *value);
    }
    u16v(out, 0xFFFF); // field_180
    u16v(out, synth_plugin_index);
    u32v(out, 0); // field_184
    u8v(out, 255); // byte_188
    u8v(out, 0); // byte_189
    u8v(out, 2); // byte_190
    write_varint(out, source_track_indices.len() as u64);
    for value in source_track_indices {
        u16v(out, *value);
    }
    u8v(out, byte_236);
    u8v(out, 255); // byte_237
    u8v(out, 0); // ext_byte
}

/// `build_automation_clip` (+ the generator's index shift and event fill).
/// `plugin_index` keeps the pre-shift template value on purpose.
fn automation_clip(
    index: u16,
    field_4: u16,
    event_indices: &[u32],
    pit: Option<(&[u32], &[u32])>,
    index_offset: u16,
) -> Vec<u8> {
    let mut payload = Vec::new();
    u16v(&mut payload, field_4);
    u8v(&mut payload, field_4 as u8); // field_6 == field_4 in the template
    write_varint(&mut payload, event_indices.len() as u64);
    for value in event_indices {
        u32v(&mut payload, *value);
    }
    let sub_count = field_4 as usize - 1;
    // `Array(field_4 - 1, ...)`: a fixed-size array — no count prefix.
    for sub in 0..sub_count {
        let entry: &[u32] = match pit {
            Some((pit_idx, _)) if sub == V3_PARAM_PIT_INDEX - 1 => pit_idx,
            Some((_, pbs_idx)) if sub == V3_PARAM_PBS_INDEX - 1 => pbs_idx,
            _ => &[],
        };
        write_varint(&mut payload, entry.len() as u64);
        for value in entry {
            u32v(&mut payload, *value);
        }
    }
    u8v(&mut payload, 0); // padding
    u16v(&mut payload, 1); // marker
    u16v(&mut payload, index); // plugin_index (pre-shift)
    track_frame(b"AMCL", index + index_offset, &payload)
}

/// `_bar_index_to_tick`.
fn bar_index_to_tick(time_signatures: &[TimeSignature]) -> Vec<i64> {
    let mut ticks = Vec::with_capacity(time_signatures.len());
    let mut current_tick: i64 = 0;
    for (i, ts) in time_signatures.iter().enumerate() {
        if i == 0 {
            ticks.push(0);
        } else {
            let prev_ts = &time_signatures[i - 1];
            let bar_diff = ts.bar_index - prev_ts.bar_index;
            current_tick += bar_diff * py_round(prev_ts.bar_length());
            ticks.push(current_tick);
        }
    }
    ticks
}

pub(crate) fn generate_legacy_project(project: &Project) -> Result<Vec<u8>> {
    let singing_tracks: Vec<&SingingTrack> = project
        .track_list
        .iter()
        .filter_map(|track| match track {
            libresvip_core::model::base::Track::Singing(singing) => Some(singing),
            _ => None,
        })
        .collect();
    let num_v3 = singing_tracks.len().max(1);
    let index_offset = singing_tracks.len() as u16;

    let first_bar_length = match project.time_signature_list.first() {
        Some(first) => py_round(first.bar_length()),
        None => TICKS_IN_BEAT * 4,
    };
    let synchronizer = TimeSynchronizer::from_project(&project.song_tempo_list);

    let music_base = V3_BLOCK_BASE + num_v3 * 6;
    let tempo_clip_idx = music_base as u16;
    let meter_clip_idx = (music_base + 2) as u16;

    // ---- events (notes + pit/pbs per track, then tempo, then meter) ---- //
    let mut events: Vec<Vec<u8>> = Vec::new();
    let mut per_track_pit: Vec<(Vec<u32>, Vec<u32>)> = Vec::new();
    let mut note_clip_extras: Vec<(i64, Vec<u32>)> = Vec::new();
    for track in &singing_tracks {
        let mut note_indices = Vec::with_capacity(track.note_list.len());
        for note in &track.note_list {
            note_indices.push(events.len() as u32);
            events.push(framed_event(0x08, &v3_note_event_payload(note)));
        }
        let song_end_tick = track
            .note_list
            .iter()
            .map(|note| note.start_pos + note.length)
            .max()
            .unwrap_or(0);
        note_clip_extras.push((song_end_tick, note_indices));

        let (pit_idx, pbs_idx) = if !track.edited_params.pitch.points.is_empty() {
            build_pit_events(
                track,
                &project.time_signature_list,
                first_bar_length,
                &synchronizer,
                &mut events,
            )?
        } else {
            (Vec::new(), Vec::new())
        };
        per_track_pit.push((pit_idx, pbs_idx));
    }

    // `_build_tempo_events` — a single 120 BPM tempo at tick 0 is omitted.
    let mut tempo_event_indices: Vec<u32> = Vec::new();
    let skip_tempo = project.song_tempo_list.len() == 1
        && project.song_tempo_list[0].position == 0
        && (project.song_tempo_list[0].bpm - 120.0).abs() < 0.01;
    if !skip_tempo {
        for tempo in &project.song_tempo_list {
            tempo_event_indices.push(events.len() as u32);
            events.push(framed_event(
                0x00,
                &automation_event_payload(
                    tempo.position as u32,
                    py_round(tempo.bpm * 10000.0) as u32,
                ),
            ));
        }
    }

    // `_build_meter_events` — a single 4/4 at bar 0 is omitted.
    let mut meter_event_indices: Vec<u32> = Vec::new();
    let skip_meter = project.time_signature_list.len() == 1
        && project.time_signature_list[0].bar_index == 0
        && project.time_signature_list[0].numerator == 4
        && project.time_signature_list[0].denominator == 4;
    if !skip_meter {
        let ticks = bar_index_to_tick(&project.time_signature_list);
        for (ts, tick) in project.time_signature_list.iter().zip(ticks) {
            let value = ((ts.numerator as u32) << 8) | ts.denominator as u32;
            meter_event_indices.push(events.len() as u32);
            events.push(framed_event(0x00, &automation_event_payload(tick as u32, value)));
        }
    }

    // ---- note clips ---------------------------------------------------- //
    let mut v3_note_clips: Vec<Vec<u8>> = Vec::new();
    for (i, track) in singing_tracks.iter().enumerate() {
        let (song_end_tick, note_indices) = &note_clip_extras[i];
        let mut payload = Vec::new();
        // iclip
        u16v(&mut payload, 1); // field_4
        u8v(&mut payload, 1); // field_6
        write_varint(&mut payload, note_indices.len() as u64);
        for value in note_indices {
            u32v(&mut payload, *value);
        }
        varint_string(&mut payload, &track.title); // name
        u8v(&mut payload, 1); // flag
        write_varint(&mut payload, 0); // iclip.extension: prefix of b""
        // noteclip anchors
        u32v(&mut payload, 0); // offset
        u32v(&mut payload, 0); // field_108
        u32v(&mut payload, *song_end_tick as u32);
        u32v(&mut payload, 0); // field_116
        u32v(&mut payload, *song_end_tick as u32);
        u8v(&mut payload, 0); // field_124
        u32v(&mut payload, 0); // field_128
        u32v(&mut payload, 0); // field_132
        u16v(&mut payload, 0xFFFF); // v3_field_156
        u8v(&mut payload, 1); // v3_field_158
        v3_note_clips.push(track_frame(b"V3CL", i as u16, &payload));
    }

    // ---- automation clips ---------------------------------------------- //
    let mut automation_clips: Vec<Vec<u8>> = Vec::new();
    // Audio track triplets.
    for track_idx in 0..NUM_AUDIO_OUT_TRACKS {
        let base = (track_idx * 3) as u16;
        automation_clips.push(automation_clip(base, 2, &[], None, index_offset));
        automation_clips.push(automation_clip(base + 1, 8, &[], None, index_offset));
        automation_clips.push(automation_clip(base + 2, 10, &[], None, index_offset));
    }
    // V3 track blocks; the field_4 = 11 clip carries the pit/pbs indices.
    for i in 0..num_v3 {
        let base = (V3_BLOCK_BASE + i * 6) as u16;
        automation_clips.push(automation_clip(base, 2, &[], None, index_offset));
        automation_clips.push(automation_clip(base + 1, 8, &[], None, index_offset));
        automation_clips.push(automation_clip(base + 2, 10, &[], None, index_offset));
        let (pit_idx, pbs_idx) = per_track_pit
            .get(i)
            .map(|(pit, pbs)| (pit.as_slice(), pbs.as_slice()))
            .unwrap_or((&[], &[]));
        automation_clips.push(automation_clip(
            base + 3,
            11,
            &[],
            Some((pit_idx, pbs_idx)),
            index_offset,
        ));
        automation_clips.push(automation_clip(base + 4, 1, &[], None, index_offset));
        automation_clips.push(automation_clip(base + 5, 76, &[], None, index_offset));
    }
    // Music param block: Tempo, Key, Meter.
    for offset in 0..3u16 {
        let index = music_base as u16 + offset;
        let event_indices: &[u32] = if index == tempo_clip_idx {
            &tempo_event_indices
        } else if index == meter_clip_idx {
            &meter_event_indices
        } else {
            &[]
        };
        automation_clips.push(automation_clip(index, 1, event_indices, None, index_offset));
    }

    // ---- tracks chunk --------------------------------------------------- //
    let mut v3_tracks: Vec<Vec<u8>> = Vec::new();
    for i in 0..num_v3 {
        let block = V3_BLOCK_BASE + i * 6;
        let mut data = Vec::new();
        track_base(
            &mut data,
            &[(block + 3) as u16, (block + 4) as u16],
            &[block as u16, (block + 1) as u16, (block + 2) as u16],
            (block + 5) as u16,
            &[],
            0, // byte_236
        );
        // `track_names[i] if i < len(track_names)`: an empty title stays an
        // empty name — only a MISSING list entry falls back to the default.
        let name = singing_tracks
            .get(i)
            .map(|track| track.title.clone())
            .unwrap_or_else(|| "MIKU_V4_CHINESE".to_owned());
        varint_string(&mut data, &name);
        let clip_indices: Vec<u16> = if (i as u16) < index_offset {
            vec![i as u16]
        } else {
            Vec::new()
        };
        write_varint(&mut data, clip_indices.len() as u64);
        for value in &clip_indices {
            u16v(&mut data, *value);
        }
        v3_tracks.push(track_frame(b"V3TK", (NUM_AUDIO_OUT_TRACKS + i) as u16, &data));
    }
    let mut audio_out_tracks: Vec<Vec<u8>> = Vec::new();
    for i in 0..NUM_AUDIO_OUT_TRACKS {
        let audio_plugin_base = i * 3;
        let source_indices: Vec<u16> = if i == 0 {
            (0..num_v3)
                .map(|k| (NUM_AUDIO_OUT_TRACKS + k) as u16)
                .collect()
        } else {
            Vec::new()
        };
        let mut data = Vec::new();
        track_base(
            &mut data,
            &[],
            &[
                audio_plugin_base as u16,
                (audio_plugin_base + 1) as u16,
                (audio_plugin_base + 2) as u16,
            ],
            0xFFFF,
            &source_indices,
            255, // byte_236
        );
        // device_impl
        u8v(&mut data, 3); // byte_12
        u8v(&mut data, 4); // byte_13
        u32v(&mut data, 0); // field_16
        varint_string(&mut data, "VDAW Host Output");
        u16v(&mut data, (i * 2) as u16); // field_44
        audio_out_tracks.push(track_frame(b"AODT", i as u16, &data));
    }
    let mut musical_param_tracks: Vec<Vec<u8>> = Vec::new();
    {
        let mut data = Vec::new();
        track_base(&mut data, &[], &[], 0xFFFF, &[], 255);
        u16v(&mut data, music_base as u16);
        u16v(&mut data, (music_base + 1) as u16);
        u16v(&mut data, (music_base + 2) as u16);
        musical_param_tracks.push(track_frame(
            b"MPTR",
            (NUM_AUDIO_OUT_TRACKS + num_v3) as u16,
            &data,
        ));
    }

    // ---- project field_12/16 -------------------------------------------- //
    let mut max_tick: i64 = 0;
    for track in &singing_tracks {
        for note in &track.note_list {
            let end = note.start_pos + note.length;
            if end > max_tick {
                max_tick = end;
            }
        }
    }
    let (project_field_12, project_field_16) = if max_tick > 0 {
        let padded = max_tick + first_bar_length * 2;
        (
            130560.max(padded),
            136000.max(padded + first_bar_length * 12),
        )
    } else {
        (130560, 136000)
    };

    // ---- chunk assembly -------------------------------------------------- //
    let mut chunks: Vec<Vec<u8>> = Vec::new();

    // Info: PpsfVarIntString("").
    let mut payload = Vec::new();
    write_varint(&mut payload, 0);
    let mut info = vec![b'I', b'N', b'F', b'O'];
    u32v(&mut info, payload.len() as u32);
    info.extend_from_slice(&payload);
    chunks.push(info);

    // Project
    let mut payload = Vec::new();
    u32v(&mut payload, project_field_12 as u32);
    u32v(&mut payload, project_field_16 as u32);
    u8v(&mut payload, 0); // project_field_20
    u32v(&mut payload, 0); // project_field_24
    u32v(&mut payload, 126720); // project_field_28
    u32v(&mut payload, 1); // project_field_32
    u32v(&mut payload, 0); // project_field_36
    u32v(&mut payload, 0); // project_field_40
    write_varint(&mut payload, 0); // vocaloid2_singers
    write_varint(&mut payload, 1); // vocaloid3_singers
    varint_string(&mut payload, "MIKU_V4_CHINESE");
    varint_string(&mut payload, "MIKU_V4_Chinese");
    u8v(&mut payload, 0); // unknown1
    u8v(&mut payload, 4); // unknown2
    varint_string(&mut payload, "BNGE7CP7EMTRSNC3"); // voice_setting.name
    for _ in 0..5 {
        u8v(&mut payload, 0); // voice_setting.parameters
    }
    u8v(&mut payload, 0); // has_extra_voice_setting
    let extension_arrays: [Vec<u16>; 9] = [
        Vec::new(),
        (0..16).collect(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        (0..num_v3)
            .map(|k| (NUM_AUDIO_OUT_TRACKS + k) as u16)
            .collect(),
        Vec::new(),
    ];
    for array in &extension_arrays {
        write_varint(&mut payload, array.len() as u64);
        for value in array {
            u16v(&mut payload, *value);
        }
    }
    u16v(&mut payload, (NUM_AUDIO_OUT_TRACKS + num_v3) as u16); // extension_flags
    u8v(&mut payload, 0); // extension_data
    let mut chunk = vec![b'P', b'R', b'O', b'J'];
    u32v(&mut chunk, payload.len() as u32);
    chunk.extend_from_slice(&payload);
    chunks.push(chunk);

    // Transport
    let mut chunk = vec![b'T', b'R', b'N', b'S'];
    u32v(&mut chunk, 6);
    u32v(&mut chunk, 0);
    u16v(&mut chunk, 0);
    chunks.push(chunk);

    // Config
    let mut chunk = vec![b'C', b'O', b'N', b'F'];
    u32v(&mut chunk, 10);
    u32v(&mut chunk, 0);
    u32v(&mut chunk, 0);
    u16v(&mut chunk, 0);
    chunks.push(chunk);

    // Devices (5 empty subcomponent arrays; their size is a Byte on new
    // versions).
    let mut chunk = vec![b'D', b'V', b'C', b'S'];
    u32v(&mut chunk, 13);
    for _ in 0..5 {
        u8v(&mut chunk, 0);
    }
    u16v(&mut chunk, 0);
    u16v(&mut chunk, 0);
    u32v(&mut chunk, 32);
    chunks.push(chunk);

    // Tracks
    let mut payload = Vec::new();
    payload.extend_from_slice(&typed_tracks(b"AETS", &[]));
    payload.extend_from_slice(&typed_tracks(b"METS", &[]));
    payload.extend_from_slice(&typed_tracks(b"VETS", &[]));
    payload.extend_from_slice(&typed_tracks(b"V3TS", &v3_tracks));
    payload.extend_from_slice(&typed_tracks(b"MIDS", &[]));
    payload.extend_from_slice(&typed_tracks(b"MODS", &[]));
    payload.extend_from_slice(&typed_tracks(b"AIDS", &[]));
    payload.extend_from_slice(&typed_tracks(b"AODS", &audio_out_tracks));
    payload.extend_from_slice(&typed_tracks(b"ASTS", &[]));
    payload.extend_from_slice(&typed_tracks(b"MPTS", &musical_param_tracks));
    payload.push(0); // extension_data
    let mut chunk = vec![b'T', b'R', b'K', b'S'];
    u32v(&mut chunk, payload.len() as u32);
    chunk.extend_from_slice(&payload);
    chunks.push(chunk);

    // Clips
    let mut payload = Vec::new();
    for _ in 0..5 {
        write_varint(&mut payload, 0);
    }
    write_varint(&mut payload, v3_note_clips.len() as u64);
    for clip in &v3_note_clips {
        payload.extend_from_slice(clip);
    }
    write_varint(&mut payload, automation_clips.len() as u64);
    for clip in &automation_clips {
        payload.extend_from_slice(clip);
    }
    payload.push(0); // extension_data
    let mut chunk = vec![b'C', b'L', b'P', b'S'];
    u32v(&mut chunk, payload.len() as u32);
    chunk.extend_from_slice(&payload);
    chunks.push(chunk);

    // Events — chunk size is payload + 1, padded with one zero byte.
    let mut payload = Vec::new();
    write_varint(&mut payload, events.len() as u64);
    for event in &events {
        payload.extend_from_slice(event);
    }
    let mut chunk = vec![b'E', b'V', b'T', b'S'];
    u32v(&mut chunk, payload.len() as u32 + 1);
    chunk.extend_from_slice(&payload);
    chunk.push(0);
    chunks.push(chunk);

    // Plugins
    let plugin_lists: Vec<Vec<Vec<u8>>> = vec![
        audio_event_control_plugins(num_v3, index_offset),
        Vec::new(), // midi
        Vec::new(), // vsq
        v3_event_control_plugins(num_v3, index_offset),
        Vec::new(), // synth
        (0..num_v3)
            .map(|i| v3_synth_plugin((V3_BLOCK_BASE + i * 6 + 5) as u16))
            .collect(),
        music_param_plugins(num_v3, index_offset),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let mut payload = Vec::new();
    for list in &plugin_lists {
        write_varint(&mut payload, list.len() as u64);
        for plugin in list {
            payload.extend_from_slice(plugin);
        }
    }
    payload.push(0); // extension_data
    let mut chunk = vec![b'P', b'L', b'G', b'S'];
    u32v(&mut chunk, payload.len() as u32);
    chunk.extend_from_slice(&payload);
    chunks.push(chunk);

    // EditorDatas
    let note_lists: Vec<&[Note]> = singing_tracks
        .iter()
        .map(|track| track.note_list.as_slice())
        .collect();
    let payload = editor_datas(num_v3, &note_lists);
    let mut chunk = vec![b'E', b'D', b'T', b'S'];
    u32v(&mut chunk, payload.len() as u32);
    chunk.extend_from_slice(&payload);
    chunks.push(chunk);

    // ---- file ------------------------------------------------------------ //
    let mut inner = Vec::new();
    u16v(&mut inner, VERSION_STRING.len() as u16);
    inner.extend_from_slice(VERSION_STRING.as_bytes());
    for chunk in &chunks {
        inner.extend_from_slice(chunk);
    }
    let mut out = Vec::with_capacity(8 + inner.len());
    out.extend_from_slice(b"PPSF");
    u32v(&mut out, inner.len() as u32);
    out.extend_from_slice(&inner);
    Ok(out)
}

/// `_build_pit_events`.
fn build_pit_events(
    track: &SingingTrack,
    time_signatures: &[TimeSignature],
    first_bar_length: i64,
    synchronizer: &TimeSynchronizer,
    events: &mut Vec<Vec<u8>>,
) -> Result<(Vec<u32>, Vec<u32>)> {
    let handler = VocaloidPitchHandler::new(
        synchronizer.clone(),
        &track.note_list,
        time_signatures,
        first_bar_length,
    );
    let result = handler.from_absolute_pitch(&track.edited_params.pitch)?;
    if result.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut pending: Vec<Vec<u8>> = Vec::new();
    for pit_event in &result.pit.events {
        let mut value = pit_event.value;
        if value < 0 {
            value += 0x1_0000_0000;
        }
        pending.push(framed_event(
            0x00,
            &automation_event_payload(pit_event.pos as u32, value as u32),
        ));
    }
    if pending.len() > MAX_PIT_ENTRIES {
        let step = pending.len() / MAX_PIT_ENTRIES + 1;
        // The value u32 of a framed automation event lives at bytes 7..11.
        let value_of = |event: &[u8]| -> [u8; 4] {
            [event[7], event[8], event[9], event[10]]
        };
        let mut kept = vec![pending[0].clone()];
        for index in 1..pending.len() - 1 {
            let keep = index % step == 0
                || value_of(&pending[index]) != value_of(&pending[index - 1]);
            if keep {
                kept.push(pending[index].clone());
            }
        }
        kept.push(pending[pending.len() - 1].clone());
        pending = kept;
    }
    let mut pit_indices = Vec::with_capacity(pending.len());
    for event in pending {
        pit_indices.push(events.len() as u32);
        events.push(event);
    }

    let mut pbs_indices = Vec::new();
    for pbs_event in &result.pbs.events {
        pbs_indices.push(events.len() as u32);
        events.push(framed_event(
            0x00,
            &automation_event_payload(pbs_event.pos as u32, pbs_event.value as u32),
        ));
    }
    Ok((pit_indices, pbs_indices))
}
