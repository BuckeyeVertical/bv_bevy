//! Real-world terrain streamed with [bevytiles](https://github.com/ziv/bevytiles).
//!
//! `satellite_map`: Esri satellite imagery draped over
//! Terrarium heightmaps, streamed around the debug camera. The world origin sits
//! on the anchor coordinate and the terrain is lowered so the ground there is at
//! y = 0, matching the flat-ground convention of the other profiles.
//!
//! bevytiles lays tiles out with east = +X and north = -Z. Everything else here,
//! including the vehicle, uses Gazebo's East-North-Up frame mapped into Bevy
//! (`sim::gazebo_position_to_bevy`: north = -X, east = -Z). So bevytiles runs in
//! its own frame, driven by a proxy camera, and each tile is yawed into ours.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    camera::{PerspectiveProjection, Projection, visibility::NoAutoAabb},
    prelude::*,
};
use bevytiles::{TerrainSet, prelude::*, store::Tile};

use super::shared::DaylightPlugin;
use super::{DebugCameraStart, WorldLoad};
use crate::camera::DebugCamera;
use crate::geo::GeoPoint;

/// Matches the daylight sky so the horizon blends in.
const FOG: Color = Color::srgb(0.52, 0.72, 0.92);
const CAMERA_FAR: f32 = 200_000.0;
/// Finest LOD: ~117 m tiles. Heightmaps above z15 are synthesized by bevytiles.
const MAX_ZOOM: u8 = 18;
/// bevytiles skips a base tile outright when its *center* is beyond the
/// horizon (3.57 km * sqrt(camera height)). Its default z9 tiles are ~60 km
/// wide, so below a few hundred meters nothing loads; z12 tiles (~7.5 km)
/// keep the ground under the camera from a few meters up.
const BASE_ZOOM: u8 = 12;
/// Base tiles loaded around the camera, ~30 km at z12; fog hides the edge.
const STREAMING_RADIUS: i32 = 4;
const EQUATOR_CIRCUMFERENCE_M: f64 = 40_075_016.686;
/// Debug camera height above the ground once the terrain has loaded (150 ft).
const START_AGL_M: f32 = 45.72;
const CACHE_DIR: &str = ".cache/geo_tiles";
/// The map centre, shown in the HUD.
#[derive(Resource, Clone, Copy, Debug)]
struct MapCenter(GeoPoint);

/// Terrain elevation (meters MSL) at the anchor, once known. Every tile is
/// shifted down by it so the anchor ground sits at y = 0.
#[derive(Resource, Default)]
struct HomeElevation(Option<f32>);

/// Stands in for the debug camera in bevytiles' frame; bevytiles streams tiles
/// around this entity's translation.
#[derive(Component)]
struct StreamingFocus;

#[derive(Component)]
struct GeoHud;

pub struct SatelliteMapPlugin {
    pub anchor: GeoPoint,
}

