//! The drone model, drawn wherever the simulator says the vehicle is.

use bevy::{camera::visibility::RenderLayers, prelude::*};

use crate::camera::VEHICLE_RENDER_LAYER;
use crate::sim::{PoseHistory, SimEntity, SimUpdateSet, SmoothedPose, VEHICLE_ID};

const DRONE_SCALE: f32 = 0.001;
/// Horizontal centre of the CAD model (m); Gazebo's x500 origin is centred too.
const DRONE_CENTER_X: f32 = 0.626_729;
const DRONE_CENTER_Z: f32 = 0.626_644;
/// Lowest point of the CAD landing gear (m, model space).
const DRONE_FEET_Y: f32 = 0.005_946;
/// Gazebo's x500 origin sits at the bottom of its landing gear: landed on the
/// z = 0 ground plane it reports z = -0.013 m, so its feet are 1.3 cm above
/// the origin. Anchoring the CAD feet there keeps the model on the ground
/// (centring the model's bounding box instead buried its legs ~24 cm).
const GAZEBO_FEET_ABOVE_ORIGIN: f32 = 0.013;

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_vehicle)
            .add_systems(Update, apply_render_layer)
            .add_systems(Update, follow_smoothed_pose.after(SimUpdateSet::Smooth));
    }
}

/// Hidden until the simulator reports the vehicle.
fn spawn_vehicle(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Name::new("Gazebo vehicle"),
            SimEntity::new(VEHICLE_ID),
            PoseHistory::default(),
            SmoothedPose::default(),
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|parent| spawn_visual(parent, &asset_server));
}

#[derive(Component)]
struct VehicleVisual;

fn visual_offset() -> Transform {
    Transform::from_translation(Vec3::new(-DRONE_CENTER_X, GAZEBO_FEET_ABOVE_ORIGIN - DRONE_FEET_Y, -DRONE_CENTER_Z))
        .with_scale(Vec3::splat(DRONE_SCALE))
}

fn spawn_visual(parent: &mut ChildSpawnerCommands, asset_server: &AssetServer) {
    let scene =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/Drone_optimized.glb"));

    parent.spawn((
        Name::new("Buckeye Vertical drone visual"),
        VehicleVisual,
        WorldAssetRoot(scene),
        visual_offset(),
    ));
}

/// Draw the drone model at the smoothed pose while its parent keeps the exact
/// simulator pose (see `sim::smoothing`): local = exact⁻¹ · smoothed · offset.
fn follow_smoothed_pose(
    vehicles: Query<(&Transform, &SmoothedPose, &Children), Without<VehicleVisual>>,
    mut visuals: Query<&mut Transform, With<VehicleVisual>>,
) {
    for (exact, smoothed, children) in &vehicles {
        let correction = Transform::from_matrix(exact.to_matrix().inverse()).mul_transform(smoothed.0);
        for child in children.iter() {
            if let Ok(mut visual) = visuals.get_mut(child) {
                *visual = correction.mul_transform(visual_offset());
            }
        }
    }
}

fn apply_render_layer(
    mut commands: Commands,
    added_meshes: Query<(Entity, &ChildOf), Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    visual_roots: Query<(), With<VehicleVisual>>,
) {
    for (mesh, parent) in &added_meshes {
        let mut ancestor = parent.parent();

        loop {
            if visual_roots.contains(ancestor) {
                commands
                    .entity(mesh)
                    .insert(RenderLayers::layer(VEHICLE_RENDER_LAYER));
                break;
            }
            let Ok(parent) = parents.get(ancestor) else {
                break;
            };
            ancestor = parent.parent();
        }
    }
}
