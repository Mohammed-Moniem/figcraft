//! Uniform corners on the computed Boolean result, without changing any document operand.

use figcraft_doc::VectorNetwork;
use figcraft_geom::kurbo::BezPath;

use crate::{MAX_SEGMENTS, PathOpsError, finite};

/// Round a normalized closed Boolean result using the existing vector corner construction.
/// Keep contour orientation; callers retain the Boolean's even-odd fill rule. Radius zero
/// returns the original elements exactly. Pre/post caps bound the temporary network and result.
pub fn round_boolean_result(path: &BezPath, radius: f64) -> Result<BezPath, PathOpsError> {
    // Check the cheap count before scanning or allocating geometry.
    if path.elements().len() > MAX_SEGMENTS {
        return Err(PathOpsError::TooComplex);
    }
    if !radius.is_finite() || radius < 0.0 || !finite(path) {
        return Err(PathOpsError::BadInput);
    }
    if radius == 0.0 {
        return Ok(path.clone());
    }
    // Each input element adds at most one vertex/segment. Rounding adds at most one of each
    // per corner, so the temporary network stays bounded before the output segment check.
    let mut network = VectorNetwork::from_path(path);
    for vertex in &mut network.vertices {
        vertex.radius = radius;
    }
    let rounded = network.fill_path();
    if rounded.segments().count() > MAX_SEGMENTS {
        return Err(PathOpsError::TooComplex);
    }
    if !finite(&rounded) {
        return Err(PathOpsError::BadInput);
    }
    Ok(rounded)
}

#[cfg(test)]
mod tests {
    use super::round_boolean_result;
    use crate::{BoolOp, MAX_SEGMENTS, PathOpsError, boolean};
    use figcraft_geom::kurbo::{BezPath, PathEl, Point, Rect, Shape};

    #[test]
    fn boolean_radius_helper_keeps_closed_holes_and_both_orientations() {
        let outer = Rect::new(0.0, 0.0, 140.0, 120.0).to_path(0.1);
        let hole = Rect::new(45.0, 35.0, 95.0, 85.0).to_path(0.1);
        let path = boolean(&[outer, hole], BoolOp::Subtract).unwrap();
        for path in [path.clone(), <BezPath as figcraft_geom::ReverseSubpaths>::reverse_subpaths(&path)] {
            assert_eq!(round_boolean_result(&path, 0.0).unwrap(), path);
            let rounded = round_boolean_result(&path, 8.0).unwrap();
            assert_eq!(rounded.winding(Point::new(70.0, 60.0)) % 2, 0);
            assert_ne!(rounded.winding(Point::new(25.0, 60.0)) % 2, 0);
            assert_eq!(rounded.winding(Point::new(0.5, 0.5)) % 2, 0);
            assert_ne!(rounded.winding(Point::new(45.5, 35.5)) % 2, 0);
            assert_eq!(rounded.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 2);
        }
    }

    #[test]
    fn boolean_radius_helper_bounds_hostile_geometry_and_preserves_finite_clamping() {
        let path = Rect::new(0.0, 0.0, 30.0, 20.0).to_path(0.1);
        for radius in [f64::NAN, f64::INFINITY, -1.0] {
            assert_eq!(round_boolean_result(&path, radius), Err(PathOpsError::BadInput));
        }
        let huge_radius = round_boolean_result(&path, 1e100).unwrap();
        assert!(crate::finite(&huge_radius));
        assert!(huge_radius.area().abs() > 0.0 && huge_radius.area().abs() < path.area().abs());
        let mut hostile = BezPath::new();
        hostile.move_to((f64::NAN, 0.0));
        hostile.line_to((1.0, 1.0));
        assert_eq!(round_boolean_result(&hostile, 2.0), Err(PathOpsError::BadInput));
        let oversized: BezPath = std::iter::repeat_n(PathEl::MoveTo(Point::new(0.0, 0.0)), MAX_SEGMENTS + 1).collect();
        assert_eq!(round_boolean_result(&oversized, 2.0), Err(PathOpsError::TooComplex));
    }
}
