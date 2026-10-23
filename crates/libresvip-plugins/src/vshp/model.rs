//! Port of `libresvip.plugins.vshp.model`.
//!
//! The Python module describes the `.vshp` file with `construct` structs. The
//! Rust port parses the same layout explicitly; every field is little-endian and
//! every `Const` magic is verified.

use libresvip_core::error::{LibreSvipError, Result};

use super::utils::until_nul;

/// A malformed `.vshp` file.
pub fn parse_error(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Parse(message.into())
}

/// `ansi2unicode` for a NUL-padded fixed-size field.
pub fn fixed_string(content: &[u8]) -> String {
    super::utils::ansi2unicode(until_nul(content))
}

/// `VocalShifterPatternType` (`CSEnum(Int32ul, WAVE=0, MIDI=1)`).
pub const PATTERN_TYPE_WAVE: u32 = 0;
/// `VocalShifterPatternType.MIDI`.
pub const PATTERN_TYPE_MIDI: u32 = 1;

/// `VocalShifterLabel`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterLabel {
    pub start_tick: u32,
    pub end_tick: u32,
    pub flag: u8,
    pub name: [u8; 64],
}

/// `VocalShifterNote`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterNote {
    pub start_tick: u32,
    pub length: u32,
    pub pitch: u16,
    pub flag: u8,
}

/// `VocalShifterTime`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterTime {
    pub original_time: f64,
    pub time: f64,
}

/// `VocalShifterControlPoint`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterControlPoint {
    pub pit_analyze: [u16; 4],
    pub wave: [u16; 4],
    pub fix_org: u16,
    pub fix_edit: u16,
    pub ori_pit: u16,
    pub pit: u16,
    pub frm: i16,
    pub bre: i16,
    pub brightness: i16,
    pub clearness: i16,
    pub ori_dyn: f64,
    /// dyn (the name is a Rust keyword).
    pub dynamic: f64,
    pub vol: f64,
    pub pan: f64,
    pub sdyn: f64,
    pub dsdyn: f64,
    pub unknown: i16,
    pub heq: i16,
}

/// `VocalShifterPatternHeader`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterPatternHeader {
    pub size: u32,
    pub sample_count: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub pattern_type: u32,
    pub points_per_second: u32,
    pub points_count: u32,
    pub time_count: u32,
    pub analyze_params: u32,
    pub analyze_prm1: u16,
    pub analyze_prm2: u16,
    pub analyze_prm3: u16,
    pub synth_mode: u16,
    pub attack_threshold: i16,
    pub fix_threshold: i16,
    pub fix_threshold_dyn: u16,
    pub synth_option: u32,
    pub fade_in_sample: u32,
    pub fade_out_sample: u32,
    pub spectrum_key_min: u16,
    pub spectrum_key_scale: u16,
    pub spectrum_points_density: u16,
    pub spectrum_points_per_group: u16,
    pub spectrum_points_count: u32,
    pub spectrum_unknown: u16,
    pub notes_count: u32,
    pub label_count: u32,
    pub eq1: [i16; 16],
    pub eq2: [i16; 16],
    pub heq: [i16; 16],
}

/// `VocalShifterSpectrumData`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterSpectrumData {
    pub size: u32,
    pub data: Vec<u16>,
}

/// `VocalShifterPatternData`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterPatternData {
    pub size: u32,
    pub header: VocalShifterPatternHeader,
    pub points: Vec<VocalShifterControlPoint>,
    pub start_time: VocalShifterTime,
    pub end_time: VocalShifterTime,
    pub spectrum: Option<VocalShifterSpectrumData>,
    pub notes: Vec<VocalShifterNote>,
    /// `None` when `label_count == 0` (`If(...)` in Python).
    pub labels: Option<Vec<VocalShifterLabel>>,
}

