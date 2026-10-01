//! Tunable dimensions, densities, and quality settings for the proving ground.
//!
//! Everything that shapes the world lives here so placement code stays free of
//! magic numbers. Distances are metres (1 Bevy unit = 1 m). Bevy's frame follows
//! the Gazebo bridge: north = -X, east = -Z, up = +Y.

use std::ops::Range;

use bevy::prelude::*;

/// Rendering quality preset, chosen with `BV_ENV_QUALITY=low|medium|high`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub fn from_env() -> Self {
        match std::env::var("BV_ENV_QUALITY").as_deref() {
            Ok("low") => Self::Low,
            Ok("high") => Self::High,
            _ => Self::Medium,
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ProvingGroundConfig {
    pub seed: u64,
    pub quality: Quality,
    pub site: SiteConfig,
    pub terrain: TerrainConfig,
    pub forest: ForestConfig,
    pub ground_cover: GroundCoverConfig,
    pub lod: LodConfig,
    pub lighting: LightingConfig,
}

#[derive(Clone, Debug)]
pub struct SiteConfig {
    /// Half the side of the square playable area (300 m square).
    pub half_extent: f32,
    /// Centre and radii of the open clearing; the tree line wobbles around this ellipse.
    pub clearing_center: Vec2,
    pub clearing_radii: Vec2,
    /// Peak-to-peak irregularity of the tree line.
    pub clearing_edge_noise: f32,
    /// Width of the sparse-tree band between meadow and dense forest.
    pub transition_width: f32,
    /// Launch/landing pad at the origin (where the vehicle spawns).
    pub launch_pad_size: f32,
    /// Radius of the mown area around the pad that stays free of anything tall.
    pub launch_clear_radius: f32,
    /// Gravel road control points (XZ), from the site edge to the work yard.
    pub road_path: Vec<Vec2>,
    pub road_width: f32,
    /// Asphalt work yard (centre XZ, half size, heading in radians about +Y).
    pub yard_center: Vec2,
    pub yard_half_size: Vec2,
    pub yard_heading: f32,
    /// Spacing of the electricity poles along the road.
    pub pole_spacing: f32,
    /// Lateral offset of the pole line from the road centre.
    pub pole_offset: f32,
    /// Poly Haven poles are authored ~6.1 m; rural distribution poles are ~8-9 m.
    pub pole_scale: f32,
    /// Matches bv_core config/proving_ground_params.yaml (scan box 5-45 m N, 0-45 m E).
    pub scan_targets: Vec<ScanTarget>,
}

/// A vision target for the scan phase (same models as the missionTest world).
#[derive(Clone, Debug)]
pub struct ScanTarget {
    pub name: &'static str,
    pub asset: &'static str,
    /// Offset from the launch pad / PX4 home, metres north and east.
    pub north: f32,
    pub east: f32,
    pub heading: f32,
    /// Rotation about X (the mannequin model is authored standing up).
    pub tilt: f32,
    pub ground_clearance: f32,
}

#[derive(Clone, Debug)]
pub struct TerrainConfig {
    /// Half extent of the rendered ground (reaches well past the fogged horizon).
    pub half_extent: f32,
    /// Grid spacing inside the site; it grows geometrically outside it.
    pub inner_spacing: f32,
    pub outer_growth: f32,
    /// Max rolling-hill amplitude outside the flat flight area.
    pub hill_amplitude: f32,
    /// Distance outside the clearing over which the ground stays exactly flat
    /// (Gazebo physics uses a flat plane, so the flight area must match it).
    pub flat_margin: f32,
}

#[derive(Clone, Debug)]
pub struct ForestConfig {
    /// Mean spacing of trees in dense forest (Poisson-disk radius).
    pub dense_spacing: f32,
    /// Spacing in the transition band (sparser, more saplings).
    pub transition_spacing: f32,
    /// Spacing of the far forest ring outside the site (impostor-heavy).
    pub far_spacing: f32,
    /// The forest continues out to this half extent so the horizon reads as forest.
    pub far_half_extent: f32,
    /// Moderate size variation applied on top of the 17 source tree variants.
    pub scale_range: Range<f32>,
    /// Share of pines vs firs (rest are firs).
    pub pine_fraction: f32,
}

#[derive(Clone, Debug)]
pub struct GroundCoverConfig {
    pub fern_clusters: usize,
    pub rock_clusters: usize,
    pub branch_count: usize,
    pub stump_count: usize,
    pub log_count: usize,
    pub grass_clusters: usize,
    pub meadow_grass_clusters: usize,
    pub boulders: usize,
}

/// Camera distances (m) where each LOD hands over to the next.
#[derive(Clone, Debug)]
pub struct LodConfig {
    pub tree: [f32; 3],
    pub crossfade: f32,
    pub small_prop_cull: f32,
    pub ground_cover_cull: f32,
    pub grass_cull: f32,
    /// How far outside the site the drone may plausibly fly; LODs that can never
    /// be seen from there are not spawned.
    pub flight_margin: f32,
}

#[derive(Clone, Debug)]
pub struct LightingConfig {
    pub sky_cubemap: &'static str,
    /// Small copy for image-based lighting (Bevy filters it every frame).
    pub ibl_cubemap: &'static str,
    /// Compass azimuth the sun should come from (deg clockwise from north).
    /// The HDRI is rotated so its sun lands here.
    pub sun_azimuth_deg: f32,
    pub sun_illuminance_lux: f32,
    /// Skybox / IBL luminance scale (cd/m^2 per HDRI unit).
    pub sky_brightness: f32,
    pub ibl_intensity: f32,
    pub exposure_ev100: f32,
    pub fog_visibility: f32,
    /// `BV_ENV_SHADOWS=0` turns sun shadows off for very weak GPUs.
    pub shadows: bool,
    pub shadow_distance: f32,
    pub shadow_cascades: usize,
    pub shadow_map_size: usize,
    /// MSAA sample count (foliage edges vs. fill-rate on weak GPUs).
    pub msaa_samples: u32,
}

impl ProvingGroundConfig {
    pub fn new(quality: Quality) -> Self {
        let (lod, shadow_distance, shadow_cascades, shadow_map_size) = match quality {
            Quality::Low => ([35.0, 85.0, 150.0], 100.0, 2, 1024),
            Quality::Medium => ([45.0, 110.0, 190.0], 150.0, 3, 2048),
            Quality::High => ([60.0, 140.0, 240.0], 200.0, 4, 4096),
        };
        // MSAA off rendered nothing at all with this camera setup (HDR + skybox +
        // generated IBL) in Bevy 0.19 on Metal, so 2x is the floor.
        let msaa_samples = match quality {
            Quality::Low | Quality::Medium => 2,
            Quality::High => 4,
        };
        let density = match quality {
            Quality::Low => 0.6,
            Quality::Medium => 1.0,
            Quality::High => 1.3,
        };
        let count = |n: usize| ((n as f32) * density).round() as usize;

        Self {
            seed: 0x5052_4f56_4752_4e44,
            quality,
            site: SiteConfig {
                half_extent: 150.0,
                clearing_center: Vec2::new(4.0, -6.0),
                clearing_radii: Vec2::new(96.0, 88.0),
                clearing_edge_noise: 18.0,
                transition_width: 22.0,
                launch_pad_size: 8.0,
                launch_clear_radius: 22.0,
                // Enters from the south-west edge, bends through the forest and
                // ends at the work yard on the west side of the clearing.
                road_path: vec![
                    Vec2::new(205.0, 128.0),
                    Vec2::new(160.0, 112.0),
                    Vec2::new(128.0, 104.0),
                    Vec2::new(102.0, 88.0),
                    Vec2::new(84.0, 64.0),
                    Vec2::new(66.0, 53.0),
                    Vec2::new(49.0, 51.5),
                ],
                road_width: 4.6,
                yard_center: Vec2::new(36.0, 58.0),
                yard_half_size: Vec2::new(13.0, 9.0),
                // Long side faces the clearing centre; the road meets the +x end.
                yard_heading: 0.46,
                pole_spacing: 34.0,
                pole_offset: 6.5,
                pole_scale: 1.38,
                scan_targets: vec![
                    ScanTarget {
                        name: "mannequin",
                        asset: "models/polo_shirt_mannequin_optimized.glb",
                        north: 18.0,
                        east: 14.0,
                        heading: 0.45,
                        tilt: std::f32::consts::FRAC_PI_2,
                        ground_clearance: 0.17,
                    },
                    ScanTarget {
                        name: "tent",
                        asset: "models/Tent_optimized.glb",
                        north: 36.0,
                        east: 34.0,
                        heading: 1.35,
                        tilt: 0.0,
                        ground_clearance: 0.0,
                    },
                ],
            },
            terrain: TerrainConfig {
                half_extent: 900.0,
                inner_spacing: 1.0,
                outer_growth: 1.08,
                hill_amplitude: 2.6,
                flat_margin: 10.0,
            },
            forest: ForestConfig {
                dense_spacing: 4.7,
                transition_spacing: 13.0,
                far_spacing: 6.5,
                far_half_extent: 470.0,
                scale_range: 0.82..1.12,
                pine_fraction: 0.55,
            },
            ground_cover: GroundCoverConfig {
                fern_clusters: count(170),
                rock_clusters: count(60),
                branch_count: count(320),
                stump_count: count(34),
                log_count: count(26),
                grass_clusters: count(260),
                meadow_grass_clusters: count(140),
                boulders: 14,
            },
            lod: LodConfig {
                tree: lod,
                crossfade: 6.0,
                small_prop_cull: 70.0,
                ground_cover_cull: 140.0,
                grass_cull: 45.0,
                flight_margin: 40.0,
            },
            lighting: LightingConfig {
                sky_cubemap: "environment/sky/meadow_2_cubemap.ktx2",
                ibl_cubemap: "environment/sky/meadow_2_ibl.ktx2",
                sun_azimuth_deg: 222.0,
                sun_illuminance_lux: 82_000.0,
                sky_brightness: 10_000.0,
                ibl_intensity: 5_200.0,
                exposure_ev100: 14.4,
                fog_visibility: 2_600.0,
                shadows: std::env::var("BV_ENV_SHADOWS").map_or(true, |v| v != "0"),
                shadow_distance,
                shadow_cascades,
                shadow_map_size,
                msaa_samples,
            },
        }
    }
}
