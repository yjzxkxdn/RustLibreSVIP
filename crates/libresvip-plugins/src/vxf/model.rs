//! Pitch metadata model of the VX-β plugin (`libresvip.plugins.vxf.model`).

use libresvip_core::error::{LibreSvipError, Result};

use super::pyjson::Json;

fn validation(message: impl Into<String>) -> LibreSvipError {
    LibreSvipError::Validation(message.into())
}

fn field<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value.get(key)
}

fn f64_of(value: &Json, key: &str) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| validation(format!("field `{key}` must be a number")))
}

fn i64_of(value: &Json, key: &str) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| validation(format!("field `{key}` must be an integer")))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VxPitchPoint {
    pub pitch: f64,
    pub position: i64,
    pub applicable: bool,
}

impl VxPitchPoint {
    fn to_json(self) -> Json {
        Json::object([
            ("pitch", Json::Float(self.pitch)),
            ("position", Json::Int(self.position)),
            ("applicable", Json::Bool(self.applicable)),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let pitch = field(value, "pitch")
            .map(|item| f64_of(item, "pitch"))
            .transpose()?
            .unwrap_or(0.0);
        let position = field(value, "position")
            .map(|item| i64_of(item, "position"))
            .transpose()?
            .unwrap_or(0);
        let applicable = match field(value, "applicable") {
            None | Some(Json::Null) => true,
            Some(item) => item
                .as_bool()
                .ok_or_else(|| validation("field `applicable` must be a boolean"))?,
        };
        Ok(Self {
            pitch,
            position,
            applicable,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VxTimeBasedPitchSequence {
    pub time_frame_period_seconds: f64,
    pub num_frames_overall_sequence: i64,
    pub pitch_sequence: Vec<VxPitchPoint>,
}

impl VxTimeBasedPitchSequence {
    fn to_json(&self) -> Json {
        Json::object([
            (
                "timeFramePeriodSeconds",
                Json::Float(self.time_frame_period_seconds),
            ),
            (
                "numFramesOverallSequence",
                Json::Int(self.num_frames_overall_sequence),
            ),
            (
                "pitchSequence",
                Json::Array(self.pitch_sequence.iter().copied().map(VxPitchPoint::to_json).collect()),
            ),
        ])
    }

    fn from_json(value: &Json) -> Result<Self> {
        let time_frame_period_seconds = match field(value, "timeFramePeriodSeconds") {
            Some(item) => f64_of(item, "timeFramePeriodSeconds")?,
            None => {
                return Err(validation(
                    "missing field `timeFramePeriodSeconds`",
                ))
            }
        };
        let num_frames_overall_sequence = match field(value, "numFramesOverallSequence") {
            None | Some(Json::Null) => 0,
            Some(item) => i64_of(item, "numFramesOverallSequence")?,
        };
        let mut pitch_sequence = Vec::new();
        match field(value, "pitchSequence") {
            None | Some(Json::Null) => {}
            Some(Json::Array(items)) => {
                for item in items {
                    pitch_sequence.push(VxPitchPoint::from_json(item)?);
                }
            }
            Some(_) => return Err(validation("field `pitchSequence` must be an array")),
        }
        Ok(Self {
            time_frame_period_seconds,
            num_frames_overall_sequence,
            pitch_sequence,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VxPitchData {
    pub time_based_pitch_sequence: VxTimeBasedPitchSequence,
}

impl VxPitchData {
    pub fn to_json(&self) -> Json {
        Json::object([(
            "TimeBasedPitchSequence",
            self.time_based_pitch_sequence.to_json(),
        )])
    }

    pub fn from_json(value: &Json) -> Result<Self> {
        let sequence = field(value, "TimeBasedPitchSequence")
            .ok_or_else(|| validation("missing field `TimeBasedPitchSequence`"))?;
        Ok(Self {
            time_based_pitch_sequence: VxTimeBasedPitchSequence::from_json(sequence)?,
        })
    }
}
