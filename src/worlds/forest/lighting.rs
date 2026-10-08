//! Daylight from the Meadow 2 HDRI: skybox, image-based lighting, a sun aligned
//! with the HDRI's sun, cascaded shadows, distance fog and exposure.
//!
//! The camera-side components are attached to every 3D camera, including the
//! simulated onboard camera, so the vision stream sees the same world as the
//! desktop view.

use bevy::{
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    light::{
        CascadeShadowConfigBuilder, DirectionalLightShadowMap, GeneratedEnvironmentMapLight, GlobalAmbientLight,
        Skybox,
    },
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
    render::{render_resource::TextureFormat, renderer::RenderAdapter},
};
use serde::Deserialize;

use super::{assets::EnvironmentAssets, config::ForestConfig, layout::compass_direction};

/// Metadata written by tools/blender/hdri_to_cubemap.py.
const SKY_META: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/forest/sky/meadow_2.json"));

#[derive(Deserialize)]
struct SkyMeta {
    sun_direction: [f32; 3],
}

/// Orientation of the HDRI in the world and the resulting sun direction.
#[derive(Resource, Clone, Copy)]
pub struct SkyOrientation {
    /// Rotation applied to the cubemap (skybox and IBL use the same convention).
    pub rotation: Quat,
    /// Unit vector pointing towards the sun.
    pub sun_direction: Vec3,
}

impl SkyOrientation {
    pub fn new(sun_azimuth_deg: f32) -> Self {
        let meta: SkyMeta = serde_json::from_str(SKY_META).expect("valid sky metadata");
        let hdri_sun = Vec3::from_array(meta.sun_direction).normalize();
        let hdri_horizontal = Vec3::new(hdri_sun.x, 0.0, hdri_sun.z).normalize();
        let rotation = Quat::from_rotation_arc(hdri_horizontal, compass_direction(sun_azimuth_deg));
        Self { rotation, sun_direction: rotation * hdri_sun }
    }
}

pub fn spawn_sun(commands: &mut Commands, config: &ForestConfig, sky: &SkyOrientation) {
    let lighting = &config.lighting;
    commands.insert_resource(DirectionalLightShadowMap { size: lighting.shadow_map_size });
    // The IBL provides all ambient light; a flat ambient term would wash out shadows.
    commands.insert_resource(GlobalAmbientLight { brightness: 0.0, ..default() });
    commands.insert_resource(ClearColor(Color::srgb(0.62, 0.72, 0.84)));

    let cascades = CascadeShadowConfigBuilder {
        num_cascades: lighting.shadow_cascades,
        minimum_distance: 0.1,
        first_cascade_far_bound: 14.0,
        maximum_distance: lighting.shadow_distance,
        overlap_proportion: 0.2,
    }
    .build();

    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            // Late-afternoon sun at ~27 deg elevation is slightly warm.
            color: Color::srgb(1.0, 0.95, 0.88),
            illuminance: lighting.sun_illuminance_lux,
            shadow_maps_enabled: lighting.shadows,
            shadow_depth_bias: 0.04,
            shadow_normal_bias: 1.4,
            ..default()
        },
        cascades,
        Transform::default().looking_to(-sky.sun_direction, Vec3::Y),
    ));
}

/// Give every 3D camera the sky, IBL, fog and exposure as it appears.
pub fn configure_cameras(
    mut commands: Commands,
    cameras: Query<Entity, Added<Camera3d>>,
    config: Res<ForestConfig>,
    env: Res<EnvironmentAssets>,
    sky: Res<SkyOrientation>,
    adapter: Res<RenderAdapter>,
) {
    let lighting = &config.lighting;
    let msaa = supported_msaa(&adapter, lighting.msaa_samples);
    for camera in &cameras {
        commands.entity(camera).insert((
            Hdr,
            // No DepthPrepass: with alpha-tested foliage and LOD cross-fade
            // dithering it produced white speckles, for only ~10% speed-up.
            msaa,
            Tonemapping::TonyMcMapface,
            Exposure { ev100: lighting.exposure_ev100 },
            Skybox { image: Some(env.sky.clone()), brightness: lighting.sky_brightness, rotation: sky.rotation },
            GeneratedEnvironmentMapLight {
                environment_map: env.ibl.clone(),
                intensity: lighting.ibl_intensity,
                rotation: sky.rotation,
                ..default()
            },
            DistanceFog {
                // Light aerial perspective: keeps the far tree line soft without
                // hurting visibility across the 300 m site.
                color: Color::srgb(0.74, 0.8, 0.88),
                directional_light_color: Color::srgba(1.0, 0.93, 0.78, 0.35),
                directional_light_exponent: 24.0,
                falloff: FogFalloff::from_visibility_squared(lighting.fog_visibility),
            },
        ));
    }
}

/// The requested MSAA level if the adapter can do it for the HDR colour and
/// depth targets, else 4x (guaranteed by WebGPU). Software rasterisers such as
/// llvmpipe (WSL without GPU passthrough) reject 2x and abort on the first frame.
fn supported_msaa(adapter: &RenderAdapter, requested: u32) -> Msaa {
    let supported = |count| {
        [TextureFormat::Rgba16Float, TextureFormat::Depth32Float]
            .into_iter()
            .all(|format| adapter.get_texture_format_features(format).flags.sample_count_supported(count))
    };
    if requested == 2 && supported(2) {
        Msaa::Sample2
    } else {
        if requested == 2 {
            warn!("forest: adapter does not support 2x MSAA, using 4x");
        }
        Msaa::Sample4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_lands_on_requested_azimuth_and_keeps_hdri_elevation() {
        let meta: SkyMeta = serde_json::from_str(SKY_META).unwrap();
        let elevation = Vec3::from_array(meta.sun_direction).normalize().y;
        let sky = SkyOrientation::new(222.0);
        assert!((sky.sun_direction.y - elevation).abs() < 1e-4);
        let horizontal = Vec3::new(sky.sun_direction.x, 0.0, sky.sun_direction.z).normalize();
        assert!(horizontal.dot(compass_direction(222.0)) > 0.9999);
    }
}
