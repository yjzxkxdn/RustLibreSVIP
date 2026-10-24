//! Port of `libresvip.plugins.vpr.vocaloid_controllers` together with the
//! helpers it uses from `libresvip.model.vocaloid.controller_models`,
//! `controller_registry` and `simple_controller_handler`.

use libresvip_core::model::point::Point;

use super::model::{PointValue, VocaloidControllers, VocaloidPoint, VocaloidVoicePart};

pub const PITCH_MIN_VALUE: i64 = -8192;
pub const PITCH_MAX_VALUE: i64 = 8191;
pub const DEFAULT_PITCH_BEND_SENSITIVITY: i64 = 2;
pub const MAX_PITCH_BEND_SENSITIVITY: i64 = 24;

/// Python's `round()` — ties to even.

pub use libresvip_core::core::compat::py_round;

/// `ControllerEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerEvent {
    pub pos: i64,
    pub value: i64,
}

/// `ControllerCurve`.
#[derive(Clone, Debug)]
pub struct ControllerCurve {
    pub name: String,
    pub events: Vec<ControllerEvent>,
    pub default_value: i64,
    pub min_value: i64,
    pub max_value: i64,
}

impl ControllerCurve {
    /// `ControllerCurve(name=..., events=[])` with the dataclass defaults.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: Vec::new(),
            default_value: 0,
            min_value: -127,
            max_value: 127,
        }
    }

    /// `ControllerCurve(...)`: `__post_init__` sorts the events by position.
    pub fn with_events(
        name: impl Into<String>,
        events: Vec<ControllerEvent>,
        default_value: i64,
        min_value: i64,
        max_value: i64,
    ) -> Self {
        let mut events = events;
        if !events.is_empty() {
            events.sort_by_key(|event| event.pos);
        }
        Self {
            name: name.into(),
            events,
            default_value,
            min_value,
            max_value,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn get_value_at(&self, pos: i64) -> i64 {
        let mut current_value = self.default_value;
        for event in &self.events {
            if event.pos <= pos {
                current_value = event.value;
            } else {
                break;
            }
        }
        current_value
    }
}

/// `VocaloidParameterDef`.
#[derive(Clone, Copy, Debug)]
pub struct VocaloidParameterDef {
    pub name: &'static str,
    pub display_name: &'static str,
    pub vpr_name: Option<&'static str>,
    pub vsq_name: Option<&'static str>,
    pub min_value: i64,
    pub max_value: i64,
    pub default_value: i64,
    pub is_complex: bool,
}

pub static VOCALOID_PARAMETERS: [VocaloidParameterDef; 6] = [
    VocaloidParameterDef {
        name: "pitch_bend",
        display_name: "Pitch Bend",
        vpr_name: Some("pitchBend"),
        vsq_name: Some("PIT"),
        min_value: PITCH_MIN_VALUE,
        max_value: PITCH_MAX_VALUE,
        default_value: 0,
        is_complex: true,
    },
    VocaloidParameterDef {
        name: "pitch_bend_sens",
        display_name: "Pitch Bend Sensitivity",
        vpr_name: Some("pitchBendSens"),
        vsq_name: Some("PBS"),
        min_value: 1,
        max_value: MAX_PITCH_BEND_SENSITIVITY,
        default_value: DEFAULT_PITCH_BEND_SENSITIVITY,
        is_complex: true,
    },
    VocaloidParameterDef {
        name: "dynamics",
        display_name: "Dynamics",
        vpr_name: Some("dynamics"),
        vsq_name: Some("DYN"),
        min_value: 0,
        max_value: 127,
        default_value: 64,
        is_complex: false,
    },
    VocaloidParameterDef {
        name: "breathiness",
        display_name: "Breathiness",
        vpr_name: Some("breathiness"),
        vsq_name: Some("BRE"),
        min_value: 0,
        max_value: 127,
        default_value: 0,
        is_complex: false,
    },
    VocaloidParameterDef {
        name: "brightness",
        display_name: "Brightness",
        vpr_name: Some("brightness"),
        vsq_name: Some("BRI"),
        min_value: 0,
        max_value: 127,
        default_value: 0,
        is_complex: false,
    },
    VocaloidParameterDef {
        name: "gender",
        display_name: "Gender Factor",
        vpr_name: Some("character"),
        vsq_name: Some("GEN"),
        min_value: 0,
        max_value: 127,
        default_value: 64,
        is_complex: false,
    },
];

pub fn get_param_def(name: &str) -> Option<&'static VocaloidParameterDef> {
    VOCALOID_PARAMETERS
        .iter()
        .find(|definition| definition.name == name)
}

