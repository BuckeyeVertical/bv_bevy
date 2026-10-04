//! Pieces used by more than one world.

use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::Affine2;
use bevy::prelude::*;

const GRASS_TILE_SIZE_METERS: f32 = 2.0;
const SKY: Color = Color::srgb(0.52, 0.72, 0.92);

/// Plain daylight: a blue sky and one shadow-casting sun.
pub struct DaylightPlugin;

impl Plugin for DaylightPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY))
            .add_systems(Startup, spawn_sun);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

pub fn spawn_ground(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    size: f32,
) {
    spawn_ground_rect(commands, asset_server, meshes, materials, Vec2::splat(size));
}

pub fn spawn_ground_rect(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    dimensions: Vec2,
) {
    let base_color = load_repeating_texture(asset_server, "textures/grass004/color.jpg", true);
    let normal = load_repeating_texture(asset_server, "textures/grass004/normal_gl.jpg", false);
    let roughness = load_repeating_texture(asset_server, "textures/grass004/roughness.jpg", false);
    let ambient_occlusion = load_repeating_texture(
        asset_server,
        "textures/grass004/ambient_occlusion.jpg",
        false,
    );
    let ground = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(base_color),
        normal_map_texture: Some(normal),
        metallic_roughness_texture: Some(roughness),
        occlusion_texture: Some(ambient_occlusion),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.3,
        uv_transform: Affine2::from_scale(dimensions / GRASS_TILE_SIZE_METERS),
        ..default()
    });
    let ground_mesh = Plane3d::default()
        .mesh()
        .size(dimensions.x, dimensions.y)
        .build()
        .with_generated_tangents()
        .expect("the ground plane has valid positions, normals, and UVs");
    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(ground_mesh)),
        MeshMaterial3d(ground),
    ));
}

pub fn load_repeating_texture(
    asset_server: &AssetServer,
    path: &'static str,
    is_srgb: bool,
) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = is_srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                ..default()
            });
        })
        .load(path)
}
