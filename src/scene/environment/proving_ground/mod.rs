//! Rural drone proving ground built from Poly Haven assets.
//!
//! Selected with `BV_WORLD_PROFILE=provingGround`. A ~300 m square site: open
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
//! Asset conversion lives in `tools/blender/` (see `assets/environment/README.md`).

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
    config::{ProvingGroundConfig, Quality},
    layout::SiteLayout,
    lighting::SkyOrientation,
    terrain::TerrainMaterial,
};

pub fn is_selected(profile: &str) -> bool {
    matches!(profile, "provingGround" | "proving_ground")
}

pub struct ProvingGroundPlugin;

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
enum LoadState {
    #[default]
    Loading,
    Ready,
}

impl Plugin for ProvingGroundPlugin {
    fn build(&self, app: &mut App) {
        let config = ProvingGroundConfig::new(Quality::from_env());
        let sky = SkyOrientation::new(config.lighting.sun_azimuth_deg);
        let layout = SiteLayout::new(&config);
        app.insert_resource(config)
            .insert_resource(sky)
            .insert_resource(layout)
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .init_state::<LoadState>()
            .add_systems(Startup, start_loading)
            .add_systems(
                Update,
                (
                    lighting::configure_cameras.run_if(resource_exists::<EnvironmentAssets>),
                    wait_for_assets.run_if(in_state(LoadState::Loading)),
                ),
            )
            .add_systems(OnEnter(LoadState::Ready), spawn_world);
        if let Some(tour) = preview::PreviewTour::from_env() {
            app.insert_resource(tour)
                .add_systems(Update, preview::run_tour.run_if(in_state(LoadState::Ready)));
        }
    }
}

fn start_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    config: Res<ProvingGroundConfig>,
    sky: Res<SkyOrientation>,
) {
    commands.insert_resource(assets::load(&asset_server, config.lighting.sky_cubemap, config.lighting.ibl_cubemap));
    lighting::spawn_sun(&mut commands, &config, &sky);
    info!("proving ground: loading environment assets ({:?} quality)", config.quality);
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
    mut next: ResMut<NextState<LoadState>>,
) {
    if !env.all_loaded(&asset_server) {
        return;
    }
    let started = std::time::Instant::now();
    let library = assets::build_library(&env, &asset_server, &gltfs, &gltf_nodes, &gltf_meshes, &mut materials, &mut images);
    info!("proving ground: assets ready, textures mipmapped in {:.1?}", started.elapsed());
    commands.insert_resource(library);
    next.set(LoadState::Ready);
}

#[allow(clippy::too_many_arguments)]
fn spawn_world(
    mut commands: Commands,
    config: Res<ProvingGroundConfig>,
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
    info!("proving ground: world generated in {:.1?}", started.elapsed());
}
