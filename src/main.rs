use bevy::{camera::visibility::RenderLayers, prelude::*};
use bv_bevy::camera::{CameraFrameServerPlugin, DebugCamera, DebugCameraPlugin, DroneCameraPlugin};
use bv_bevy::scene::{SimulationScenePlugin, VEHICLE_RENDER_LAYER};
use bv_bevy::sim::SimReceiverPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DebugCameraPlugin)
        .add_plugins(DroneCameraPlugin::from_env())
        .add_plugins(CameraFrameServerPlugin::from_env())
        .add_plugins(SimReceiverPlugin::from_env())
        .add_plugins(SimulationScenePlugin)
        .add_systems(Startup, spawn_debug_camera)
        .run();
}

fn spawn_debug_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        DebugCamera::default(),
        RenderLayers::from_layers(&[0, VEHICLE_RENDER_LAYER]),
        Transform::from_xyz(6.0, 5.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
