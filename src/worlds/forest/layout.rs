//! The site plan: clearing shape, road, work yard and the fields derived from them.
//!
//! Everything here is pure geometry so terrain, vegetation and structures agree on
//! where the tree line, road and yard are, and so the layout can be unit tested.

use bevy::prelude::*;

use super::config::ForestConfig;
use super::scatter::{fbm, smoothstep};

const NOISE_EDGE: u32 = 11;
const NOISE_HILLS: u32 = 23;

#[derive(Resource, Clone)]
pub struct SiteLayout {
    pub half_extent: f32,
    pub clearing_center: Vec2,
    pub clearing_radii: Vec2,
    pub edge_noise: f32,
    pub transition_width: f32,
    pub launch_pad_size: f32,
    pub launch_clear_radius: f32,
    /// Road centre line sampled every ~1 m, with cumulative length.
    pub road: Vec<Vec2>,
    pub road_length: Vec<f32>,
    pub road_width: f32,
    pub yard_center: Vec2,
    pub yard_half_size: Vec2,
    pub yard_heading: f32,
    pub hill_amplitude: f32,
    pub flat_margin: f32,
    pub flight_margin: f32,
    seed: u32,
}

impl SiteLayout {
    pub fn new(config: &ForestConfig) -> Self {
        let site = &config.site;
        let road = catmull_rom(&site.road_path, 1.0);
        let mut road_length = Vec::with_capacity(road.len());
        let mut total = 0.0;
        for (i, p) in road.iter().enumerate() {
            if i > 0 {
                total += p.distance(road[i - 1]);
            }
            road_length.push(total);
        }
        Self {
            half_extent: site.half_extent,
            clearing_center: site.clearing_center,
            clearing_radii: site.clearing_radii,
            edge_noise: site.clearing_edge_noise,
            transition_width: site.transition_width,
            launch_pad_size: site.launch_pad_size,
            launch_clear_radius: site.launch_clear_radius,
            road,
            road_length,
            road_width: site.road_width,
            yard_center: site.yard_center,
            yard_half_size: site.yard_half_size,
            yard_heading: site.yard_heading,
            hill_amplitude: config.terrain.hill_amplitude,
            flat_margin: config.terrain.flat_margin,
            flight_margin: config.lod.flight_margin,
            seed: config.seed as u32,
        }
    }

    // ------------------------------------------------------------------ clearing

    /// Signed distance to the tree line: negative inside the clearing.
    pub fn clearing_distance(&self, p: Vec2) -> f32 {
        let d = p - self.clearing_center;
        let angle = d.y.atan2(d.x);
        let (s, c) = angle.sin_cos();
        let r = 1.0 / ((c / self.clearing_radii.x).powi(2) + (s / self.clearing_radii.y).powi(2)).sqrt();
        // Periodic noise around the ellipse: sample fbm on a circle so it wraps.
        let ring = Vec2::new(c, s) * 2.2;
        let wobble = fbm(ring + Vec2::splat(4.0), 4, self.seed ^ NOISE_EDGE);
        // Small bays and promontories on top of the large-scale wobble.
        let detail = fbm(p * 0.06, 2, self.seed ^ (NOISE_EDGE + 1)) * 0.35;
        d.length() - (r + self.edge_noise * (wobble + detail))
    }

    /// 0 in the open clearing, 1 in dense forest, ramping across the transition band.
    pub fn forest_factor(&self, p: Vec2) -> f32 {
        let edge = self.clearing_distance(p);
        smoothstep(0.0, self.transition_width, edge)
    }

    // ------------------------------------------------------------------ road