/// `VocalShifterPatternMetadata`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterPatternMetadata {
    pub size: u32,
    pub path_and_ext: [u8; 256],
    pub id: u32,
    pub sel_flg: u32,
    pub track_num: u32,
    pub offset_add: i32,
    pub offset: f64,
    pub meas_offset: f64,
    pub freq_a4: f64,
    pub key: u32,
    pub tune: u32,
    pub user_tune: [u8; 12],
    pub base_freq_key: u16,
    pub base_freq_scale: u16,
    pub base_freq: f64,
    pub key_type: u8,
    pub option: u32,
}

/// `VocalShifterTrackMetadata`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterTrackMetadata {
    pub size: u32,
    pub name: [u8; 64],
    pub volume: f64,
    pub pan: f64,
    pub mute: u32,
    pub solo: u32,
    pub sel_flg: u32,
    pub color: u32,
    pub invert_flg: u32,
    pub morphing_group: u32,
    pub option: u32,
}

/// `VocalShifterProjectMetadata`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterProjectMetadata {
    pub size: u32,
    pub track_count: u32,
    pub pattern_count: u32,
    pub save_option: u32,
    pub vslib_version: u32,
    pub sample_rate: u32,
    pub numerator: u32,
    pub denominator: u32,
    pub tempo: f64,
    pub master_volume: f64,
    pub is_float: u32,
    pub bit_depth: u32,
    pub channels: u32,
}

/// `VocalShifterProjectData`.
#[derive(Clone, Debug, PartialEq)]
pub struct VocalShifterProjectData {
    pub version: [u8; 8],
    pub size: u32,
    pub project_metadata: VocalShifterProjectMetadata,
    pub track_metadatas: Vec<VocalShifterTrackMetadata>,
    pub pattern_metadatas: Vec<VocalShifterPatternMetadata>,
    pub pattern_datas: Vec<VocalShifterPatternData>,
}

