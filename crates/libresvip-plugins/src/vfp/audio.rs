//! Minimal `audio_track_info` replacement for the VOX Factory generator.
//!
//! Python delegates to `pymediainfo`/`soundfile`, which are not available in
//! Rust. RIFF/WAVE files (the common case for embedded VOX Factory resources)
//! are parsed directly; any other format returns `None`, so the audio track is
//! skipped exactly like Python does for unreadable files.

use std::path::Path;

use libresvip_core::core::warning_types::show_warning;
use libresvip_core::utils::translation::gettext_lazy;

pub struct AudioTrackInfo {
    pub sample_rate: i64,
    pub channels: i64,
    pub duration: f64,
}

pub fn audio_track_info(path: &Path) -> Option<AudioTrackInfo> {
    let bytes = match libresvip_core::io::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            show_warning(format!(
                "{}{}",
                gettext_lazy("Invalid audio file: "),
                path.display()
            ));
            return None;
        }
    };
    parse_wav(&bytes)
}

fn parse_wav(bytes: &[u8]) -> Option<AudioTrackInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut sample_rate = 0i64;
    let mut channels = 0i64;
    let mut byte_rate = 0i64;
    let mut data_size = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let body_start = offset + 8;
        match id {
            b"fmt " => {
                if body_start + 16 > bytes.len() {
                    return None;
                }
                channels = u16::from_le_bytes(
                    bytes[body_start + 2..body_start + 4].try_into().ok()?,
                ) as i64;
                sample_rate = u32::from_le_bytes(
                    bytes[body_start + 4..body_start + 8].try_into().ok()?,
                ) as i64;
                byte_rate = u32::from_le_bytes(
                    bytes[body_start + 8..body_start + 12].try_into().ok()?,
                ) as i64;
            }
            b"data" => {
                data_size = Some(size.min(bytes.len().saturating_sub(body_start)));
            }
            _ => {}
        }
        offset = body_start.checked_add(size + (size & 1))?;
        if data_size.is_some() && sample_rate > 0 {
            break;
        }
    }
    let data_size = data_size? as i64;
    if sample_rate <= 0 || channels <= 0 || byte_rate <= 0 {
        return None;
    }
    Some(AudioTrackInfo {
        sample_rate,
        channels,
        duration: data_size as f64 / byte_rate as f64,
    })
}
