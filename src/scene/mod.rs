mod environment;
mod vehicle;

use bevy::prelude::*;

use crate::sim::{SimEntity, VEHICLE_ID};

pub const VEHICLE_RENDER_LAYER: usize = 1;
pub const WORLD_DEBUG_RENDER_LAYER: usize = 2;

pub struct SimulationScenePlugin;

impl Plugin for SimulationScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.52, 0.72, 0.92)))
            .add_systems(Startup, setup_scene)
            .add_systems(Update, vehicle::apply_render_layer);
        if proving_ground_selected() {
            // Brings its own sky, sun, fog and exposure.
            app.add_plugins(environment::proving_ground::ProvingGroundPlugin);
        } else {
            app.add_systems(Startup, spawn_default_sun);
        }
    }
}

fn proving_ground_selected() -> bool {
    std::env::var("BV_WORLD_PROFILE").is_ok_and(|profile| environment::proving_ground::is_selected(&profile))
}

fn setup_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    environment::spawn(&mut commands, &asset_server, &mut meshes, &mut materials);

    commands
        .spawn((
            Name::new("Gazebo vehicle"),
            SimEntity::new(VEHICLE_ID),
            Transform::default(),
            Visibility::Hidden,
        ))
        .with_children(|parent| vehicle::spawn(parent, &asset_server));
}

fn spawn_default_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