/// `get_param_by_format("vpr", param_id)`.
pub fn get_param_by_format_vpr(param_id: &str) -> Option<&'static VocaloidParameterDef> {
    VOCALOID_PARAMETERS
        .iter()
        .find(|definition| definition.vpr_name == Some(param_id))
}

// ---------------------------------------------------------------------------
// SimpleControllerHandler
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SimpleControllerHandler {
    pub simplify: bool,
    pub simplify_threshold: f64,
    pub internal_min_value: i64,
    pub internal_max_value: i64,
    pub internal_default_value: i64,
}

impl Default for SimpleControllerHandler {
    fn default() -> Self {
        Self {
            simplify: true,
            simplify_threshold: 1.0,
            internal_min_value: -1000,
            internal_max_value: 1000,
            internal_default_value: 0,
        }
    }
}

impl SimpleControllerHandler {
    fn map_external_to_internal(
        &self,
        value: i64,
        default_value: i64,
        min_value: i64,
        max_value: i64,
        reverse: bool,
    ) -> i64 {
        let mut mapped_value = if value >= default_value {
            let denominator = max_value - default_value;
            if denominator == 0 {
                self.internal_default_value
            } else {
                py_round(
                    (value - default_value) as f64 / denominator as f64
                        * (self.internal_max_value - self.internal_default_value) as f64
                        + self.internal_default_value as f64,
                )
            }
        } else {
            let denominator = default_value - min_value;
            if denominator == 0 {
                self.internal_default_value
            } else {
                py_round(
                    (value - default_value) as f64 / denominator as f64
                        * (self.internal_default_value - self.internal_min_value) as f64
                        + self.internal_default_value as f64,
                )
            }
        };
        if reverse {
            mapped_value = -mapped_value;
        }
        mapped_value.clamp(self.internal_min_value, self.internal_max_value)
    }

    fn map_internal_to_external(
        &self,
        value: i64,
        default_value: i64,
        min_value: i64,
        max_value: i64,
        reverse: bool,
    ) -> i64 {
        let mut clamped_value = value.clamp(self.internal_min_value, self.internal_max_value);
        if reverse {
            clamped_value = -clamped_value;
        }
        let mapped_value = if clamped_value >= self.internal_default_value {
            let denominator = self.internal_max_value - self.internal_default_value;
            if denominator == 0 {
                default_value
            } else {
                py_round(
                    (clamped_value - self.internal_default_value) as f64 / denominator as f64
                        * (max_value - default_value) as f64
                        + default_value as f64,
                )
            }
        } else {
            let denominator = self.internal_default_value - self.internal_min_value;
            if denominator == 0 {
                default_value
            } else {
                py_round(
                    (clamped_value - self.internal_default_value) as f64 / denominator as f64
                        * (default_value - min_value) as f64
                        + default_value as f64,
                )
            }
        };
        mapped_value.clamp(min_value, max_value)
    }

    fn append_step_points(
        points: &mut Vec<Point>,
        current_pos: i64,
        previous_value: i64,
        current_value: i64,
    ) {
        let boundary_pos = current_pos - 1;
        if current_value != previous_value && boundary_pos >= 0 {
            let boundary_point = Point::new(boundary_pos, previous_value);
            if points.last() != Some(&boundary_point) {
                points.push(boundary_point);
            }
        }
        let current_point = Point::new(current_pos, current_value);
        if points.last() != Some(&current_point) {
            points.push(current_point);
        }
    }

    fn collapse_step_points(points: &[Point]) -> Vec<Point> {
        if points.len() <= 2 {
            return points.to_vec();
        }
        let mut result: Vec<Point> = Vec::new();
        let mut index = 0usize;
        while index < points.len() {
            let point = points[index];
            if let Some(next_point) = points.get(index + 1).copied() {
                if next_point.x == point.x + 1
                    && next_point.y != point.y
                    && ((index == 0 && point.y == 0)
                        || result.iter().any(|previous| previous.y == point.y))
                {
                    result.push(next_point);
                    index += 2;
                    continue;
                }
            }
            if result.last() != Some(&point) {
                result.push(point);
            }
            index += 1;
        }
        result
    }