impl Plugin for SatelliteMapPlugin {
    fn build(&self, app: &mut App) {
        let anchor = self.anchor;
        let mut world = world_config(anchor.latitude, anchor.longitude, BASE_ZOOM);
        world.max_zoom = MAX_ZOOM;
        world.skirt_overlap = [1.01; ZOOM_LEVELS];
        // Put the anchor coordinate at the user-space origin, where the
        // vehicle and the debug camera expect the home position.
        let terrain_anchor = TerrainAnchor {
            world_offset: -world.origin_offset,
        };

        // Just south of home (north is -X), looking north; raised to 150 ft
        // AGL once the terrain under it has loaded.
        let camera = Transform::from_xyz(120.0, START_AGL_M, 0.0).looking_at(Vec3::new(-500.0, 0.0, 0.0), Vec3::Y);
        app.insert_resource(DebugCameraStart(camera))
            .add_plugins(DaylightPlugin)
            .insert_resource(MapCenter(anchor))
            .insert_resource(world)
            .insert_resource(terrain_anchor)
            .insert_resource(RenderingConfig {
                fog_color: FOG,
                fog_start: 8_000.0,
                fog_end: 26_000.0,
                ambient: Color::srgb(0.78, 0.78, 0.78),
                sun_direction: Vec3::new(0.4, 1.0, 0.3),
                ..default()
            })
            .insert_resource(streaming_config(BASE_ZOOM))
            .insert_resource(NetworkConfig {
                threads: 8,
                cache_dir: CACHE_DIR.into(),
                ..default()
            })
            .init_resource::<HomeElevation>()
            .add_plugins(TerrainPlugin)
            .add_systems(Startup, (spawn_streaming_focus, spawn_hud))
            .add_systems(
                Update,
                (widen_debug_camera, follow_debug_camera)
                    .chain()
                    .before(TerrainSet::Reconcile),
            )
            .add_systems(
                Update,
                (keep_tile_bounds, resolve_home_elevation, place_tiles)
                    .chain()
                    .after(TerrainSet::Status),
            )
            .add_systems(Update, update_hud.after(TerrainSet::Status));
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

/// Yaw from bevytiles' frame (east = +X) into ours (east = -Z).
fn tiles_to_world() -> Quat {
    Quat::from_rotation_y(FRAC_PI_2)
}

fn world_to_tiles(position: Vec3) -> Vec3 {
    tiles_to_world().inverse() * position
}

fn spawn_streaming_focus(mut commands: Commands) {
    commands.spawn((
        Name::new("bevytiles streaming focus"),
        StreamingFocus,
        TerrainCamera,
        Transform::default(),
    ));
}

/// Widen the debug camera's far plane to the horizon.
fn widen_debug_camera(mut cameras: Query<&mut Projection, Added<DebugCamera>>) {
    for mut projection in &mut cameras {
        if let Projection::Perspective(PerspectiveProjection { far, .. }) = projection.as_mut() {
            *far = CAMERA_FAR;
        }
    }
}

/// Stream tiles around the debug camera.
fn follow_debug_camera(
    camera: Single<&Transform, (With<DebugCamera>, Without<StreamingFocus>)>,
    mut focus: Single<&mut Transform, With<StreamingFocus>>,
) {
    focus.translation = world_to_tiles(camera.translation);
}

/// Terrain height (meters MSL) under a point in our frame.
fn terrain_height(
    grids: &HeightGrids,
    world: &WorldConfig,
    anchor: &TerrainAnchor,
    position: Vec3,
) -> Option<f32> {
    ground_height(grids, world, anchor, world_to_tiles(position))
}

/// Sample the anchor's ground height once the initial tile set is resident,
/// then put the debug camera at its start height above the ground below it.
fn resolve_home_elevation(
    mut home: ResMut<HomeElevation>,
    status: Res<TerrainStatus>,
    grids: Res<HeightGrids>,
    world: Res<WorldConfig>,
    anchor: Res<TerrainAnchor>,
    mut camera: Single<&mut Transform, With<DebugCamera>>,
    mut next: ResMut<NextState<WorldLoad>>,
) {
    if home.0.is_some() || status.loading {
        return;
    }
    let Some(height) = terrain_height(&grids, &world, &anchor, Vec3::ZERO) else {
        return;
    };
    info!("satellite_map: home elevation {height:.1} m MSL");
    home.0 = Some(height);

    let ground = terrain_height(&grids, &world, &anchor, camera.translation).unwrap_or(height);
    camera.translation.y = ground - height + START_AGL_M;
    next.set(WorldLoad::Ready);
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

/// Move tiles from bevytiles' layout into our frame: yaw them, and lower them
/// so the anchor's ground is at y = 0. bevytiles only writes tile transforms
/// when a tile spawns or the anchor changes, so those are the frames to redo.
fn place_tiles(
    home: Res<HomeElevation>,
    anchor: Res<TerrainAnchor>,
    mut tiles: Query<(&mut Transform, Ref<Tile>)>,
) {
    let all = home.is_changed() || anchor.is_changed();
    let elevation = home.0.unwrap_or(0.0);
    let offset = anchor.world_offset;
    for (mut transform, tile) in &mut tiles {
        if all || tile.is_added() {
            let in_tiles = Vec3::new(
                (tile.abs_x + f64::from(offset.x)) as f32,
                -elevation,
                (tile.abs_z + f64::from(offset.z)) as f32,
            );
            transform.translation = tiles_to_world() * in_tiles;
            transform.rotation = tiles_to_world();
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
    center: Res<MapCenter>,
    status: Res<TerrainStatus>,
    home: Res<HomeElevation>,
    world: Res<WorldConfig>,
    anchor: Res<TerrainAnchor>,
    grids: Res<HeightGrids>,
    camera: Single<&Transform, With<DebugCamera>>,
    mut hud: Single<&mut Text, With<GeoHud>>,
) {
    let position = camera.translation;
    let (lat, lon) = world_to_lat_lon(&world, &anchor, position);
    let elevation = home.0.unwrap_or(0.0);
    let agl = terrain_height(&grids, &world, &anchor, position)
        .map(|ground| format!("{:.0} m", position.y + elevation - ground))
        .unwrap_or_else(|| "?".into());
    let loading = if status.loading {
        format!("loading {:.0}%", status.progress * 100.0)
    } else {
        "loaded".into()
    };
    hud.0 = format!(
        "satellite_map  centre {:.5}, {:.5}  (home {:.0} m MSL)\ncamera {lat:.5}, {lon:.5}  AGL {agl}\ntiles {}  {loading}",
        center.0.latitude, center.0.longitude, elevation, status.resident,
    );
}

/// Inverse of bevytiles' web-mercator anchoring (`WorldConfig::from_lat_lon`),
/// for a point in our frame.
fn world_to_lat_lon(world: &WorldConfig, anchor: &TerrainAnchor, position: Vec3) -> (f64, f64) {
    let absolute = world_to_tiles(position) - anchor.world_offset;
    let n = f64::from(1u32 << world.base_zoom);
    let x = f64::from(world.anchor_x) + f64::from(absolute.x) / f64::from(world.tile_size);
    let y = f64::from(world.anchor_z) + f64::from(absolute.z) / f64::from(world.tile_size);
    let lon = x / n * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * y / n)).sinh().atan().to_degrees();
    (lat, lon)
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
        let (lat, lon) = world_to_lat_lon(&world, &anchor, Vec3::ZERO);
        assert!((lat - 39.9874).abs() < 1e-4, "{lat}");
        assert!((lon + 83.0456).abs() < 1e-4, "{lon}");
    }

    #[test]
    fn terrain_matches_the_gazebo_east_north_up_frame() {
        let (lat, lon) = (39.9874, -83.0456);
        let world = world_config(lat, lon, BASE_ZOOM);
        let anchor = TerrainAnchor {
            world_offset: -world.origin_offset,
        };
        let meters_per_degree_lat = 111_000.0;
        let east = crate::sim::gazebo_position_to_bevy(Vec3::X * 1_000.0);
        let north = crate::sim::gazebo_position_to_bevy(Vec3::Y * 1_000.0);

        let (east_lat, east_lon) = world_to_lat_lon(&world, &anchor, east);
        assert!((east_lat - lat).abs() < 1e-6, "{east_lat}");
        let east_m = (east_lon - lon) * meters_per_degree_lat * lat.to_radians().cos();
        assert!((east_m - 1_000.0).abs() < 15.0, "{east_m}");

        let (north_lat, north_lon) = world_to_lat_lon(&world, &anchor, north);
        assert!((north_lon - lon).abs() < 1e-6, "{north_lon}");
        let north_m = (north_lat - lat) * meters_per_degree_lat;
        assert!((north_m - 1_000.0).abs() < 15.0, "{north_m}");
    }
}

