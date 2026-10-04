//! Real-world terrain streamed with [bevytiles](https://github.com/ziv/bevytiles).
//!
//! Selected with `BV_WORLD_PROFILE=geoTiles`. Esri satellite imagery draped over
//! Terrarium heightmaps, streamed around the debug camera. The world origin sits
//! on the anchor coordinate and the terrain is lowered so the ground there is at
//! y = 0, matching the flat-ground convention of the other profiles.
//!
//! Environment:
//! - `BV_GEO_LAT` / `BV_GEO_LON`  anchor (degrees); falls back to `PX4_HOME_LAT` / `PX4_HOME_LON`
//! - `BV_GEO_MAX_ZOOM`            finest LOD, 15..=19 (default 18; above 15 heights are synthesized)
//! - `BV_GEO_START_AGL_M`         debug camera height above the ground once loaded (default 45.72 = 150 ft)
//! - `BV_GEO_CACHE_DIR`           tile cache (default `.cache/geo_tiles`)
//! - `BV_GEO_SCREENSHOT`          save a screenshot here once loaded, then exit

use bevy::{
    app::AppExit,
    camera::{PerspectiveProjection, Projection, visibility::NoAutoAabb},
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use bevytiles::{TerrainSet, prelude::*, store::Tile};

use crate::camera::DebugCamera;

const SKY: Color = Color::srgb(0.52, 0.72, 0.92);
const CAMERA_FAR: f32 = 200_000.0;
const DEFAULT_MAX_ZOOM: u8 = 18;
/// bevytiles skips a base tile outright when its *center* is beyond the
/// horizon (3.57 km * sqrt(camera height)). Its default z9 tiles are ~60 km
/// wide, so below a few hundred meters nothing loads; z12 tiles (~7.5 km)
/// keep the ground under the camera from a few meters up.
const BASE_ZOOM: u8 = 12;
/// Base tiles loaded around the camera, ~30 km at z12; fog hides the edge.
const STREAMING_RADIUS: i32 = 4;
const EQUATOR_CIRCUMFERENCE_M: f64 = 40_075_016.686;
const DEFAULT_START_AGL_M: f32 = 45.72;
const DEFAULT_CACHE_DIR: &str = ".cache/geo_tiles";
/// Time after the initial load before the screenshot, so the finest tiles
/// requested by the settled camera can arrive and pipelines can compile.
const SCREENSHOT_SETTLE_SECS: f32 = 6.0;

pub fn is_selected(profile: &str) -> bool {
    matches!(profile, "geoTiles" | "geo_tiles")
}

/// Anchor coordinate from the environment, for the debug camera and HUD.
#[derive(Resource, Clone, Copy, Debug)]
pub struct GeoAnchor {
    pub lat: f64,
    pub lon: f64,
}

impl GeoAnchor {
    pub fn from_env() -> Self {
        let read = |primary: &str, fallback: &str| -> f64 {
            let value = std::env::var(primary)
                .or_else(|_| std::env::var(fallback))
                .unwrap_or_else(|_| panic!("geoTiles needs {primary} or {fallback}"));
            value
                .trim()
                .parse()
                .unwrap_or_else(|error| panic!("{primary}/{fallback} = {value:?}: {error}"))
        };
        Self {
            lat: read("BV_GEO_LAT", "PX4_HOME_LAT"),
            lon: read("BV_GEO_LON", "PX4_HOME_LON"),
        }
    }
}

/// Terrain elevation (meters MSL) at the anchor, once known. Every tile is
/// shifted down by it so the anchor ground sits at y = 0.
#[derive(Resource, Default)]
struct HomeElevation(Option<f32>);

#[derive(Component)]
struct GeoHud;

#[derive(Resource)]
struct ScreenshotRequest {
    path: String,
    loaded_at: Option<f32>,
    taken: bool,
}

pub struct GeoTilesPlugin;

impl Plugin for GeoTilesPlugin {
    fn build(&self, app: &mut App) {
        let anchor = GeoAnchor::from_env();
        let max_zoom = std::env::var("BV_GEO_MAX_ZOOM")
            .ok()
            .and_then(|zoom| zoom.parse::<u8>().ok())
            .unwrap_or(DEFAULT_MAX_ZOOM)
            .clamp(15, 19);
        info!(
            "geoTiles anchored at {:.6}, {:.6} (max zoom {max_zoom})",
            anchor.lat, anchor.lon
        );

        let mut world = world_config(anchor.lat, anchor.lon, BASE_ZOOM);
        world.max_zoom = max_zoom;
        world.skirt_overlap = [1.01; ZOOM_LEVELS];
        // Put the anchor coordinate at the user-space origin, where the
        // vehicle and the debug camera expect the home position.
        let terrain_anchor = TerrainAnchor {
            world_offset: -world.origin_offset,
        };

        app.insert_resource(ClearColor(SKY))
            .insert_resource(anchor)
            .insert_resource(world)
            .insert_resource(terrain_anchor)
            .insert_resource(RenderingConfig {
                fog_color: SKY,
                fog_start: 8_000.0,
                fog_end: 26_000.0,
                ambient: Color::srgb(0.78, 0.78, 0.78),
                sun_direction: Vec3::new(0.4, 1.0, 0.3),
                ..default()
            })
            .insert_resource(streaming_config(BASE_ZOOM))
            .insert_resource(NetworkConfig {
                threads: 8,
                cache_dir: std::env::var("BV_GEO_CACHE_DIR")
                    .unwrap_or_else(|_| DEFAULT_CACHE_DIR.into())
                    .into(),
                ..default()
            })
            .init_resource::<HomeElevation>()
            .add_plugins(TerrainPlugin)
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, attach_terrain_camera.before(TerrainSet::Reconcile))
            .add_systems(
                Update,
                (keep_tile_bounds, resolve_home_elevation, lower_tiles)
                    .chain()
                    .after(TerrainSet::Status),
            )
            .add_systems(Update, update_hud.after(TerrainSet::Status));

        if let Ok(path) = std::env::var("BV_GEO_SCREENSHOT") {
            app.insert_resource(ScreenshotRequest {
                path,
                loaded_at: None,
                taken: false,
            })
            .add_systems(Update, take_screenshot_when_loaded.after(lower_tiles));
        }
    }
}

