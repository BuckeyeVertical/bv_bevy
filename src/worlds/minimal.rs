//! `minimal`: flat grass with a grid, a landing pad and three obstacles.
//! Pairs with Gazebo's `gazebo/worlds/minimal.sdf` (no PX4).

use bevy::prelude::*;

use super::DebugCameraStart;
use super::shared::{DaylightPlugin, spawn_ground};

const GROUND_SIZE: f32 = 100.0;
const GRID_EXTENT: i32 = 20;
const GRID_SPACING: f32 = 2.0;


pub struct MinimalPlugin;

impl Plugin for MinimalPlugin {
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
    spawn_grid(&mut commands, &mut meshes, &mut materials);
    spawn_minimal(&mut commands, &mut meshes, &mut materials);
}

fn spawn_grid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let grid = materials.add(Color::srgba(0.72, 0.76, 0.65, 0.42));
    let line_length = GRID_EXTENT as f32 * GRID_SPACING * 2.0;
    let line_mesh_x = meshes.add(Cuboid::new(line_length, 0.008, 0.018));
    let line_mesh_z = meshes.add(Cuboid::new(0.018, 0.008, line_length));

    for index in -GRID_EXTENT..=GRID_EXTENT {
        let offset = index as f32 * GRID_SPACING;
        commands.spawn((
            Mesh3d(line_mesh_x.clone()),
            MeshMaterial3d(grid.clone()),
            Transform::from_xyz(0.0, 0.006, offset),
        ));
        commands.spawn((
            Mesh3d(line_mesh_z.clone()),
            MeshMaterial3d(grid.clone()),
            Transform::from_xyz(offset, 0.006, 0.0),
        ));
    }
}

fn spawn_minimal(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let pad = materials.add(Color::srgb(0.18, 0.2, 0.22));
    let obstacle = materials.add(Color::srgb(0.65, 0.29, 0.12));

    commands.spawn((
        Name::new("Landing pad"),
        Mesh3d(meshes.add(Cylinder::new(2.0, 0.04))),
        MeshMaterial3d(pad),
        Transform::from_xyz(0.0, 0.02, 0.0),
    ));

    let obstacle_mesh = meshes.add(Cuboid::new(1.5, 3.0, 1.5));
    for position in [
        Vec3::new(-8.0, 1.5, -8.0),
        Vec3::new(9.0, 1.5, -5.0),
        Vec3::new(-6.0, 1.5, 10.0),
    ] {
        commands.spawn((
            Mesh3d(obstacle_mesh.clone()),
            MeshMaterial3d(obstacle.clone()),
            Transform::from_translation(position),
        ));
    }
}
