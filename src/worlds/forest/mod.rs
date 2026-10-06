//! Forest clearing built from Poly Haven assets (`cargo run -- forest`).
//!
//! A ~300 m square site: open
//! meadow with a launch pad at the origin, a pine/fir forest around it, a gravel
//! access road with a power line, and a shed with an asphalt work yard.
//!
//! Module layout:
//! - `config`     dimensions, densities, LOD distances, lighting (no magic numbers elsewhere)
//! - `layout`     the site plan as pure geometry (clearing, road, yard, ground height)
//! - `scatter`    RNG, noise and Poisson-disk sampling
//! - `assets`     GLB loading, mesh library, material tuning, mipmap generation
//! - `terrain`    splat-mapped ground mesh, asphalt yard, launch pad
//! - `vegetation` forest and ground-cover planning and spawning
//! - `structures` shed, props, barriers, power line
//! - `lighting`   HDRI sky, IBL, sun, shadows, fog, exposure
//!
//! Asset conversion lives in `tools/blender/` (see `assets/forest/README.md`).

mod assets;
mod config;
mod layout;
mod lighting;
mod preview;
mod scatter;
mod structures;
mod terrain;
mod vegetation;

use bevy::{
    gltf::{Gltf, GltfMesh, GltfNode},
    pbr::MaterialPlugin,
    prelude::*,
};

use self::{
    assets::{EnvironmentAssets, MeshLibrary},
    config::ForestConfig,
    layout::SiteLayout,
    lighting::SkyOrientation,
    terrain::TerrainMaterial,
};

pub use self::config::Quality;
use super::{DebugCameraStart, WorldLoad};

pub struct ForestPlugin {
    pub quality: Quality,
    pub shadows: bool,
    /// Screenshot a fixed set of viewpoints into this folder, then exit.
    pub tour: Option<std::path::PathBuf>,
}

impl Plugin for ForestPlugin {
    fn build(&self, app: &mut App) {
        let mut config = ForestConfig::new(self.quality);
        config.lighting.shadows = self.shadows;
        let sky = SkyOrientation::new(config.lighting.sun_azimuth_deg);
        let layout = SiteLayout::new(&config);
        // Overlooking the launch pad from above the meadow, the yard on the left.
        let camera = Transform::from_xyz(-38.0, 24.0, -46.0).looking_at(Vec3::new(10.0, 0.0, 12.0), Vec3::Y);
        app.insert_resource(DebugCameraStart(camera))
            .insert_resource(config)
            .insert_resource(sky)
            .insert_resource(layout)
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_systems(Startup, start_loading)
            .add_systems(
                Update,
                (
                    lighting::configure_cameras.run_if(resource_exists::<EnvironmentAssets>),
                    wait_for_assets.run_if(in_state(WorldLoad::Loading)),
                ),
            )
            .add_systems(OnEnter(WorldLoad::Ready), spawn_world);
        if let Some(tour) = self.tour.clone().map(preview::PreviewTour::new) {
            app.insert_resource(tour)
                .add_systems(Update, preview::run_tour.run_if(in_state(WorldLoad::Ready)));
        }
    }
}

fn start_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<ForestConfig>,
    sky: Res<SkyOrientation>,
) {
    commands.insert_resource(assets::load(&asset_server, config.lighting.sky_cubemap, config.lighting.ibl_cubemap));
    lighting::spawn_sun(&mut commands, &config, &sky);
    info!("forest: loading environment assets ({:?} quality)", config.quality);
}

fn wait_for_assets(
    mut commands: Commands,
    env: Res<EnvironmentAssets>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut next: ResMut<NextState<WorldLoad>>,
) {
    if !env.all_loaded(&asset_server) {
        return;
    }
    let started = std::time::Instant::now();
    let library = assets::build_library(&env, &asset_server, &gltfs, &gltf_nodes, &gltf_meshes, &mut materials, &mut images);
    info!("forest: assets ready, textures mipmapped in {:.1?}", started.elapsed());
    commands.insert_resource(library);
    next.set(WorldLoad::Ready);
}

#[allow(clippy::too_many_arguments)]
fn spawn_world(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<ForestConfig>,
    layout: Res<SiteLayout>,
    env: Res<EnvironmentAssets>,
    library: Res<MeshLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
) {
    let started = std::time::Instant::now();
    let plan = vegetation::plan(&config, &layout);
    terrain::spawn(&mut commands, &config, &layout, &plan, &env, &mut meshes, &mut terrain_materials, &mut materials);
    vegetation::spawn(&mut commands, &config, &plan, &library, &mut meshes);
    structures::spawn(&mut commands, &config, &layout, &library, &env, &mut meshes, &mut materials);
    structures::spawn_scan_targets(&mut commands, &config, &asset_server);
    info!("forest: world generated in {:.1?}", started.elapsed());
}