    /// `SimpleControllerHandler.convert_vocaloid_curve_to_param_points`.
    pub fn convert_vocaloid_curve_to_param_points(
        &self,
        curve: &ControllerCurve,
        position_offset: i64,
        reverse_value: bool,
    ) -> Vec<Point> {
        if curve.is_empty() {
            return Vec::new();
        }
        let mut points: Vec<Point> = Vec::new();
        let mut previous_value = self.map_external_to_internal(
            curve.default_value,
            curve.default_value,
            curve.min_value,
            curve.max_value,
            reverse_value,
        );
        for event in &curve.events {
            let current_pos = event.pos + position_offset;
            let current_value = self.map_external_to_internal(
                event.value,
                curve.default_value,
                curve.min_value,
                curve.max_value,
                reverse_value,
            );
            Self::append_step_points(&mut points, current_pos, previous_value, current_value);
            previous_value = current_value;
        }
        points
    }

    /// `SimpleControllerHandler.convert_param_points_to_vocaloid_curve`.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_param_points_to_vocaloid_curve(
        &self,
        points: &[Point],
        param_name: &str,
        position_offset: i64,
        default_value: Option<i64>,
        min_value: Option<i64>,
        max_value: Option<i64>,
        reverse_value: bool,
    ) -> ControllerCurve {
        let param_def = get_param_def(param_name);
        let default_val = default_value
            .or_else(|| param_def.map(|definition| definition.default_value))
            .unwrap_or(0);
        let min_val = min_value
            .or_else(|| param_def.map(|definition| definition.min_value))
            .unwrap_or(-127);
        let max_val = max_value
            .or_else(|| param_def.map(|definition| definition.max_value))
            .unwrap_or(127);

        let real_points: Vec<Point> = points
            .iter()
            .copied()
            .filter(|point| {
                point.x != Point::start_point().x && point.x != Point::end_point().x
            })
            .collect();
        // Both Python branches (`_simplify_points` for simplify+len>2, plain
        // `_collapse_step_points` otherwise) end up collapsing step points.
        let real_points = Self::collapse_step_points(&real_points);

        let events = real_points
            .iter()
            .map(|point| ControllerEvent {
                pos: point.x + position_offset,
                value: self.map_internal_to_external(
                    point.y,
                    default_val,
                    min_val,
                    max_val,
                    reverse_value,
                ),
            })
            .collect();

        ControllerCurve::with_events(param_name, events, default_val, min_val, max_val)
    }
}

/// `convert_vocaloid_curve_to_param_points` (module-level helper).
pub fn convert_vocaloid_curve_to_param_points(
    curve: &ControllerCurve,
    position_offset: i64,
    reverse_value: bool,
) -> Vec<Point> {
    SimpleControllerHandler::default().convert_vocaloid_curve_to_param_points(
        curve,
        position_offset,
        reverse_value,
    )
}

/// `convert_param_points_to_vocaloid_curve` (module-level helper,
/// `simplify=False`).
#[allow(clippy::too_many_arguments)]
pub fn convert_param_points_to_vocaloid_curve(
    points: &[Point],
    param_name: &str,
    position_offset: i64,
    default_value: Option<i64>,
    min_value: Option<i64>,
    max_value: Option<i64>,
    reverse_value: bool,
) -> ControllerCurve {
    SimpleControllerHandler {
        simplify: false,
        ..SimpleControllerHandler::default()
    }
    .convert_param_points_to_vocaloid_curve(
        points,
        param_name,
        position_offset,
        default_value,
        min_value,
        max_value,
        reverse_value,
    )
}

// ---------------------------------------------------------------------------
// VprControllerAdapter
// ---------------------------------------------------------------------------

pub struct VprControllerAdapter;

/// `VPR_PARAM_OVERRIDES`: gender uses a negative lower bound in VPR.
const GENDER_OVERRIDE: (&str, (i64, i64, i64)) = ("gender", (0, -64, 63));

impl Default for VprControllerAdapter {
    fn default() -> Self {
        Self
    }
}

impl VprControllerAdapter {
    pub fn new() -> Self {
        Self
    }

    /// `get_param_metadata` — `(default_value, min_value, max_value)`.
    pub fn get_param_metadata(&self, param_name: &str) -> Option<(i64, i64, i64)> {
        if param_name == GENDER_OVERRIDE.0 {
            return Some(GENDER_OVERRIDE.1);
        }
        get_param_def(param_name)
            .map(|definition| (definition.default_value, definition.min_value, definition.max_value))
    }

