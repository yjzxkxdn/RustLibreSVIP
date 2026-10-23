//! Port of the Jinja template in `libresvip.plugins.ust.template`.

use libresvip_core::Result;

use super::encode;
use super::math::{float_repr, round_ndigits, truthy};
use super::model::{ListValue, UtauProject};

fn render_list_value(value: ListValue) -> String {
    match value {
        ListValue::Int(value) => value.to_string(),
        ListValue::Float(value) => float_repr(value),
        ListValue::Empty => String::new(),
    }
}

fn push_float(out: &mut String, value: f64) {
    out.push_str(&float_repr(value));
}

fn push_optional_float(out: &mut String, value: Option<f64>) {
    if let Some(value) = value {
        out.push_str(&float_repr(value));
    }
}

fn push_str_if_present(out: &mut String, label: &str, value: Option<&String>) {
    if let Some(value) = value {
        if !value.is_empty() {
            out.push('\n');
            out.push_str(label);
            out.push('=');
            out.push_str(value);
        }
    }
}

fn push_float_if_truthy(out: &mut String, label: &str, value: Option<f64>) {
    if let Some(value) = value {
        if truthy(value) {
            out.push('\n');
            out.push_str(label);
            out.push('=');
            push_float(out, value);
        }
    }
}

fn push_float_rounded_if_truthy(
    out: &mut String,
    label: &str,
    value: Option<f64>,
    digits: usize,
) {
    if let Some(value) = value {
        if truthy(value) {
            out.push('\n');
            out.push_str(label);
            out.push('=');
            push_float(out, round_ndigits(value, digits));
        }
    }
}