/// `WorldConfig::from_lat_lon` always anchors at bevytiles' minimum zoom (9);
/// the same web-mercator anchoring at `base_zoom`.
fn world_config(lat: f64, lon: f64, base_zoom: u8) -> WorldConfig {
    let n = 2f64.powi(i32::from(base_zoom));
    let lat_rad = lat.to_radians();
    let x = (lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    let anchor_x = x.floor() as i32;
    let anchor_z = y.floor() as i32;
    let tile_size = EQUATOR_CIRCUMFERENCE_M * lat_rad.cos() / n;
    WorldConfig {
        anchor_x,
        anchor_z,
        base_zoom,
        tile_size: tile_size as f32,
        origin_offset: Vec3::new(
            ((x - f64::from(anchor_x)) * tile_size) as f32,
            0.0,
            ((y - f64::from(anchor_z)) * tile_size) as f32,
        ),
        ..default()
    }
}

/// Default subdivision distances re-indexed from `MIN_ZOOM` to `base_zoom`
/// (`thresholds[i]` applies to zoom `base_zoom + i`), halving past the end.
fn streaming_config(base_zoom: u8) -> StreamingConfig {
    let defaults = StreamingConfig::default();
    let skip = usize::from(base_zoom - MIN_ZOOM);
    let mut thresholds = [0.0; ZOOM_LEVELS];
    for (i, threshold) in thresholds.iter_mut().enumerate() {
        *threshold = defaults
            .thresholds
            .get(i + skip)
            .copied()
            .unwrap_or_else(|| defaults.thresholds[ZOOM_LEVELS - 1] / 2f32.powi((i + skip + 1 - ZOOM_LEVELS) as i32));
    }
    StreamingConfig {
        radius: STREAMING_RADIUS,
        thresholds,
        ..defaults
    }
}

/// The debug camera drives tile streaming; widen its far plane to the horizon.
fn attach_terrain_camera(
    mut commands: Commands,
    mut cameras: Query<(Entity, &mut Projection), Added<DebugCamera>>,
) {
    for (entity, mut projection) in &mut cameras {
        if let Projection::Perspective(PerspectiveProjection { far, .. }) = projection.as_mut() {
            *far = CAMERA_FAR;
        }
        commands.entity(entity).insert(TerrainCamera);
    }
}

/// Sample the anchor's ground height once the initial tile set is resident,
/// then put the debug camera at its start height above the ground below it.
fn resolve_home_elevation(
    mut home: ResMut<HomeElevation>,
    status: Res<TerrainStatus>,
    grids: Res<HeightGrids>,
    world: Res<WorldConfig>,
    anchor: Res<TerrainAnchor>,
    mut camera: Single<&mut Transform, With<TerrainCamera>>,
) {
    if home.0.is_some() || status.loading {
        return;
    }
    let Some(height) = ground_height(&grids, &world, &anchor, Vec3::ZERO) else {
        return;
    };
    info!("geoTiles home elevation {height:.1} m MSL");
    home.0 = Some(height);

    let agl = std::env::var("BV_GEO_START_AGL_M")
        .ok()
        .and_then(|agl| agl.parse::<f32>().ok())
        .unwrap_or(DEFAULT_START_AGL_M);
    let ground = ground_height(&grids, &world, &anchor, camera.translation).unwrap_or(height);
    camera.translation.y = ground - height + agl;
}

/// bevytiles gives each tile an AABB spanning the full height column, but
/// Bevy 0.19's `calculate_bounds` replaces it with the flat grid's bounds on
/// spawn, so tiles near the camera whose displaced ground is in view get
/// frustum-culled. `NoAutoAabb` keeps the bevytiles AABB.
fn keep_tile_bounds(mut commands: Commands, tiles: Query<Entity, Added<Tile>>) {
    for entity in &tiles {
        commands.entity(entity).insert(NoAutoAabb);
    }
}

/// bevytiles spawns tiles at y = 0 and its rebase only touches x/z, so the
/// vertical offset is applied here to new tiles (and to all once it is known).
fn lower_tiles(home: Res<HomeElevation>, mut tiles: Query<(&mut Transform, Ref<Tile>)>) {
    let Some(elevation) = home.0 else {
        return;
    };
    let all = home.is_changed();
    for (mut transform, tile) in &mut tiles {
        if all || tile.is_added() {
            transform.translation.y = -elevation;
        }
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        GeoHud,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextShadow {
            offset: Vec2::splat(1.0),
            color: Color::BLACK.with_alpha(0.8),
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            top: Val::Px(10.0),
            ..default()
        },
    ));
}

