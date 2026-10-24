//! Port of `libresvip.utils.audio`.
//!
//! The Python implementation is backed by `pymediainfo` / `soundfile`. The Rust
//! port reads the formats it needs directly (RIFF/WAVE) and reports anything
//! else the same way Python does: a localized `Invalid audio file: …` warning
//! and `None`.

use std::path::{Path, PathBuf};

use crate::core::warning_types::{show_warning_kind, WarningKind};
use crate::utils::translation::gettext_lazy;

#[derive(Clone, Debug, PartialEq)]
pub struct SoundFileInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: String,
    pub duration: f64,
    pub bit_depth: Option<u32>,
}

/// `audio_path_validator`: resolve a project-relative audio path.
pub fn audio_path_validator(path: &str, project_path: Option<&Path>) -> String {
    let audio_path = Path::new(path);
    if audio_path.is_absolute() {
        return path.to_owned();
    }
    match project_path.and_then(Path::parent) {
        Some(parent) => {
            let resolved = parent.join(audio_path);
            resolved.display().to_string()
        }
        None => path.to_owned(),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

/// Parses a RIFF/WAVE header (enough for duration, sample rate and bit depth).
fn parse_wav(bytes: &[u8]) -> Option<SoundFileInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut position = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut byte_rate = 0u32;
    let mut bit_depth = None;
    let mut data_size = 0u32;
    while position + 8 <= bytes.len() {
        let chunk_id = &bytes[position..position + 4];
        let chunk_size = read_u32(bytes, position + 4)? as usize;
        let body = position + 8;
        match chunk_id {
            b"fmt " => {
                let audio_format = read_u16(bytes, body)?;
                channels = read_u16(bytes, body + 2)?;
                sample_rate = read_u32(bytes, body + 4)?;
                byte_rate = read_u32(bytes, body + 8)?;
                let bits = read_u16(bytes, body + 14)?;
                bit_depth = if bits == 0 { None } else { Some(bits as u32) };
                // Only PCM-like formats are reported as "WAV" by libsndfile.
                if audio_format > 3 {
                    return None;
                }
            }
            b"data" => {
                data_size = chunk_size as u32;
            }
            _ => {}
        }
        position = body + chunk_size + (chunk_size & 1);
        if data_size > 0 && sample_rate > 0 {
            break;
        }
    }
    if sample_rate == 0 || byte_rate == 0 {
        return None;
    }
    Some(SoundFileInfo {
        sample_rate,
        channels,
        format: "WAV".to_owned(),
        duration: data_size as f64 / byte_rate as f64,
        bit_depth,
    })
}

/// `audio_track_info`: metadata for an audio file, or `None` (with a warning).
pub fn audio_track_info(file_path: impl AsRef<Path>, only_wav: bool) -> Option<SoundFileInfo> {
    let path = file_path.as_ref();
    let warning = || {
        show_warning_kind(WarningKind::FieldDropped, format!(
            "{}{}",
            gettext_lazy("Invalid audio file: "),
            path.display()
        ));
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            warning();
            return None;
        }
    };
    let Some(info) = parse_wav(&bytes) else {
        warning();
        return None;
    };
    if only_wav && info.format != "WAV" {
        return None;
    }
    Some(info)
}

/// Convenience: resolves a project-relative path before inspecting it.
pub fn audio_track_info_relative(file_path: &str, project_path: Option<&Path>) -> Option<SoundFileInfo> {
    let resolved = audio_path_validator(file_path, project_path);
    audio_track_info(PathBuf::from(resolved), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_wav() {
        // 44-byte canonical header, 8000 Hz mono 16-bit, 0.5 s of silence.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + 8000u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        let info = parse_wav(&bytes).expect("wav");
        assert_eq!(info.sample_rate, 8000);
        assert_eq!(info.channels, 1);
        assert!((info.duration - 0.5).abs() < 1e-9);
        assert_eq!(info.bit_depth, Some(16));
    }
}