/// `render_ust` — returns the encoded file contents.
pub fn render_ust(project: &UtauProject, encoding: &str) -> Result<Vec<u8>> {
    let mut out = String::new();
    out.push_str("[#VERSION]\nUST Version=");
    match project.ust_version {
        Some(version) => push_float(&mut out, version),
        None => out.push_str("None"),
    }
    if let Some(charset) = project.charset.as_ref() {
        if !charset.is_empty() {
            out.push_str("\nCharset=");
            out.push_str(charset);
        }
    }
    out.push_str("\n[#SETTING]");
    // `{% if ust_project.tempo | round(2) %}`
    if let Some(tempo) = project.tempo {
        if truthy(round_ndigits(tempo, 2)) {
            out.push_str("\nTempo=");
            push_float(&mut out, tempo);
        }
    }
    if project.track_count != 0 {
        out.push_str("\nTracks=");
        out.push_str(&project.track_count.to_string());
    }
    if !project.project_name.is_empty() {
        out.push_str("\nProject=");
        out.push_str(&project.project_name);
    }
    push_str_if_present(&mut out, "VoiceDir", project.voice_dir.as_ref());
    push_str_if_present(&mut out, "OutFile", project.out_file.as_ref());
    push_str_if_present(&mut out, "CacheDir", project.cache_dir.as_ref());
    push_str_if_present(&mut out, "Tool1", project.tool1.as_ref());
    push_str_if_present(&mut out, "Tool2", project.tool2.as_ref());
    if project.pitch_mode2 {
        // `{{ ust_project.pitch_mode2 | string | capitalize }}`
        out.push_str("\nMode2=True");
    }
    if let Some(autoren) = project.autoren {
        if autoren {
            out.push_str("\nAutoren=True");
        }
    }
    push_str_if_present(&mut out, "Flags", project.flags.as_ref());

    if let Some(track) = project.track.first() {
        for note in &track.notes {
            out.push_str("\n[#");
            out.push_str(&note.note_type);
            out.push(']');
            out.push_str("\nLength=");
            out.push_str(&note.length.to_string());
            out.push_str("\nLyric=");
            out.push_str(&note.lyric);
            out.push_str("\nNoteNum=");
            out.push_str(&note.note_num.to_string());
            push_str_if_present(&mut out, "PreUtterance", note.pre_utterance.as_ref());
            push_float_rounded_if_truthy(&mut out, "VoiceOverlap", note.voice_overlap, 2);
            push_float_if_truthy(&mut out, "Intensity", note.intensity);
            push_float_if_truthy(&mut out, "Modulation", note.modulation);
            push_float_rounded_if_truthy(&mut out, "StartPoint", note.start_point, 2);
            if let Some(envelope) = note.envelope.as_ref() {
                out.push_str("\nEnvelope=");
                push_float(&mut out, envelope.p1);
                out.push(',');
                push_float(&mut out, envelope.p2);
                out.push(',');
                push_float(&mut out, envelope.p3);
                out.push(',');
                push_float(&mut out, envelope.v1);
                out.push(',');
                push_float(&mut out, envelope.v2);
                out.push(',');
                push_float(&mut out, envelope.v3);
                out.push(',');
                push_float(&mut out, envelope.v4);
                if let Some(v5) = envelope.v5.filter(|value| truthy(*value)) {
                    out.push_str(",%,");
                    push_optional_float(&mut out, envelope.p4);
                    out.push(',');
                    push_optional_float(&mut out, envelope.p5);
                    out.push(',');
                    push_float(&mut out, v5);
                } else if let Some(p5) = envelope.p5.filter(|value| truthy(*value)) {
                    out.push_str(",%,");
                    push_optional_float(&mut out, envelope.p4);
                    out.push(',');
                    push_float(&mut out, p5);
                } else if let Some(p4) = envelope.p4.filter(|value| truthy(*value)) {
                    out.push_str(",,");
                    push_float(&mut out, p4);
                }
            }
            push_float_rounded_if_truthy(&mut out, "Tempo", note.tempo, 2);
            push_float_rounded_if_truthy(&mut out, "Velocity", note.velocity, 2);
            push_str_if_present(&mut out, "Label", note.label.as_ref());
            push_str_if_present(&mut out, "Flags", note.flags.as_ref());
            if let Some(pitchbend_type) = note.pitchbend_type {
                out.push_str("\nPBType=");
                out.push_str(pitchbend_type.as_str());
            }
            push_float_if_truthy(&mut out, "PBStart", note.pitchbend_start);
            if !note.pitch_bend_points.is_empty() {
                out.push_str("\nPitchBend=");
                out.push_str(
                    &note
                        .pitch_bend_points
                        .iter()
                        .map(|value| value.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            if !note.pbs.is_empty() {
                out.push_str("\nPBS=");
                out.push_str(
                    &note
                        .pbs
                        .iter()
                        .map(|value| render_list_value(*value))
                        .collect::<Vec<_>>()
                        .join(";"),
                );
            }
            for (label, values) in [("PBW", &note.pbw), ("PBY", &note.pby)] {
                if !values.is_empty() {
                    out.push('\n');
                    out.push_str(label);
                    out.push('=');
                    out.push_str(
                        &values
                            .iter()
                            .map(|value| render_list_value(*value))
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                }
            }
            if !note.pbm.is_empty() {
                out.push_str("\nPBM=");
                out.push_str(
                    &note
                        .pbm
                        .iter()
                        .map(|value| value.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            if let Some(vbr) = note.vbr.as_ref() {
                out.push_str("\nVBR=");
                push_float(&mut out, vbr.length);
                out.push(',');
                push_float(&mut out, vbr.period);
                out.push(',');
                push_float(&mut out, vbr.depth);
                out.push(',');
                push_float(&mut out, vbr.fade_in);
                out.push(',');
                push_float(&mut out, vbr.fade_out);
                out.push(',');
                push_float(&mut out, vbr.phase_shift);
                out.push(',');
                push_float(&mut out, vbr.shift);
            }
        }
        out.push_str("\n[#TRACKEND]");
    }
    // Jinja strips the template's single trailing newline
    // (`keep_trailing_newline=False` is the default).
    encode::encode(&out, encoding)
}
