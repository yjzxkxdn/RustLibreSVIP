//! Port of `libresvip.plugins.dspx.param_utils`.

use std::collections::BTreeMap;

use libresvip_core::model::base::ParamCurve as LibreParamCurve;
use libresvip_core::model::point::{Point, END_POINT_X, START_POINT_X};

use super::model::{AnchorNode, Param, ParamCurve};
use super::pyutil::{ceil_to_interval, clamp, floor_to_interval, py_round, round_to_interval};

pub const SAMPLE_INTERVAL: i64 = 5;
pub const PITCH_BREAK_VALUE: i64 = -100;
pub const PITCH_MIN: f64 = 0.0;
pub const PITCH_MAX: f64 = 12800.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitchImportMode {
    EditedAndOriginal,
    EditedOnly,
}

fn curve_bounds(curve: &ParamCurve) -> Option<(i64, i64)> {
    match curve {
        ParamCurve::Free {
            start,
            step,
            values,
        } => {
            if values.is_empty() {
                None
            } else {
                Some((*start, start + step * (values.len() as i64 - 1)))
            }
        }
        ParamCurve::Anchor { start, nodes } => {
            if nodes.is_empty() {
                None
            } else {
                let mut minimum = i64::MAX;
                let mut maximum = i64::MIN;
                for node in nodes {
                    let position = start + node.x;
                    minimum = minimum.min(position);
                    maximum = maximum.max(position);
                }
                Some((minimum, maximum))
            }
        }
    }
}

fn combined_bounds<'a>(curves: impl Iterator<Item = &'a ParamCurve>) -> Option<(i64, i64)> {
    let mut minimum = None;
    let mut maximum = None;
    for curve in curves {
        if let Some((low, high)) = curve_bounds(curve) {
            minimum = Some(minimum.map_or(low, |value: i64| value.min(low)));
            maximum = Some(maximum.map_or(high, |value: i64| value.max(high)));
        }
    }
    match (minimum, maximum) {
        (Some(low), Some(high)) => Some((low, high)),
        _ => None,
    }
}

fn free_curve_value(
    start: i64,
    step: i64,
    values: &[i64],
    position: i64,
) -> Option<f64> {
    if values.is_empty() || step <= 0 {
        return None;
    }
    let relative = position - start;
    let last_offset = step * (values.len() as i64 - 1);
    if relative < 0 || relative > last_offset {
        return None;
    }
    let left_index = relative.div_euclid(step);
    let remainder = relative.rem_euclid(step);
    if remainder == 0 {
        return values.get(left_index as usize).map(|value| *value as f64);
    }
    let right_index = left_index + 1;
    if right_index as usize >= values.len() {
        return None;
    }
    let left = values[left_index as usize];
    let right = values[right_index as usize];
    let ratio = remainder as f64 / step as f64;
    Some(left as f64 + (right - left) as f64 * ratio)
}

#[derive(Clone, Debug, Default)]
pub struct FreeCurveEvaluator {
    curves: Vec<(i64, i64, Vec<i64>)>,
}

impl FreeCurveEvaluator {
    pub fn new(curves: &[ParamCurve]) -> Self {
        let curves = curves
            .iter()
            .filter_map(|curve| match curve {
                ParamCurve::Free {
                    start,
                    step,
                    values,
                } => Some((*start, *step, values.clone())),
                ParamCurve::Anchor { .. } => None,
            })
            .collect();
        Self { curves }
    }

