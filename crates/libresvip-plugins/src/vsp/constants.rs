//! Port of `experimental/vsp/constants.py`.

pub const DEFAULT_NUMERATOR: i64 = 4;
pub const DEFAULT_DENOMINATOR: i64 = 4;
pub const TICKS_IN_WHOLE_NOTE: i64 = 1920;

pub const VSP_PIT_CENTER: i64 = 64;
pub const VSP_PIT_RANGE: i64 = 64;

pub const EFFECT_TYPE_PIT: u16 = 0;
pub const MAX_EFFECT_ENTRIES: usize = 8000;
pub const EFFECT_TYPES_ORDER: [u16; 12] = [0, 6, 17, 1, 12, 13, 14, 7, 11, 8, 15, 16];

pub const DEFAULT_VELOCITY: u16 = 64;
pub const DEFAULT_TOTAL_MEASURES: i64 = 40;

pub const MASTER_TRACK_INDEX: u16 = 65526;
pub const EFFECT_TRACK_INDEX: u16 = 65525;

pub const BGM_SAMPLE_RATE: u32 = 44100;
pub const BGM_CHANNELS: u16 = 2;
pub const BGM_SAMPLE_WIDTH: u16 = 2;