// ---------------------------------------------------------------------------
// Byte reader
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| parse_error("vshp: offset overflow"))?;
        if end > self.data.len() {
            return Err(parse_error("vshp: unexpected end of file"));
        }
        let slice = &self.data[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| parse_error("vshp: short fixed-size field"))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array::<2>()?))
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array::<2>()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array::<4>()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array::<4>()?))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.array::<8>()?))
    }

    fn expect(&mut self, magic: &[u8]) -> Result<()> {
        let actual = self.take(magic.len())?;
        if actual != magic {
            return Err(parse_error(format!(
                "vshp: expected magic {:?}, found {:?}",
                String::from_utf8_lossy(magic),
                String::from_utf8_lossy(actual)
            )));
        }
        Ok(())
    }

    fn skip(&mut self, length: usize) -> Result<()> {
        self.take(length).map(|_| ())
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// A conservative pre-allocation hint: counts come from the file and must not be
/// trusted enough to allocate gigabytes up front (the push loops still fail
/// cleanly on a truncated file).
fn capacity_hint(count: usize) -> usize {
    count.min(4096)
}

/// `VocalShifterProjectData.parse_file`.
pub fn parse_project(data: &[u8]) -> Result<VocalShifterProjectData> {
    let mut reader = Reader::new(data);
    reader.expect(b"VSPD")?;
    let version = reader.array::<8>()?;
    let size = reader.u32()?;
    let project_metadata = parse_project_metadata(&mut reader)?;
    let mut track_metadatas = Vec::with_capacity(capacity_hint(project_metadata.track_count as usize));
    for _ in 0..project_metadata.track_count {
        track_metadatas.push(parse_track_metadata(&mut reader)?);
    }
    let mut pattern_metadatas = Vec::with_capacity(capacity_hint(project_metadata.pattern_count as usize));
    for _ in 0..project_metadata.pattern_count {
        pattern_metadatas.push(parse_pattern_metadata(&mut reader)?);
    }
    let mut pattern_datas = Vec::with_capacity(capacity_hint(project_metadata.pattern_count as usize));
    for _ in 0..project_metadata.pattern_count {
        pattern_datas.push(parse_pattern_data(&mut reader)?);
    }
    Ok(VocalShifterProjectData {
        version,
        size,
        project_metadata,
        track_metadatas,
        pattern_metadatas,
        pattern_datas,
    })
}

fn parse_project_metadata(reader: &mut Reader<'_>) -> Result<VocalShifterProjectMetadata> {
    reader.expect(b"PRJP")?;
    let size = reader.u32()?;
    let track_count = reader.u32()?;
    let pattern_count = reader.u32()?;
    let save_option = reader.u32()?;
    let vslib_version = reader.u32()?;
    let sample_rate = reader.u32()?;
    let numerator = reader.u32()?;
    let denominator = reader.u32()?;
    reader.skip(4)?;
    let tempo = reader.f64()?;
    let master_volume = reader.f64()?;
    let is_float = reader.u32()?;
    let bit_depth = reader.u32()?;
    let channels = reader.u32()?;
    reader.skip(196)?;
    Ok(VocalShifterProjectMetadata {
        size,
        track_count,
        pattern_count,
        save_option,
        vslib_version,
        sample_rate,
        numerator,
        denominator,
        tempo,
        master_volume,
        is_float,
        bit_depth,
        channels,
    })
}

fn parse_track_metadata(reader: &mut Reader<'_>) -> Result<VocalShifterTrackMetadata> {
    reader.expect(b"TRKP")?;
    let size = reader.u32()?;
    let name = reader.array::<64>()?;
    let volume = reader.f64()?;
    let pan = reader.f64()?;
    let mute = reader.u32()?;
    let solo = reader.u32()?;
    let sel_flg = reader.u32()?;
    let color = reader.u32()?;
    let invert_flg = reader.u32()?;
    let morphing_group = reader.u32()?;
    let option = reader.u32()?;
    reader.skip(148)?;
    Ok(VocalShifterTrackMetadata {
        size,
        name,
        volume,
        pan,
        mute,
        solo,
        sel_flg,
        color,
        invert_flg,
        morphing_group,
        option,
    })
}

fn parse_pattern_metadata(reader: &mut Reader<'_>) -> Result<VocalShifterPatternMetadata> {
    reader.expect(b"ITMP")?;
    let size = reader.u32()?;
    let path_and_ext = reader.array::<256>()?;
    let id = reader.u32()?;
    let sel_flg = reader.u32()?;
    let track_num = reader.u32()?;
    let offset_add = reader.i32()?;
    let offset = reader.f64()?;
    let meas_offset = reader.f64()?;
    let freq_a4 = reader.f64()?;
    let key = reader.u32()?;
    let tune = reader.u32()?;
    let user_tune = reader.array::<12>()?;
    let base_freq_key = reader.u16()?;
    let base_freq_scale = reader.u16()?;
    let base_freq = reader.f64()?;
    let key_type = reader.u8()?;
    reader.skip(3)?;
    let option = reader.u32()?;
    reader.skip(176)?;
    Ok(VocalShifterPatternMetadata {
        size,
        path_and_ext,
        id,
        sel_flg,
        track_num,
        offset_add,
        offset,
        meas_offset,
        freq_a4,
        key,
        tune,
        user_tune,
        base_freq_key,
        base_freq_scale,
        base_freq,
        key_type,
        option,
    })
}

fn parse_pattern_header(reader: &mut Reader<'_>) -> Result<VocalShifterPatternHeader> {
    reader.expect(b"Itmp")?;
    let size = reader.u32()?;
    let sample_count = reader.u32()?;
    let sample_rate = reader.u32()?;
    let channels = reader.u32()?;
    let pattern_type = reader.u32()?;
    let points_per_second = reader.u32()?;
    let points_count = reader.u32()?;
    let time_count = reader.u32()?;
    let analyze_params = reader.u32()?;
    let analyze_prm1 = reader.u16()?;
    let analyze_prm2 = reader.u16()?;
    let analyze_prm3 = reader.u16()?;
    reader.skip(10)?;
    let synth_mode = reader.u16()?;
    let attack_threshold = reader.i16()?;
    reader.skip(2)?;
    let fix_threshold = reader.i16()?;
    reader.skip(2)?;
    let fix_threshold_dyn = reader.u16()?;
    let synth_option = reader.u32()?;
    let fade_in_sample = reader.u32()?;
    let fade_out_sample = reader.u32()?;
    reader.skip(56)?;
    let spectrum_key_min = reader.u16()?;
    let spectrum_key_scale = reader.u16()?;
    let spectrum_points_density = reader.u16()?;
    let spectrum_points_per_group = reader.u16()?;
    let spectrum_points_count = reader.u32()?;
    let spectrum_unknown = reader.u16()?;
    reader.skip(2)?;
    let notes_count = reader.u32()?;
    let label_count = reader.u32()?;
    reader.skip(8)?;
    let mut eq1 = [0i16; 16];
    for value in &mut eq1 {
        *value = reader.i16()?;
    }
    let mut eq2 = [0i16; 16];
    for value in &mut eq2 {
        *value = reader.i16()?;
    }
    let mut heq = [0i16; 16];
    for value in &mut heq {
        *value = reader.i16()?;
    }
    Ok(VocalShifterPatternHeader {
        size,
        sample_count,
        sample_rate,
        channels,
        pattern_type,
        points_per_second,
        points_count,
        time_count,
        analyze_params,
        analyze_prm1,
        analyze_prm2,
        analyze_prm3,
        synth_mode,
        attack_threshold,
        fix_threshold,
        fix_threshold_dyn,
        synth_option,
        fade_in_sample,
        fade_out_sample,
        spectrum_key_min,
        spectrum_key_scale,
        spectrum_points_density,
        spectrum_points_per_group,
        spectrum_points_count,
        spectrum_unknown,
        notes_count,
        label_count,
        eq1,
        eq2,
        heq,
    })
}

fn parse_control_point(reader: &mut Reader<'_>) -> Result<VocalShifterControlPoint> {
    reader.expect(b"Ctrp")?;
    reader.skip(4)?; // size
    let mut pit_analyze = [0u16; 4];
    for value in &mut pit_analyze {
        *value = reader.u16()?;
    }
    let mut wave = [0u16; 4];
    for value in &mut wave {
        *value = reader.u16()?;
    }
    let fix_org = reader.u16()?;
    let fix_edit = reader.u16()?;
    let ori_pit = reader.u16()?;
    let pit = reader.u16()?;
    let frm = reader.i16()?;
    let bre = reader.i16()?;
    let brightness = reader.i16()?;
    let clearness = reader.i16()?;
    let ori_dyn = reader.f64()?;
    let dyn_value = reader.f64()?;
    let vol = reader.f64()?;
    let pan = reader.f64()?;
    let sdyn = reader.f64()?;
    let dsdyn = reader.f64()?;
    let unknown = reader.i16()?;
    let heq = reader.i16()?;
    reader.skip(12)?;
    Ok(VocalShifterControlPoint {
        pit_analyze,
        wave,
        fix_org,
        fix_edit,
        ori_pit,
        pit,
        frm,
        bre,
        brightness,
        clearness,
        ori_dyn,
        dynamic: dyn_value,
        vol,
        pan,
        sdyn,
        dsdyn,
        unknown,
        heq,
    })
}

fn parse_time(reader: &mut Reader<'_>) -> Result<VocalShifterTime> {
    reader.expect(b"Time")?;
    let _size = reader.u32()?;
    let original_time = reader.f64()?;
    let time = reader.f64()?;
    Ok(VocalShifterTime {
        original_time,
        time,
    })
}

fn parse_spectrum_data(
    reader: &mut Reader<'_>,
    header: &VocalShifterPatternHeader,
) -> Result<VocalShifterSpectrumData> {
    reader.expect(b"Spct")?;
    let size = reader.u32()?;
    let count = (header.spectrum_points_count as usize)
        .checked_mul(header.spectrum_points_density as usize)
        .and_then(|value| value.checked_mul(header.spectrum_key_scale as usize))
        .ok_or_else(|| parse_error("vshp: spectrum data size overflows"))?;
    let mut data = Vec::with_capacity(capacity_hint(count));
    for _ in 0..count {
        data.push(reader.u16()?);
    }
    Ok(VocalShifterSpectrumData { size, data })
}

fn parse_notes(reader: &mut Reader<'_>, count: u32) -> Result<Vec<VocalShifterNote>> {
    reader.expect(b"Note")?;
    let _size = reader.u32()?;
    let mut notes = Vec::with_capacity(capacity_hint(count as usize));
    for _ in 0..count {
        let start_tick = reader.u32()?;
        let length = reader.u32()?;
        let pitch = reader.u16()?;
        let flag = reader.u8()?;
        reader.skip(21)?;
        notes.push(VocalShifterNote {
            start_tick,
            length,
            pitch,
            flag,
        });
    }
    Ok(notes)
}

fn parse_labels(reader: &mut Reader<'_>, count: u32) -> Result<Vec<VocalShifterLabel>> {
    reader.expect(b"Labl")?;
    let _size = reader.u32()?;
    let mut labels = Vec::with_capacity(capacity_hint(count as usize));
    for _ in 0..count {
        let start_tick = reader.u32()?;
        let end_tick = reader.u32()?;
        let flag = reader.u8()?;
        reader.skip(23)?;
        let name = reader.array::<64>()?;
        labels.push(VocalShifterLabel {
            start_tick,
            end_tick,
            flag,
            name,
        });
    }
    Ok(labels)
}

fn parse_pattern_data(reader: &mut Reader<'_>) -> Result<VocalShifterPatternData> {
    reader.expect(b"ITMD")?;
    let size = reader.u32()?;
    let header = parse_pattern_header(reader)?;
    let mut points = Vec::with_capacity(capacity_hint(header.points_count as usize));
    for _ in 0..header.points_count {
        points.push(parse_control_point(reader)?);
    }
    let start_time = parse_time(reader)?;
    let end_time = parse_time(reader)?;
    let spectrum = if header.spectrum_points_count > 0 {
        Some(parse_spectrum_data(reader, &header)?)
    } else {
        None
    };
    let notes = parse_notes(reader, header.notes_count)?;
    let labels = if header.label_count > 0 {
        Some(parse_labels(reader, header.label_count)?)
    } else {
        None
    };
    Ok(VocalShifterPatternData {
        size,
        header,
        points,
        start_time,
        end_time,
        spectrum,
        notes,
        labels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u8(out: &mut Vec<u8>, value: u8) {
        out.push(value);
    }

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i16(out: &mut Vec<u8>, value: i16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i32(out: &mut Vec<u8>, value: i32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f64(out: &mut Vec<u8>, value: f64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_zeros(out: &mut Vec<u8>, count: usize) {
        out.resize(out.len() + count, 0);
    }

    /// A `VocalShifterProjectMetadata` block (268 bytes including the magic).
    fn project_metadata(track_count: u32, pattern_count: u32, tempo: f64) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"PRJP");
        push_u32(&mut out, 260);
        push_u32(&mut out, track_count);
        push_u32(&mut out, pattern_count);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 44100);
        push_u32(&mut out, 4);
        push_u32(&mut out, 4);
        push_zeros(&mut out, 4);
        push_f64(&mut out, tempo);
        push_f64(&mut out, 1.0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 16);
        push_u32(&mut out, 2);
        push_zeros(&mut out, 196);
        out
    }

    /// A `VocalShifterTrackMetadata` block (244 bytes including the magic).
    fn track_metadata(volume: f64, pan: f64, mute: u32, solo: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"TRKP");
        push_u32(&mut out, 236);
        let mut name = [0u8; 64];
        name[..4].copy_from_slice(b"main");
        out.extend_from_slice(&name);
        push_f64(&mut out, volume);
        push_f64(&mut out, pan);
        push_u32(&mut out, mute);
        push_u32(&mut out, solo);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_zeros(&mut out, 148);
        out
    }

    /// A `VocalShifterPatternMetadata` block (520 bytes including the magic).
    fn pattern_metadata(track_num: u32, offset: f64, offset_add: i32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ITMP");
        push_u32(&mut out, 512);
        let mut path = [0u8; 256];
        path[..9].copy_from_slice(b"vocal.wav");
        out.extend_from_slice(&path);
        push_u32(&mut out, 1);
        push_u32(&mut out, 0);
        push_u32(&mut out, track_num);
        push_i32(&mut out, offset_add);
        push_f64(&mut out, offset);
        push_f64(&mut out, 0.0);
        push_f64(&mut out, 440.0);
        push_u32(&mut out, 60);
        push_u32(&mut out, 0);
        push_zeros(&mut out, 12);
        push_u16(&mut out, 69);
        push_u16(&mut out, 0);
        push_f64(&mut out, 440.0);
        push_u8(&mut out, 0);
        push_zeros(&mut out, 3);
        push_u32(&mut out, 0);
        push_zeros(&mut out, 176);
        out
    }

    /// A `VocalShifterPatternHeader` block (264 bytes including the magic).
    fn pattern_header(
        pattern_type: u32,
        points_per_second: u32,
        points: &[(u16, u16, f64, f64, i16)],
        notes_count: u32,
        label_count: u32,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Itmp");
        push_u32(&mut out, 260);
        push_u32(&mut out, 44100);
        push_u32(&mut out, 44100);
        push_u32(&mut out, 1);
        push_u32(&mut out, pattern_type);
        push_u32(&mut out, points_per_second);
        push_u32(&mut out, points.len() as u32);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_zeros(&mut out, 10);
        push_u16(&mut out, 0);
        push_i16(&mut out, -40);
        push_zeros(&mut out, 2);
        push_i16(&mut out, -40);
        push_zeros(&mut out, 2);
        push_u16(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_zeros(&mut out, 56);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u32(&mut out, 0); // spectrum_points_count
        push_u16(&mut out, 0);
        push_zeros(&mut out, 2);
        push_u32(&mut out, notes_count);
        push_u32(&mut out, label_count);
        push_zeros(&mut out, 8);
        push_zeros(&mut out, 32);
        push_zeros(&mut out, 32);
        push_zeros(&mut out, 32);
        out
    }

    fn control_point(ori_pit: u16, pit: u16, ori_dyn: f64, dyn_value: f64, bre: i16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Ctrp");
        push_u32(&mut out, 100);
        for _ in 0..4 {
            push_u16(&mut out, 0);
        }
        for _ in 0..4 {
            push_u16(&mut out, 0);
        }
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, ori_pit);
        push_u16(&mut out, pit);
        push_i16(&mut out, 0);
        push_i16(&mut out, bre);
        push_i16(&mut out, 0);
        push_i16(&mut out, 0);
        push_f64(&mut out, ori_dyn);
        push_f64(&mut out, dyn_value);
        push_f64(&mut out, 1.0);
        push_f64(&mut out, 0.0);
        push_f64(&mut out, 0.0);
        push_f64(&mut out, 0.0);
        push_i16(&mut out, 0);
        push_i16(&mut out, 0);
        push_zeros(&mut out, 12);
        out
    }

    fn time_block(value: f64) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Time");
        push_u32(&mut out, 16);
        push_f64(&mut out, value);
        push_f64(&mut out, value);
        out
    }

    fn note_block(start_tick: u32, length: u32, pitch: u16) -> Vec<u8> {
        let mut out = Vec::new();
        push_u32(&mut out, start_tick);
        push_u32(&mut out, length);
        push_u16(&mut out, pitch);
        push_u8(&mut out, 0);
        push_zeros(&mut out, 21);
        out
    }

    fn notes_block(notes: &[(u32, u32, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Note");
        push_u32(&mut out, (32 * notes.len()) as u32);
        for (start_tick, length, pitch) in notes {
            out.extend(note_block(*start_tick, *length, *pitch));
        }
        out
    }

    fn label_block(start_tick: u32, end_tick: u32, name: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        push_u32(&mut out, start_tick);
        push_u32(&mut out, end_tick);
        push_u8(&mut out, 0);
        push_zeros(&mut out, 23);
        let mut buffer = [0u8; 64];
        buffer[..name.len()].copy_from_slice(name);
        out.extend_from_slice(&buffer);
        out
    }

    fn labels_block(labels: &[(u32, u32, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Labl");
        push_u32(&mut out, (96 * labels.len()) as u32);
        for (start_tick, end_tick, name) in labels {
            out.extend(label_block(*start_tick, *end_tick, name));
        }
        out
    }

    fn pattern_data(
        pattern_type: u32,
        points_per_second: u32,
        points: &[(u16, u16, f64, f64, i16)],
        notes: &[(u32, u32, u16)],
        labels: Option<&[(u32, u32, &[u8])]>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ITMD");
        push_u32(&mut out, 0);
        out.extend(pattern_header(
            pattern_type,
            points_per_second,
            points,
            notes.len() as u32,
            labels.map_or(0, |labels| labels.len()) as u32,
        ));
        for (ori_pit, pit, ori_dyn, dyn_value, bre) in points {
            out.extend(control_point(*ori_pit, *pit, *ori_dyn, *dyn_value, *bre));
        }
        out.extend(time_block(0.0));
        out.extend(time_block(1.0));
        out.extend(notes_block(notes));
        if let Some(labels) = labels {
            out.extend(labels_block(labels));
        }
        out
    }

    fn project(
        pattern_type: u32,
        points_per_second: u32,
        points: &[(u16, u16, f64, f64, i16)],
        notes: &[(u32, u32, u16)],
        labels: Option<&[(u32, u32, &[u8])]>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"VSPD");
        out.extend_from_slice(&[0u8; 8]);
        push_u32(&mut out, 0);
        out.extend(project_metadata(1, 1, 120.0));
        out.extend(track_metadata(0.8, -0.25, 0, 1));
        out.extend(pattern_metadata(0, 0.5, 0));
        out.extend(pattern_data(
            pattern_type,
            points_per_second,
            points,
            notes,
            labels,
        ));
        out
    }

    #[test]
    fn parses_a_minimal_project() {
        let data = project(PATTERN_TYPE_MIDI, 100, &[], &[], None);
        let parsed = parse_project(&data).expect("project");
        assert_eq!(parsed.project_metadata.track_count, 1);
        assert_eq!(parsed.project_metadata.pattern_count, 1);
        assert_eq!(parsed.project_metadata.tempo, 120.0);
        assert_eq!(parsed.project_metadata.numerator, 4);
        assert_eq!(parsed.project_metadata.denominator, 4);
        assert_eq!(parsed.track_metadatas.len(), 1);
        assert_eq!(parsed.track_metadatas[0].volume, 0.8);
        assert_eq!(parsed.track_metadatas[0].pan, -0.25);
        assert_eq!(parsed.track_metadatas[0].mute, 0);
        assert_eq!(parsed.track_metadatas[0].solo, 1);
        assert_eq!(&parsed.track_metadatas[0].name[..4], b"main");
        assert_eq!(parsed.pattern_metadatas.len(), 1);
        assert_eq!(parsed.pattern_metadatas[0].offset, 0.5);
        assert_eq!(parsed.pattern_metadatas[0].track_num, 0);
        assert_eq!(&parsed.pattern_metadatas[0].path_and_ext[..9], b"vocal.wav");
        assert_eq!(parsed.pattern_datas.len(), 1);
        assert_eq!(parsed.pattern_datas[0].header.pattern_type, PATTERN_TYPE_MIDI);
        assert_eq!(parsed.pattern_datas[0].header.points_per_second, 100);
        assert!(parsed.pattern_datas[0].notes.is_empty());
        assert!(parsed.pattern_datas[0].labels.is_none());
        assert!(parsed.pattern_datas[0].spectrum.is_none());
    }

    #[test]
    fn parses_control_points_notes_and_labels() {
        let points = [
            (6000u16, 6100u16, -12.0f64, -6.0f64, 5i16),
            (0, 0, 0.0, 0.0, -5),
        ];
        let notes = [(0u32, 480u32, 6000u16), (480, 240, 6200)];
        let labels = [(100u32, 400u32, b"la\0".as_slice())];
        let data = project(
            PATTERN_TYPE_MIDI,
            100,
            &points,
            &notes,
            Some(&labels),
        );
        let parsed = parse_project(&data).expect("project");
        let pattern = &parsed.pattern_datas[0];
        assert_eq!(pattern.points.len(), 2);
        assert_eq!(pattern.points[0].ori_pit, 6000);
        assert_eq!(pattern.points[0].pit, 6100);
        assert_eq!(pattern.points[0].ori_dyn, -12.0);
        assert_eq!(pattern.points[0].dynamic, -6.0);
        assert_eq!(pattern.points[0].bre, 5);
        assert_eq!(pattern.notes.len(), 2);
        assert_eq!(pattern.notes[1].start_tick, 480);
        assert_eq!(pattern.notes[1].length, 240);
        assert_eq!(pattern.notes[1].pitch, 6200);
        let labels = pattern.labels.as_ref().expect("labels");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].start_tick, 100);
        assert_eq!(labels[0].end_tick, 400);
        assert_eq!(fixed_string(&labels[0].name), "la");
        assert_eq!(&pattern.start_time, &VocalShifterTime { original_time: 0.0, time: 0.0 });
        assert_eq!(&pattern.end_time, &VocalShifterTime { original_time: 1.0, time: 1.0 });
    }

    #[test]
    fn parses_a_project_end_to_end() {
        let data = project(
            PATTERN_TYPE_MIDI,
            100,
            &[(6000, 6000, 0.0, 0.0, 0)],
            &[(0, 480, 6000)],
            None,
        );
        let parsed = parse_project(&data).expect("project");
        let options = crate::vshp::parser::InputOptions {
            wave_to_singing: true,
            use_edited_pitch: true,
            use_edited_dynamics: true,
            import_dynamics: false,
            import_formant: false,
            import_breath: false,
        };
        let project = crate::vshp::parser::VocalShifterParser::new(options)
            .parse_project(&parsed)
            .expect("parse project");
        assert_eq!(project.song_tempo_list.len(), 1);
        assert_eq!(project.song_tempo_list[0].bpm, 120.0);
        assert_eq!(project.track_list.len(), 1);
        let track = project.track_list[0].as_singing().expect("singing");
        assert_eq!(track.title, "vocal");
        assert_eq!(track.note_list.len(), 1);
        assert_eq!(track.note_list[0].start_pos, 0);
        // int(480 * 120 / 25)
        assert_eq!(track.note_list[0].length, 2304);
        assert_eq!(track.note_list[0].key_number, 60);
        assert_eq!(
            track.edited_params.pitch.points.first(),
            Some(&libresvip_core::model::point::Point::new(-192000, -100))
        );
    }

    #[test]
    fn rejects_a_bad_magic() {
        let mut data = project(PATTERN_TYPE_MIDI, 100, &[], &[], None);
        data[0] = b'X';
        assert!(parse_project(&data).is_err());
    }

    #[test]
    fn rejects_truncated_input_without_panicking() {
        let data = project(PATTERN_TYPE_MIDI, 100, &[], &[], None);
        for length in [0usize, 1, 3, 4, 10, 100, data.len() - 1] {
            assert!(parse_project(&data[..length]).is_err(), "length {length}");
        }
    }

    #[test]
    fn rejects_an_absurd_spectrum_size() {
        // Patch `spectrum_points_count` inside the pattern header so that the
        // spectrum data would be enormous: the reader must fail cleanly instead
        // of allocating.
        let mut data = project(PATTERN_TYPE_MIDI, 100, &[], &[], None);
        // VSPD magic + version + size, project metadata, track metadata,
        // pattern metadata, ITMD magic + size, then the pattern header.
        let header_start = 4 + 8 + 4 + 264 + 264 + 520 + 4 + 4;
        // `spectrum_points_count` is at byte 144 of `VocalShifterPatternHeader`.
        let offset = header_start + 144;
        data[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        data[offset + 4..offset + 6].copy_from_slice(&u16::MAX.to_le_bytes());
        data[offset + 6..offset + 8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(parse_project(&data).is_err());
    }
}
