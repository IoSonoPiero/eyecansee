//! CAD entities as points: the lines and filled outlines of the plain entities, a hatch's
//! boundaries, and an ellipse whose normal points down mirrored.

use acadrust::entities::EntityType;
use acadrust::types::Vector3;

use crate::cad::{n3, xy};
use crate::cad_geom::{self, Affine, P};

/// A hatch boundary as a closed list of points.
pub(crate) fn boundary(path: &acadrust::entities::BoundaryPath) -> Vec<P> {
    use acadrust::entities::BoundaryEdge as E;
    let mut pts: Vec<P> = Vec::new();
    for edge in &path.edges {
        let part = match edge {
            E::Line(l) => vec![[l.start.x, l.start.y], [l.end.x, l.end.y]],
            E::CircularArc(a) => {
                let c = [a.center.x, a.center.y];
                if a.counter_clockwise {
                    cad_geom::arc(c, a.radius, a.start_angle, a.end_angle)
                } else {
                    // Clockwise arcs store their angles measured clockwise.
                    let mut p = cad_geom::arc(c, a.radius, -a.end_angle, -a.start_angle);
                    p.reverse();
                    p
                }
            }
            E::EllipticArc(e) => {
                let mut p = cad_geom::ellipse([e.center.x, e.center.y], [e.major_axis_endpoint.x, e.major_axis_endpoint.y], e.minor_axis_ratio, e.start_angle, e.end_angle);
                if !e.counter_clockwise {
                    p.reverse();
                }
                p
            }
            E::Spline(s) => {
                let control: Vec<P> = s.control_points.iter().map(xy).collect();
                cad_geom::spline(s.degree.max(0) as usize, &s.knots, &control, &[]).unwrap_or(control)
            }
            E::Polyline(p) => {
                let v: Vec<(P, f64)> = p.vertices.iter().map(|v| ([v.x, v.y], v.z)).collect();
                cad_geom::bulged(&v, p.is_closed)
            }
        };
        pts.extend(part);
    }
    if let Some(&first) = pts.first() {
        pts.push(first);
    }
    pts
}

/// Lines and filled outlines, in their coordinate system: points and the fill's opacity.
pub(crate) fn shape(e: &EntityType) -> Option<(Affine, Vec<P>, Option<f32>)> {
    let ocs = |n: &Vector3| Affine::ocs(n3(n));
    let closed = |mut pts: Vec<P>, close: bool| {
        if close && let Some(&first) = pts.first() {
            pts.push(first);
        }
        pts
    };
    Some(match e {
        EntityType::Line(l) => (Affine::IDENTITY, vec![xy(&l.start), xy(&l.end)], None),
        EntityType::Circle(c) => (ocs(&c.normal), cad_geom::circle(xy(&c.center), c.radius), None),
        EntityType::Arc(a) => (ocs(&a.normal), cad_geom::arc(xy(&a.center), a.radius, a.start_angle, a.end_angle), None),
        EntityType::Ellipse(el) => {
            let pts = cad_geom::ellipse(xy(&el.center), xy(&el.major_axis), el.minor_axis_ratio, el.start_parameter, el.end_parameter);
            // The centre and axis are in world coordinates; a normal pointing down turns the parameter the other way.
            let pts = if el.normal.z < 0.0 { mirror_about(pts, xy(&el.center), xy(&el.major_axis)) } else { pts };
            (Affine::IDENTITY, pts, None)
        }
        EntityType::LwPolyline(p) => {
            let v: Vec<(P, f64)> = p.vertices.iter().map(|v| ([v.location.x, v.location.y], v.bulge)).collect();
            (ocs(&p.normal), cad_geom::bulged(&v, p.is_closed), None)
        }
        EntityType::Polyline2D(p) => {
            let v: Vec<(P, f64)> = p.vertices.iter().map(|v| (xy(&v.location), v.bulge)).collect();
            (ocs(&p.normal), cad_geom::bulged(&v, p.is_closed()), None)
        }
        EntityType::Polyline3D(p) => (Affine::IDENTITY, closed(p.vertices.iter().map(|v| xy(&v.position)).collect(), p.flags.closed), None),
        EntityType::Polyline(p) => (Affine::IDENTITY, closed(p.vertices.iter().map(|v| xy(&v.location)).collect(), p.flags.bits() & 1 != 0), None),
        EntityType::Spline(s) => {
            let control: Vec<P> = s.control_points.iter().map(xy).collect();
            let pts = cad_geom::spline(s.degree.max(0) as usize, &s.knots, &control, &s.weights).unwrap_or_else(|| s.fit_points.iter().map(xy).collect());
            (Affine::IDENTITY, pts, None)
        }
        // Corners in the order 1 2 4 3 make the outline.
        EntityType::Solid(s) => (ocs(&s.normal), vec![xy(&s.first_corner), xy(&s.second_corner), xy(&s.fourth_corner), xy(&s.third_corner), xy(&s.first_corner)], Some(1.0)),
        EntityType::Face3D(f) => (Affine::IDENTITY, vec![xy(&f.first_corner), xy(&f.second_corner), xy(&f.third_corner), xy(&f.fourth_corner), xy(&f.first_corner)], None),
        EntityType::Leader(l) => (Affine::IDENTITY, l.vertices.iter().map(xy).collect(), None),
        // The picture is another file: its frame with a cross.
        EntityType::RasterImage(r) => {
            let c = r.corners().map(|v| xy(&v));
            (Affine::IDENTITY, vec![c[0], c[1], c[2], c[3], c[0], c[2], c[3], c[1]], None)
        }
        _ => return None,
    })
}

/// An ellipse drawn with its normal pointing down: mirrored across its major axis.
fn mirror_about(pts: Vec<P>, c: P, major: P) -> Vec<P> {
    let len = major[0].hypot(major[1]);
    if len == 0.0 {
        return pts;
    }
    let (ux, uy) = (major[0] / len, major[1] / len);
    pts.into_iter()
        .map(|p| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let along = dx * ux + dy * uy;
            [c[0] + 2.0 * along * ux - dx, c[1] + 2.0 * along * uy - dy]
        })
        .collect()
}
