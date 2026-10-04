//! Scripted screenshot tour for reviewing the environment.
//!
//! `cargo run -- forest --tour <dir>` flies the desktop camera
//! through a fixed set of viewpoints (ground level, 5-20 m, 30-50 m, overhead),
//! saves one PNG per viewpoint into `<dir>`, and exits. Useful for visual
//! regression checks after changing assets or placement.

use std::path::PathBuf;

use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};

use crate::camera::DebugCamera;

/// Frames to wait at each viewpoint so LOD cross-fades and IBL settle.
const SETTLE_FRAMES: u32 = 45;
/// Minimum wall time per viewpoint, and before the first one: render pipelines
/// compile asynchronously and frames before that are empty.
const SETTLE_SECS: f32 = 1.5;
const WARMUP_SECS: f32 = 8.0;

#[derive(Resource)]
pub struct PreviewTour {
    dir: PathBuf,
    index: usize,
    wait: u32,
    frame_time: f32,
    frames: u32,
    elapsed: f32,
}

impl PreviewTour {
    pub fn new(dir: PathBuf) -> Self {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            warn!("preview: cannot create {}: {error}", dir.display());
        }
        Self { dir, index: 0, wait: SETTLE_FRAMES * 2, frame_time: 0.0, frames: 0, elapsed: 0.0 }
    }
}

fn viewpoints() -> Vec<(&'static str, Vec3, Vec3)> {
    // (name, eye, target) — Bevy frame: north = -X, east = -Z.
    vec![
        ("01_drone_45m_overview", Vec3::new(-75.0, 45.0, -70.0), Vec3::new(20.0, 0.0, 30.0)),
        ("02_drone_15m_pad", Vec3::new(-22.0, 14.0, -24.0), Vec3::new(12.0, 0.0, 18.0)),
        ("03_ground_yard", Vec3::new(14.0, 1.7, 34.0), Vec3::new(38.0, 1.5, 62.0)),
        ("04_ground_tree_line", Vec3::new(-62.0, 1.7, 8.0), Vec3::new(-115.0, 7.0, 22.0)),
        ("05_road_and_poles", Vec3::new(128.0, 2.2, 98.0), Vec3::new(62.0, 3.0, 54.0)),
        ("06_overhead_160m", Vec3::new(0.0, 230.0, 0.01), Vec3::ZERO),
        ("07_drone_30m_forest", Vec3::new(55.0, 30.0, -55.0), Vec3::new(140.0, 4.0, -125.0)),
        ("08_drone_50m_horizon", Vec3::new(-10.0, 50.0, 0.0), Vec3::new(300.0, 25.0, 40.0)),
        ("09_drone_8m_yard", Vec3::new(10.0, 8.0, 32.0), Vec3::new(40.0, 0.0, 62.0)),
        ("10_ground_forest_floor", Vec3::new(-128.0, 1.6, -40.0), Vec3::new(-150.0, 2.0, -60.0)),
    ]
}

pub fn run_tour(
    mut commands: Commands,
    mut tour: ResMut<PreviewTour>,
    mut camera: Query<&mut Transform, With<DebugCamera>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time>,
) {
    let shots = viewpoints();
    let Ok(mut transform) = camera.single_mut() else { return };
    if tour.index >= shots.len() {
        if tour.wait == 0 {
            exit.write(AppExit::Success);
        } else {
            tour.wait -= 1;
        }
        return;
    }
    let (name, eye, target) = shots[tour.index];
    *transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
    tour.elapsed += time.delta_secs();
    let min_secs = if tour.index == 0 { WARMUP_SECS } else { SETTLE_SECS };
    if tour.wait > 0 || tour.elapsed < min_secs {
        tour.wait = tour.wait.saturating_sub(1);
        // Measure the second half of the settle period.
        if tour.wait < SETTLE_FRAMES / 2 {
            tour.frame_time += time.delta_secs();
            tour.frames += 1;
        }
        return;
    }
    let ms = 1000.0 * tour.frame_time / tour.frames.max(1) as f32;
    info!("preview {name}: {ms:.1} ms/frame ({:.0} fps)", 1000.0 / ms.max(0.01));
    tour.frame_time = 0.0;
    tour.frames = 0;
    tour.elapsed = 0.0;
    let path = tour.dir.join(format!("{name}.png"));
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.index += 1;
    tour.wait = SETTLE_FRAMES;
}
