//! `gazebo_boxes`: coloured boxes where the objects of the PX4 Gazebo world
//! (`bv_mission.sdf`) stand, for checking that Bevy and Gazebo line up.

use bevy::prelude::*;

use super::DebugCameraStart;
use super::shared::{DaylightPlugin, spawn_ground};
use crate::sim::gazebo_position_to_bevy;

const GROUND_SIZE: f32 = 100.0;


pub struct GazeboBoxesPlugin;

impl Plugin for GazeboBoxesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugCameraStart(Transform::from_xyz(6.0, 5.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y)))
            .add_plugins(DaylightPlugin)
            .add_systems(Startup, (spawn_world, super::ready));
    }
}

fn spawn_world(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_ground(&mut commands, &asset_server, &mut meshes, &mut materials, GROUND_SIZE);
    spawn_boxes(&mut commands, &mut meshes, &mut materials);
}

fn spawn_boxes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    spawn_box(
        commands,
        meshes,
        materials,
        "Hatchback",
        Vec3::new(2.0, 0.0, 0.16),
        Vec3::new(0.32, 0.3, 0.68),
        Color::srgb(0.75, 0.04, 0.03),
    );

    commands.spawn((
        Name::new("Soccer ball"),
        Mesh3d(meshes.add(Sphere::new(0.11))),
        MeshMaterial3d(materials.add(Color::srgb(0.92, 0.92, 0.88))),
        Transform::from_translation(gazebo_position_to_bevy(Vec3::new(-2.0, 0.0, 0.11))),
    ));

    spawn_box(
        commands,
        meshes,
        materials,
        "RC Cessna",
        Vec3::new(0.0, 2.0, 0.12),
        Vec3::new(0.85, 0.12, 0.55),
        Color::srgb(0.93, 0.93, 0.96),
    );
    spawn_box(
        commands,
        meshes,
        materials,
        "Walking person",
        Vec3::new(-2.0, 2.0, 0.85),
        Vec3::new(0.35, 1.7, 0.35),
        Color::srgb(0.13, 0.28, 0.72),
    );
    spawn_box(
        commands,
        meshes,
        materials,
        "Bus",
        Vec3::new(2.0, 2.0, 0.22),
        Vec3::new(0.45, 0.42, 1.1),
        Color::srgb(0.94, 0.66, 0.05),
    );
    spawn_box(
        commands,
        meshes,
        materials,
        "Stop sign",
        Vec3::new(2.0, -2.0, 0.65),
        Vec3::new(0.08, 1.3, 0.75),
        Color::srgb(0.82, 0.03, 0.03),
    );
    commands.spawn((
        Name::new("Suitcase calibration target"),
        Mesh3d(meshes.add(Cuboid::new(0.8, 0.05, 0.8))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 1.0),
            unlit: true,
            ..default()
        })),
        Transform::from_translation(gazebo_position_to_bevy(Vec3::new(0.0, -2.0, 0.075))),
    ));
}

fn spawn_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    name: &'static str,
    gazebo_position: Vec3,
    size: Vec3,
    color: Color,
) {
    commands.spawn((
        Name::new(name),
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_translation(gazebo_position_to_bevy(gazebo_position)),
    ));
}
