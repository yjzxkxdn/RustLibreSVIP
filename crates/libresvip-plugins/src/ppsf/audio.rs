//! Minimal audio metadata reader — the Rust counterpart of
//! `libresvip.utils.audio.audio_track_info`.
//!
//! The Python build uses `pymediainfo` when `libmediainfo` can be loaded (it can
//! in the reference environment), otherwise `soundfile`. Only the WAV (RIFF)
//! container is parsed here; other containers report `None`, which is the same
//! result the reference produces for files it cannot inspect. Callers use the
//! duration (and, for `ppsf`, the `only_wav` filter).

use libresvip_core::core::warning_types::show_warning;

/// Port of the `SoundFileInfo` dataclass.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundFileInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: String,
    pub duration: f64,
    pub bit_depth: Option<u16>,
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let slice = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let slice = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Parses a RIFF/WAVE (`PCM` or `IEEE float`) header.
fn parse_wav(data: &[u8]) -> Option<SoundFileInfo> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut format_tag = 0u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut byte_rate = 0u32;
    let mut block_align = 0u16;
    let mut bits = 0u16;
    let mut data_size = 0u32;
    let mut has_data = false;
    while offset + 8 <= data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = read_u32(data, offset + 4)? as usize;
        let body = offset + 8;
        match chunk_id {
            b"fmt " => {
                format_tag = read_u16(data, body)?;
                channels = read_u16(data, body + 2)?;
                sample_rate = read_u32(data, body + 4)?;
                byte_rate = read_u32(data, body + 8)?;
                block_align = read_u16(data, body + 12)?;
                bits = read_u16(data, body + 14)?;
            }
            b"data" => {
                data_size = chunk_size as u32;
                has_data = true;
            }
            _ => {}
        }
        // Chunks are word aligned.
        offset = body.checked_add(chunk_size)?;
        if chunk_size % 2 == 1 {
            offset = offset.checked_add(1)?;
        }
    }
    if !has_data
        || sample_rate == 0
        || !matches!(format_tag, 1 | 3 | 0xFFFE)
    {
        return None;
    }
    let frames = if block_align > 0 {
        data_size as f64 / f64::from(block_align)
    } else if byte_rate > 0 {
        data_size as f64 / f64::from(byte_rate) * f64::from(sample_rate)
    } else {
        return None;
    };
    Some(SoundFileInfo {
        sample_rate,
        channels,
        format: "WAV".to_owned(),
        duration: frames / f64::from(sample_rate),
        bit_depth: Some(bits),
    })
}

/// `audio_track_info(file_path, only_wav=False)`.
///
/// Returns `None` for missing/unrecognised files; a missing file additionally
/// raises the same warning as the Python implementation.
pub fn audio_track_info(path: &str, only_wav: bool) -> Option<SoundFileInfo> {
    let data = match libresvip_core::io::read(path) {
        Ok(data) => data,
        Err(_) => {
            show_warning(format!(
                "{}{}",
                libresvip_core::utils::translation::gettext_lazy("Invalid audio file: "),
                path
            ));
            return None;
        }
    };
    let Some(info) = parse_wav(&data) else {
        show_warning(format!(
            "{}{}",
            libresvip_core::utils::translation::gettext_lazy("Invalid audio file: "),
            path
        ));
        return None;
    };
    if only_wav && info.format != "WAV" {
        return None;
    }
    Some(info)
}

/// Convenience wrapper returning just the duration.
pub fn audio_track_duration(path: &str, only_wav: bool) -> Option<f64> {
    audio_track_info(path, only_wav).map(|info| info.duration)
}