    pub fn evaluate(&self, position: i64) -> Option<f64> {
        let mut result = None;
        for (start, step, values) in &self.curves {
            if let Some(value) = free_curve_value(*start, *step, values, position) {
                result = Some(value);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AbsoluteAnchorNode {
    pub x: i64,
    pub y: i64,
    pub interp_none: bool,
    pub interp_linear: bool,
}

fn absolute_nodes(curve: &ParamCurve, force_last_none: bool) -> Vec<AbsoluteAnchorNode> {
    let (start, nodes) = match curve {
        ParamCurve::Anchor { start, nodes } => (*start, nodes),
        ParamCurve::Free { .. } => return Vec::new(),
    };
    let mut result: Vec<AbsoluteAnchorNode> = nodes
        .iter()
        .map(|node: &AnchorNode| AbsoluteAnchorNode {
            x: start + node.x,
            y: node.y,
            interp_none: node.interp == "none",
            interp_linear: node.interp == "linear",
        })
        .collect();
    if force_last_none {
        if let Some(last) = result.last_mut() {
            last.interp_none = true;
            last.interp_linear = false;
        }
    }
    result
}

fn deduplicate_anchor_nodes(nodes: impl Iterator<Item = AbsoluteAnchorNode>) -> Vec<AbsoluteAnchorNode> {
    let mut first_nodes: BTreeMap<i64, AbsoluteAnchorNode> = BTreeMap::new();
    for node in nodes {
        first_nodes.entry(node.x).or_insert(node);
    }
    first_nodes.into_values().collect()
}

fn secant(left: AbsoluteAnchorNode, right: AbsoluteAnchorNode) -> f64 {
    (right.y - left.y) as f64 / (right.x - left.x) as f64
}

fn fritsch_butland(
    left: AbsoluteAnchorNode,
    center: AbsoluteAnchorNode,
    right: AbsoluteAnchorNode,
) -> f64 {
    let left_width = (center.x - left.x) as f64;
    let right_width = (right.x - center.x) as f64;
    let left_delta = (center.y - left.y) as f64 / left_width;
    let right_delta = (right.y - center.y) as f64 / right_width;
    if left_delta * right_delta <= 0.0 {
        return 0.0;
    }
    let weight_1 = 2.0 * right_width + left_width;
    let weight_2 = right_width + 2.0 * left_width;
    (weight_1 + weight_2) / (weight_1 / left_delta + weight_2 / right_delta)
}

fn hermite_value(nodes: &[AbsoluteAnchorNode], left_index: usize, position: i64) -> f64 {
    let left = nodes[left_index];
    let right = nodes[left_index + 1];
    let left_slope = if left_index > 0 {
        fritsch_butland(nodes[left_index - 1], left, right)
    } else {
        secant(left, right)
    };
    let right_slope = if left_index + 2 < nodes.len() {
        fritsch_butland(left, right, nodes[left_index + 2])
    } else {
        secant(left, right)
    };
    let width = (right.x - left.x) as f64;
    let ratio = (position - left.x) as f64 / width;
    let ratio_2 = ratio * ratio;
    let ratio_3 = ratio_2 * ratio;
    (2.0 * ratio_3 - 3.0 * ratio_2 + 1.0) * left.y as f64
        + (ratio_3 - 2.0 * ratio_2 + ratio) * width * left_slope
        + (-2.0 * ratio_3 + 3.0 * ratio_2) * right.y as f64
        + (ratio_3 - ratio_2) * width * right_slope
}

fn anchor_nodes_value(nodes: &[AbsoluteAnchorNode], position: i64) -> Option<f64> {
    if nodes.is_empty() {
        return None;
    }
    let positions: Vec<i64> = nodes.iter().map(|node| node.x).collect();
    let right_index = positions.partition_point(|x| *x < position);
    if right_index < nodes.len() && nodes[right_index].x == position {
        return Some(nodes[right_index].y as f64);
    }
    if right_index == 0 || right_index == nodes.len() {
        return None;
    }
    let left_index = right_index - 1;
    let left = nodes[left_index];
    let right = nodes[right_index];
    if left.interp_none {
        return None;
    }
    if left.interp_linear {
        let ratio = (position - left.x) as f64 / (right.x - left.x) as f64;
        return Some(left.y as f64 + (right.y - left.y) as f64 * ratio);
    }
    Some(hermite_value(nodes, left_index, position))
}

#[derive(Clone, Debug, Default)]
pub struct AnchorCurveEvaluator {
    nodes: Vec<AbsoluteAnchorNode>,
}

impl AnchorCurveEvaluator {
    pub fn new(curves: &[ParamCurve]) -> Self {
        let nodes = deduplicate_anchor_nodes(
            curves
                .iter()
                .filter(|curve| matches!(curve, ParamCurve::Anchor { .. }))
                .flat_map(|curve| absolute_nodes(curve, true)),
        );
        Self { nodes }
    }

    pub fn evaluate(&self, position: i64) -> Option<f64> {
        anchor_nodes_value(&self.nodes, position)
    }
}

#[derive(Clone, Debug, Default)]
pub struct OriginalCurveEvaluator {
    free: FreeCurveEvaluator,
    anchors: Vec<Vec<AbsoluteAnchorNode>>,
}

impl OriginalCurveEvaluator {
    pub fn new(curves: &[ParamCurve]) -> Self {
        let anchors = curves
            .iter()
            .filter(|curve| matches!(curve, ParamCurve::Anchor { .. }))
            .map(|curve| deduplicate_anchor_nodes(absolute_nodes(curve, true).into_iter()))
            .collect();
        Self {
            free: FreeCurveEvaluator::new(curves),
            anchors,
        }
    }

    pub fn evaluate(&self, position: i64) -> Option<f64> {
        let mut result = self.free.evaluate(position);
        for nodes in &self.anchors {
            if let Some(value) = anchor_nodes_value(nodes, position) {
                result = Some(value);
            }
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedParam {
    mode: PitchImportMode,
    edited_anchor: AnchorCurveEvaluator,
    edited_free: FreeCurveEvaluator,
    original: OriginalCurveEvaluator,
    transform_anchor: AnchorCurveEvaluator,
    transform_free: FreeCurveEvaluator,
    pub bounds: Option<(i64, i64)>,
}

impl ResolvedParam {
    pub fn new(parameter: &Param, mode: PitchImportMode) -> Self {
        let mut relevant: Vec<&ParamCurve> = Vec::new();
        relevant.extend(parameter.edited.iter());
        relevant.extend(parameter.transform.iter());
        if mode == PitchImportMode::EditedAndOriginal {
            relevant.extend(parameter.original.iter());
        }
        Self {
            mode,
            edited_anchor: AnchorCurveEvaluator::new(&parameter.edited),
            edited_free: FreeCurveEvaluator::new(&parameter.edited),
            original: OriginalCurveEvaluator::new(&parameter.original),
            transform_anchor: AnchorCurveEvaluator::new(&parameter.transform),
            transform_free: FreeCurveEvaluator::new(&parameter.transform),
            bounds: combined_bounds(relevant.into_iter()),
        }
    }

    pub fn evaluate(&self, position: i64) -> Option<f64> {
        let mut source = self.edited_anchor.evaluate(position);
        if source.is_none() {
            source = self.edited_free.evaluate(position);
        }
        if source.is_none() && self.mode == PitchImportMode::EditedAndOriginal {
            source = self.original.evaluate(position);
        }
        let source = source?;
        let mut transform = self.transform_anchor.evaluate(position);
        if transform.is_none() {
            transform = self.transform_free.evaluate(position);
        }
        Some(source * (transform.map_or(1.0, |value| value / 1000.0)))
    }
}

pub fn build_pitch_curve(
    pitch: &ResolvedParam,
    coordinate_offset: i64,
    vibrato_value: Option<&dyn Fn(i64) -> f64>,
) -> LibreParamCurve {
    let Some((bounds_start, bounds_end)) = pitch.bounds else {
        return LibreParamCurve::default();
    };
    let start = ceil_to_interval(bounds_start);
    let end = floor_to_interval(bounds_end);
    if start > end {
        return LibreParamCurve::default();
    }
    let mut sampled: Vec<(i64, Option<i64>)> = Vec::new();
    let mut relative_tick = start;
    while relative_tick <= end {
        let mut value = pitch.evaluate(relative_tick);
        if let (Some(known), Some(vibrato)) = (value, vibrato_value) {
            value = Some(known + vibrato(relative_tick));
        }
        sampled.push((
            relative_tick + coordinate_offset,
            value.map(|value| py_round(clamp(value, PITCH_MIN, PITCH_MAX))),
        ));
        relative_tick += SAMPLE_INTERVAL;
    }
    let mut points: Vec<Point> = vec![Point::start_point()];
    let mut run_last_x: Option<i64> = None;
    for (x, value) in sampled {
        match value {
            None => {
                if let Some(last) = run_last_x.take() {
                    points.push(Point::new(last, PITCH_BREAK_VALUE));
                }
            }
            Some(value) => {
                if run_last_x.is_none() {
                    points.push(Point::new(x, PITCH_BREAK_VALUE));
                }
                points.push(Point::new(x, value));
                run_last_x = Some(x);
            }
        }
    }
    if let Some(last) = run_last_x {
        points.push(Point::new(last, PITCH_BREAK_VALUE));
    }
    if points.len() == 1 {
        return LibreParamCurve::default();
    }
    points.push(Point::end_point());
    LibreParamCurve::new(points)
}

fn split_internal_curve(points: &[Point]) -> Vec<Vec<Point>> {
    let mut indexed: Vec<(usize, Point)> = points.iter().copied().enumerate().collect();
    indexed.sort_by_key(|(index, point)| (point.x, *index));
    let mut segments: Vec<Vec<Point>> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    for (_, point) in indexed {
        if point.x == START_POINT_X || point.x == END_POINT_X || point.y == PITCH_BREAK_VALUE {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(point);
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn deduplicate_points(points: impl Iterator<Item = Point>) -> Vec<Point> {
    let mut result: BTreeMap<i64, Point> = BTreeMap::new();
    for point in points {
        result.insert(point.x, point);
    }
    result.into_values().collect()
}

fn linear_value(points: &[Point], point_positions: &[i64], position: i64) -> Option<f64> {
    if points.is_empty() {
        return None;
    }
    let right_index = point_positions.partition_point(|x| *x < position);
    if right_index < points.len() && points[right_index].x == position {
        return Some(points[right_index].y as f64);
    }
    if right_index == 0 || right_index == points.len() {
        return None;
    }
    let left = points[right_index - 1];
    let right = points[right_index];
    let ratio = (position - left.x) as f64 / (right.x - left.x) as f64;
    Some(left.y as f64 + (right.y - left.y) as f64 * ratio)
}

pub fn export_pitch_param(curve: &LibreParamCurve, coordinate_offset: i64) -> Option<Param> {
    let mut free_curves: Vec<ParamCurve> = Vec::new();
    for segment in split_internal_curve(&curve.points) {
        let points = deduplicate_points(segment.iter().map(|point| {
            Point::new(
                point.x - coordinate_offset,
                py_round(clamp(point.y as f64, PITCH_MIN, PITCH_MAX)),
            )
        }));
        if points.is_empty() {
            continue;
        }
        let start = ceil_to_interval(points[0].x);
        let end = floor_to_interval(points[points.len() - 1].x);
        let sampled_values: Vec<(i64, i64)>;
        if start > end {
            let sample_position = round_to_interval(points[0].x);
            sampled_values = vec![(sample_position, points[0].y)];
        } else {
            let point_positions: Vec<i64> = points.iter().map(|point| point.x).collect();
            let mut values = Vec::new();
            let mut position = start;
            while position <= end {
                if let Some(value) = linear_value(&points, &point_positions, position) {
                    values.push((position, py_round(clamp(value, PITCH_MIN, PITCH_MAX))));
                }
                position += SAMPLE_INTERVAL;
            }
            sampled_values = values;
        }
        if !sampled_values.is_empty() {
            free_curves.push(ParamCurve::Free {
                start: sampled_values[0].0,
                step: SAMPLE_INTERVAL,
                values: sampled_values.into_iter().map(|(_, value)| value).collect(),
            });
        }
    }
    if free_curves.is_empty() {
        return None;
    }
    Some(Param {
        edited: free_curves,
        transform: Vec::new(),
        original: Vec::new(),
    })
}

pub fn make_anchor_node(x: i64, y: i64, interp: &str) -> AnchorNode {
    AnchorNode {
        interp: interp.to_owned(),
        x,
        y,
    }
}