    fn convert_to_curve(&self, controller: &VocaloidControllers) -> Option<ControllerCurve> {
        let controller_name = controller.name.as_deref()?;
        let param_def = get_param_by_format_vpr(controller_name);

        let (name, default_value, min_value, max_value) = match param_def {
            Some(definition) => {
                let name = definition.name;
                let metadata = self
                    .get_param_metadata(name)
                    .unwrap_or((definition.default_value, definition.min_value, definition.max_value));
                (name.to_owned(), metadata.0, metadata.1, metadata.2)
            }
            None => (controller_name.to_owned(), 0, -127, 127),
        };

        let events: Vec<ControllerEvent> = controller
            .events
            .iter()
            .filter_map(|event| {
                event
                    .value
                    .as_ref()
                    .and_then(PointValue::as_py_int)
                    .map(|value| ControllerEvent {
                        pos: event.pos,
                        value,
                    })
            })
            .collect();

        Some(ControllerCurve::with_events(
            name,
            events,
            default_value,
            min_value,
            max_value,
        ))
    }

    /// `extract` — the curve of a known parameter name (`dynamics`, …).
    pub fn extract(
        &self,
        part: &VocaloidVoicePart,
        param_name: &str,
    ) -> Option<ControllerCurve> {
        let param_def = get_param_def(param_name)?;
        let vpr_name = param_def.vpr_name?;
        let controllers = part.controllers.as_deref().unwrap_or(&[]);
        controllers
            .iter()
            .find(|controller| controller.name.as_deref() == Some(vpr_name))
            .and_then(|controller| self.convert_to_curve(controller))
    }

    /// `extract_by_vpr_name` — the curve stored under a raw VPR name.
    pub fn extract_by_vpr_name(
        &self,
        part: &VocaloidVoicePart,
        vpr_name: &str,
    ) -> Option<ControllerCurve> {
        let controllers = part.controllers.as_deref().unwrap_or(&[]);
        controllers
            .iter()
            .find(|controller| controller.name.as_deref() == Some(vpr_name))
            .and_then(|controller| self.convert_to_curve(controller))
    }

    /// `extract_all` — every non-empty controller.
    #[allow(dead_code)]
    pub fn extract_all(&self, part: &VocaloidVoicePart) -> Vec<ControllerCurve> {
        let mut curves = Vec::new();
        if let Some(controllers) = part.controllers.as_deref() {
            for controller in controllers {
                if let Some(curve) = self.convert_to_curve(controller) {
                    if !curve.is_empty() {
                        curves.push(curve);
                    }
                }
            }
        }
        curves
    }

    /// `create` — a controller for `curve`, named with the VPR parameter name.
    pub fn create(&self, curve: &ControllerCurve) -> Option<VocaloidControllers> {
        if curve.is_empty() {
            return None;
        }
        let vpr_name = get_param_def(&curve.name)
            .and_then(|definition| definition.vpr_name)
            .map(str::to_owned)
            .unwrap_or_else(|| curve.name.clone());
        let events = curve
            .events
            .iter()
            .map(|event| VocaloidPoint {
                pos: event.pos,
                value: Some(PointValue::Int(event.value)),
            })
            .collect();
        Some(VocaloidControllers {
            name: Some(vpr_name),
            events,
        })
    }

    /// `create_multiple`.
    pub fn create_multiple(&self, curves: &[ControllerCurve]) -> Vec<VocaloidControllers> {
        curves
            .iter()
            .filter_map(|curve| self.create(curve))
            .collect()
    }
}

/// `extract_pitch_data` — the PIT/PBS pair of a part, if it has a PIT curve.
pub fn extract_pitch_data(
    part: &VocaloidVoicePart,
) -> Option<(ControllerCurve, ControllerCurve)> {
    let adapter = VprControllerAdapter::new();
    let pit_curve = adapter.extract(part, "pitch_bend")?;
    let pbs_curve = adapter.extract(part, "pitch_bend_sens").unwrap_or_else(|| ControllerCurve {
        name: "pitch_bend_sens".to_owned(),
        events: Vec::new(),
        default_value: DEFAULT_PITCH_BEND_SENSITIVITY,
        min_value: 1,
        max_value: MAX_PITCH_BEND_SENSITIVITY,
    });
    Some((pit_curve, pbs_curve))
}

/// `create_pitch_controllers`.
#[allow(dead_code)]
pub fn create_pitch_controllers(
    pit_curve: &ControllerCurve,
    pbs_curve: &ControllerCurve,
) -> Vec<VocaloidControllers> {
    let adapter = VprControllerAdapter::new();
    let mut controllers = Vec::new();
    if let Some(controller) = adapter.create(pit_curve) {
        controllers.push(controller);
    }
    if let Some(controller) = adapter.create(pbs_curve) {
        controllers.push(controller);
    }
    controllers
}