    /// Distance to the road centre line and the arc length of the closest point.
    pub fn road_distance(&self, p: Vec2) -> (f32, f32) {
        let mut best = (f32::MAX, 0.0);
        for i in 1..self.road.len() {
            let (a, b) = (self.road[i - 1], self.road[i]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            let d = p.distance(a + ab * t);
            if d < best.0 {
                best = (d, self.road_length[i - 1] + t * ab.length());
            }
        }
        best
    }

    /// Point and unit direction along the road at arc length s.
    pub fn road_at(&self, s: f32) -> (Vec2, Vec2) {
        let s = s.clamp(0.0, *self.road_length.last().unwrap());
        let i = self.road_length.partition_point(|&l| l < s).clamp(1, self.road.len() - 1);
        let (a, b) = (self.road[i - 1], self.road[i]);
        let seg = self.road_length[i] - self.road_length[i - 1];
        let t = if seg > 0.0 { (s - self.road_length[i - 1]) / seg } else { 0.0 };
        (a.lerp(b, t), (b - a).normalize_or(Vec2::X))
    }

    pub fn road_total_length(&self) -> f32 {
        *self.road_length.last().unwrap()
    }

    // ------------------------------------------------------------------ yard

    /// Yard-local coordinates: x along the long side (towards the road), y away
    /// from the clearing.
    pub fn yard_local(&self, p: Vec2) -> Vec2 {
        let d = p - self.yard_center;
        let (u, v) = self.yard_axes();
        Vec2::new(d.dot(u), d.dot(v))
    }

    pub fn yard_world(&self, local: Vec2) -> Vec2 {
        let (u, v) = self.yard_axes();
        self.yard_center + u * local.x + v * local.y
    }

    /// World-space axes of the yard frame (XZ).
    pub fn yard_axes(&self) -> (Vec2, Vec2) {
        // Matches Quat::from_rotation_y(heading): local X -> (cos, -sin), local Z -> (sin, cos).
        let (s, c) = self.yard_heading.sin_cos();
        (Vec2::new(c, -s), Vec2::new(s, c))
    }

    /// Rotation for an object authored along local X/Z placed in the yard frame.
    pub fn yard_rotation(&self, extra_heading: f32) -> Quat {
        Quat::from_rotation_y(self.yard_heading + extra_heading)
    }

    /// Signed distance to the asphalt rectangle (negative inside).
    pub fn yard_distance(&self, p: Vec2) -> f32 {
        let q = self.yard_local(p).abs() - self.yard_half_size;
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
    }

    /// Footprint of the shed (yard-local centre, half size); see structures.rs.
    pub fn shed_footprint(&self) -> (Vec2, Vec2) {
        (Vec2::new(-3.0, self.yard_half_size.y + 3.4), Vec2::new(7.0, 2.6))
    }

    // ------------------------------------------------------------------ derived fields

    /// Distance to anything built (road, yard, shed, launch pad). Vegetation and
    /// clutter use this to stay off developed ground.
    pub fn developed_distance(&self, p: Vec2) -> f32 {
        let road = self.road_distance(p).0 - self.road_width * 0.5;
        let yard = self.yard_distance(p);
        let (shed_c, shed_h) = self.shed_footprint();
        let q = (self.yard_local(p) - shed_c).abs() - shed_h;
        let shed = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
        let pad = p.length() - self.launch_clear_radius;
        road.min(yard).min(shed).min(pad)
    }

    pub fn in_site(&self, p: Vec2) -> bool {
        p.x.abs() <= self.half_extent && p.y.abs() <= self.half_extent
    }

    /// Closest the camera is expected to get to p (0 inside the flight area).
    /// LODs that cannot be seen from there are not spawned.
    pub fn min_view_distance(&self, p: Vec2) -> f32 {
        let outside = (p.abs() - Vec2::splat(self.half_extent)).max(Vec2::ZERO).length();
        (outside - self.flight_margin).max(0.0)
    }

    /// Ground height. Exactly 0 across the clearing (Gazebo's ground plane is flat),
    /// gently rolling inside the forest and beyond.
    pub fn ground_height(&self, p: Vec2) -> f32 {
        let edge = self.clearing_distance(p);
        let ramp = smoothstep(self.flat_margin, self.flat_margin + 45.0, edge);
        if ramp <= 0.0 {
            return 0.0;
        }
        let hills = fbm(p / 140.0, 4, self.seed ^ NOISE_HILLS);
        let swells = fbm(p / 38.0, 2, self.seed ^ (NOISE_HILLS + 1)) * 0.25;
        // Keep the road bed smoother than its surroundings.
        let road = smoothstep(self.road_width, self.road_width + 10.0, self.road_distance(p).0);
        let far = smoothstep(self.half_extent, self.half_extent + 200.0, p.abs().max_element());
        ramp * self.hill_amplitude * (hills * (1.0 + 1.5 * far) + swells * road)
    }

    pub fn ground_normal(&self, p: Vec2) -> Vec3 {
        let e = 0.5;
        let dx = self.ground_height(p + Vec2::X * e) - self.ground_height(p - Vec2::X * e);
        let dz = self.ground_height(p + Vec2::Y * e) - self.ground_height(p - Vec2::Y * e);
        Vec3::new(-dx, 2.0 * e, -dz).normalize()
    }

    pub fn ground_point(&self, p: Vec2) -> Vec3 {
        Vec3::new(p.x, self.ground_height(p), p.y)
    }
}

/// Centripetal-ish Catmull-Rom through the control points, sampled every `step` m.
fn catmull_rom(points: &[Vec2], step: f32) -> Vec<Vec2> {
    let mut out = vec![points[0]];
    for i in 0..points.len() - 1 {
        let p0 = if i == 0 { points[0] * 2.0 - points[1] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < points.len() { points[i + 2] } else { p2 * 2.0 - p1 };
        let samples = (p1.distance(p2) / step).ceil().max(1.0) as usize;
        for s in 1..=samples {
            let t = s as f32 / samples as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            out.push(
                0.5 * ((2.0 * p1)
                    + (-p0 + p2) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    out
}

/// Compass azimuth (deg clockwise from north) to a horizontal Bevy direction.
pub fn compass_direction(azimuth_deg: f32) -> Vec3 {
    // north = -X, east = -Z
    let a = azimuth_deg.to_radians();
    Vec3::new(-a.cos(), 0.0, -a.sin())
}

/// Yaw that turns local +Z towards a horizontal direction.
pub fn yaw_towards(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

#[cfg(test)]
mod tests {
    use super::super::config::{ForestConfig, Quality};
    use super::*;
    use std::f32::consts::TAU;

    fn layout() -> SiteLayout {
        SiteLayout::new(&ForestConfig::new(Quality::Medium))
    }

    #[test]
    fn launch_area_is_open_flat_ground() {
        let layout = layout();
        for i in 0..64 {
            let p = Vec2::from_angle(i as f32 / 64.0 * TAU) * layout.launch_clear_radius * 2.0;
            assert!(layout.clearing_distance(p) < -20.0, "launch area must be deep inside the clearing");
            assert_eq!(layout.ground_height(p), 0.0);
        }
        assert_eq!(layout.forest_factor(Vec2::ZERO), 0.0);
    }

    #[test]
    fn site_corners_are_forest() {
        let layout = layout();
        for c in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
            assert_eq!(layout.forest_factor(c * 140.0), 1.0);
        }
    }

    #[test]
    fn road_reaches_the_yard_from_outside_the_site() {
        let layout = layout();
        assert!(!layout.in_site(layout.road[0]));
        let end = *layout.road.last().unwrap();
        assert!(layout.yard_distance(end) < 4.0, "road should end at the asphalt yard");
    }

    #[test]
    fn yard_frame_round_trips() {
        let layout = layout();
        let p = Vec2::new(31.0, 47.0);
        assert!(layout.yard_world(layout.yard_local(p)).abs_diff_eq(p, 1e-4));
        let rotated = layout.yard_rotation(0.0) * Vec3::X;
        let (u, _) = layout.yard_axes();
        assert!(Vec2::new(rotated.x, rotated.z).abs_diff_eq(u, 1e-5));
    }
}