fn update_hud(
    geo: Res<GeoAnchor>,
    status: Res<TerrainStatus>,
    home: Res<HomeElevation>,
    world: Res<WorldConfig>,
    anchor: Res<TerrainAnchor>,
    grids: Res<HeightGrids>,
    camera: Single<&Transform, With<TerrainCamera>>,
    mut hud: Single<&mut Text, With<GeoHud>>,
) {
    let position = camera.translation;
    let (lat, lon) = user_to_lat_lon(&world, &anchor, position);
    let elevation = home.0.unwrap_or(0.0);
    let agl = ground_height(&grids, &world, &anchor, position)
        .map(|ground| format!("{:.0} m", position.y + elevation - ground))
        .unwrap_or_else(|| "?".into());
    let loading = if status.loading {
        format!("loading {:.0}%", status.progress * 100.0)
    } else {
        "loaded".into()
    };
    hud.0 = format!(
        "geoTiles  anchor {:.5}, {:.5}  (home {:.0} m MSL)\ncamera {lat:.5}, {lon:.5}  AGL {agl}\ntiles {}  {loading}",
        geo.lat, geo.lon, elevation, status.resident,
    );
}

/// Inverse of bevytiles' web-mercator anchoring (`WorldConfig::from_lat_lon`).
fn user_to_lat_lon(world: &WorldConfig, anchor: &TerrainAnchor, user: Vec3) -> (f64, f64) {
    let absolute = user - anchor.world_offset;
    let n = f64::from(1u32 << world.base_zoom);
    let x = f64::from(world.anchor_x) + f64::from(absolute.x) / f64::from(world.tile_size);
    let y = f64::from(world.anchor_z) + f64::from(absolute.z) / f64::from(world.tile_size);
    let lon = x / n * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    (lat, lon)
}

fn take_screenshot_when_loaded(
    mut commands: Commands,
    mut request: ResMut<ScreenshotRequest>,
    home: Res<HomeElevation>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = time.elapsed_secs();
    if home.0.is_none() {
        return;
    }
    let loaded_at = *request.loaded_at.get_or_insert(now);
    if !request.taken && now - loaded_at >= SCREENSHOT_SETTLE_SECS {
        info!("geoTiles screenshot -> {}", request.path);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(request.path.clone()));
        request.taken = true;
    } else if request.taken && now - loaded_at >= SCREENSHOT_SETTLE_SECS + 2.0 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_config_matches_bevytiles_at_min_zoom() {
        let ours = world_config(39.9874, -83.0456, MIN_ZOOM);
        let theirs = WorldConfig::from_lat_lon(39.9874, -83.0456);
        assert_eq!((ours.anchor_x, ours.anchor_z), (theirs.anchor_x, theirs.anchor_z));
        assert!((ours.tile_size - theirs.tile_size).abs() < 1e-3);
        assert!(ours.origin_offset.abs_diff_eq(theirs.origin_offset, 1e-2));
    }

    #[test]
    fn thresholds_shift_to_the_base_zoom() {
        let thresholds = streaming_config(12).thresholds;
        assert_eq!(thresholds[0], 20_000.0);
        assert_eq!(thresholds[10], 20.0);
        assert_eq!(thresholds[11], 10.0);
    }

    #[test]
    fn origin_maps_back_to_the_anchor() {
        let world = world_config(39.9874, -83.0456, BASE_ZOOM);
        let anchor = TerrainAnchor {
            world_offset: -world.origin_offset,
        };
        let (lat, lon) = user_to_lat_lon(&world, &anchor, Vec3::ZERO);
        assert!((lat - 39.9874).abs() < 1e-4, "{lat}");
        assert!((lon + 83.0456).abs() < 1e-4, "{lon}");
    }

    #[test]
    fn east_is_positive_x_and_north_is_negative_z() {
        let world = world_config(39.9874, -83.0456, BASE_ZOOM);
        let anchor = TerrainAnchor {
            world_offset: -world.origin_offset,
        };
        let (_, east_lon) = user_to_lat_lon(&world, &anchor, Vec3::X * 1_000.0);
        let (north_lat, _) = user_to_lat_lon(&world, &anchor, Vec3::NEG_Z * 1_000.0);
        assert!(east_lon > -83.0456);
        assert!(north_lat > 39.9874);
    }
}

