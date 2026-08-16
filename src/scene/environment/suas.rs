use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::math::Affine2;
use bevy::prelude::*;

use crate::sim::gazebo_position_to_bevy;

pub(super) const WORLD_WIDTH_M: f32 = 600.0;
pub(super) const WORLD_DEPTH_M: f32 = 500.0;

const SEARCH_HALF_WIDTH_M: f32 = 125.0;
const SEARCH_HALF_DEPTH_M: f32 = 80.0;
const TARGET_BOUNDARY_MARGIN_M: f32 = 10.0;
const TREE_MODEL_HEIGHT_M: f32 = 15.831_376;
const BUSH_MODEL_HEIGHT_M: f32 = 7.117_86;
const TARGET_STEP: Vec3 = Vec3::new(17.6777, 17.6777, 0.0);
const FIRST_TARGET: Vec3 = Vec3::new(0.0, 10.0, 0.0);
const MANNEQUIN_SCAN_INDEX: usize = 1;
const TENT_SCAN_INDEX: usize = 3;
const TARGET_CLEAR_RADIUS_M: f32 = 16.0;
const GRASS_MACRO_TILE_M: f32 = 75.0;

const TREE_CLUSTERS: [Cluster; 9] = [
    Cluster::new(-225.0, -175.0, 38.0, 28.0, 5),
    Cluster::new(-230.0, 80.0, 42.0, 34.0, 5),
    Cluster::new(-150.0, 185.0, 45.0, 28.0, 5),
    Cluster::new(-80.0, -40.0, 28.0, 22.0, 4),
    Cluster::new(90.0, 45.0, 32.0, 24.0, 4),
    Cluster::new(175.0, -150.0, 45.0, 32.0, 5),
    Cluster::new(235.0, 60.0, 38.0, 34.0, 5),
    Cluster::new(185.0, 195.0, 42.0, 26.0, 4),
    Cluster::new(30.0, 195.0, 36.0, 24.0, 3),
];

const BUSH_CLUSTERS: [Cluster; 9] = [
    Cluster::new(-225.0, -175.0, 52.0, 38.0, 10),
    Cluster::new(-230.0, 80.0, 54.0, 44.0, 9),
    Cluster::new(-150.0, 185.0, 58.0, 36.0, 9),
    Cluster::new(-80.0, -40.0, 40.0, 30.0, 9),
    Cluster::new(90.0, 45.0, 44.0, 32.0, 9),
    Cluster::new(175.0, -150.0, 58.0, 42.0, 9),
    Cluster::new(235.0, 60.0, 46.0, 44.0, 9),
    Cluster::new(185.0, 195.0, 54.0, 34.0, 8),
    Cluster::new(30.0, 195.0, 48.0, 32.0, 8),
];

#[derive(Clone, Copy)]
struct Cluster {
    center: Vec2,
    radius: Vec2,
    count: usize,
}

impl Cluster {
    const fn new(x: f32, z: f32, radius_x: f32, radius_z: f32, count: usize) -> Self {
        Self {
            center: Vec2::new(x, z),
            radius: Vec2::new(radius_x, radius_z),
            count,
        }
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    spawn_ground(commands, asset_server, meshes, materials);
    spawn_ground_variation(commands, asset_server, meshes, materials);
    spawn_access_track(commands, meshes, materials);
    spawn_vegetation(commands, asset_server);
    spawn_targets(commands, asset_server);
}

fn spawn_ground(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let grass = super::load_repeating_texture(asset_server, "textures/grass004/color.jpg", true);
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.82, 0.72),
        base_color_texture: Some(grass),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        uv_transform: Affine2::from_scale(Vec2::new(
            WORLD_WIDTH_M / GRASS_MACRO_TILE_M,
            WORLD_DEPTH_M / GRASS_MACRO_TILE_M,
        )),
        ..default()
    });
    commands.spawn((
        Name::new("SUAS ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_WIDTH_M, WORLD_DEPTH_M))),
        MeshMaterial3d(material),
    ));
}

fn spawn_ground_variation(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let patch_mesh = meshes.add(Cylinder::new(1.0, 0.012));
    let withered_grass =
        super::load_repeating_texture(asset_server, "textures/withered_grass/color.jpg", true);
    let dry_grass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.56, 0.62, 0.42, 0.48),
        base_color_texture: Some(withered_grass),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        alpha_mode: AlphaMode::Blend,
        uv_transform: Affine2::from_scale(Vec2::splat(1.5)),
        ..default()
    });
    let dirt = materials.add(StandardMaterial {
        base_color: Color::srgba(0.29, 0.22, 0.13, 0.72),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let mut rng = FixedRng::new(0x5355_4153);

    for index in 0..22 {
        let position = open_ground_position(&mut rng);
        spawn_patch(
            commands,
            &patch_mesh,
            &dry_grass,
            format!("Dry grass patch {index}"),
            position,
            Vec2::new(rng.range(10.0, 28.0), rng.range(7.0, 18.0)),
            rng.range(0.0, TAU),
        );
    }

    for index in 0..14 {
        let position = open_ground_position(&mut rng);
        spawn_patch(
            commands,
            &patch_mesh,
            &dirt,
            format!("Dirt patch {index}"),
            position,
            Vec2::new(rng.range(4.0, 14.0), rng.range(3.0, 9.0)),
            rng.range(0.0, TAU),
        );
    }
}

fn spawn_patch(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    name: String,
    position: Vec2,
    radius: Vec2,
    heading: f32,
) {
    commands.spawn((
        Name::new(name),
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_xyz(position.x, 0.006, position.y)
            .with_rotation(Quat::from_rotation_y(heading))
            .with_scale(Vec3::new(radius.x, 1.0, radius.y)),
    ));
}

fn spawn_access_track(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let road_mesh = meshes.add(Cuboid::new(1.0, 0.012, 1.0));
    let worn_grass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.31, 0.18),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });
    let tire_dirt = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.17, 0.1),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.0,
        ..default()
    });

    for index in 0..18 {
        let x_start = -270.0 + index as f32 * 30.0;
        let x_end = x_start + 31.0;
        let start = Vec2::new(x_start, track_z(x_start));
        let end = Vec2::new(x_end, track_z(x_end));
        let center = (start + end) * 0.5;
        let direction = (end - start).normalize();
        let length = start.distance(end);
        let heading = -direction.y.atan2(direction.x);
        let side = Vec2::new(-direction.y, direction.x);

        spawn_track_segment(
            commands,
            &road_mesh,
            &worn_grass,
            format!("Access road {index}"),
            center,
            Vec3::new(length, 0.007, 4.8),
            heading,
        );
        for (track_index, offset) in [-1.25, 1.25].into_iter().enumerate() {
            spawn_track_segment(
                commands,
                &road_mesh,
                &tire_dirt,
                format!("Vehicle track {index}-{track_index}"),
                center + side * offset,
                Vec3::new(length, 0.014, 0.55),
                heading,
            );
        }
    }
}

fn spawn_track_segment(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    name: String,
    center: Vec2,
    size: Vec3,
    heading: f32,
) {
    commands.spawn((
        Name::new(name),
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_xyz(center.x, size.y * 0.5, center.y)
            .with_rotation(Quat::from_rotation_y(heading))
            .with_scale(Vec3::new(size.x, size.y / 0.012, size.z)),
    ));
}

fn spawn_vegetation(commands: &mut Commands, asset_server: &AssetServer) {
    let tree = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/tree1.glb"));
    let bush = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/bush_01.glb"));
    let targets = target_positions();
    let mut rng = FixedRng::new(0x4256_5754);
    let mut tree_index = 0;

    for cluster in TREE_CLUSTERS {
        for _ in 0..cluster.count {
            let position = cluster_position(cluster, &targets, &mut rng);
            let category_height = [5.0, 10.0, 15.0][tree_index % 3];
            let scale = category_height * rng.range(0.8, 1.2) / TREE_MODEL_HEIGHT_M;
            commands.spawn((
                Name::new(format!("Tree {tree_index}")),
                WorldAssetRoot(tree.clone()),
                Transform::from_xyz(position.x, 0.0, position.y)
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, TAU)))
                    .with_scale(Vec3::splat(scale)),
            ));
            tree_index += 1;
        }
    }

    let mut bush_index = 0;
    for cluster in BUSH_CLUSTERS {
        for _ in 0..cluster.count {
            let position = cluster_position(cluster, &targets, &mut rng);
            let height = rng.range(0.8, 2.3);
            let scale = height / BUSH_MODEL_HEIGHT_M;
            commands.spawn((
                Name::new(format!("Scrub {bush_index}")),
                WorldAssetRoot(bush.clone()),
                Transform::from_xyz(position.x, 0.0, position.y)
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, TAU)))
                    .with_scale(Vec3::splat(scale)),
            ));
            bush_index += 1;
        }
    }
}

fn spawn_targets(commands: &mut Commands, asset_server: &AssetServer) {
    let [mannequin_position, tent_position] = target_positions();
    debug_assert!(target_inside_search(mannequin_position));
    debug_assert!(target_inside_search(tent_position));
    let mannequin = asset_server
        .load(GltfAssetLabel::Scene(0).from_asset("models/polo_shirt_mannequin_optimized.glb"));
    let tent = asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/Tent_optimized.glb"));

    commands.spawn((
        Name::new("SUAS target: mannequin"),
        WorldAssetRoot(mannequin),
        Transform::from_xyz(mannequin_position.x, 0.19, mannequin_position.y)
            .with_rotation(Quat::from_rotation_y(0.45) * Quat::from_rotation_x(-FRAC_PI_2))
            .with_scale(Vec3::splat(1.75 / 1.928_569)),
    ));
    commands.spawn((
        Name::new("SUAS target: tent"),
        WorldAssetRoot(tent),
        Transform::from_xyz(tent_position.x, 0.0, tent_position.y)
            .with_rotation(Quat::from_rotation_y(1.35))
            .with_scale(Vec3::new(1.0, 0.65, 0.8)),
    ));
}

fn target_positions() -> [Vec2; 2] {
    [MANNEQUIN_SCAN_INDEX, TENT_SCAN_INDEX].map(|index| {
        let position = gazebo_position_to_bevy(FIRST_TARGET + TARGET_STEP * index as f32);
        Vec2::new(position.x, position.z)
    })
}

fn target_inside_search(position: Vec2) -> bool {
    position.x.abs() <= SEARCH_HALF_WIDTH_M - TARGET_BOUNDARY_MARGIN_M
        && position.y.abs() <= SEARCH_HALF_DEPTH_M - TARGET_BOUNDARY_MARGIN_M
}

fn cluster_position(cluster: Cluster, targets: &[Vec2; 2], rng: &mut FixedRng) -> Vec2 {
    for _ in 0..24 {
        let position = ellipse_position(cluster.center, cluster.radius, rng);
        if within_world(position, 6.0)
            && targets
                .iter()
                .all(|target| position.distance(*target) >= TARGET_CLEAR_RADIUS_M)
        {
            return position;
        }
    }
    cluster.center
}

fn open_ground_position(rng: &mut FixedRng) -> Vec2 {
    Vec2::new(rng.range(-285.0, 285.0), rng.range(-235.0, 235.0))
}

fn ellipse_position(center: Vec2, radius: Vec2, rng: &mut FixedRng) -> Vec2 {
    let angle = rng.range(0.0, TAU);
    let distance = rng.unit().sqrt();
    center + Vec2::new(angle.cos(), angle.sin()) * radius * distance
}

fn within_world(position: Vec2, margin: f32) -> bool {
    position.x.abs() <= WORLD_WIDTH_M * 0.5 - margin
        && position.y.abs() <= WORLD_DEPTH_M * 0.5 - margin
}

fn track_z(x: f32) -> f32 {
    130.0 + 8.0 * (x / 85.0).sin()
}

struct FixedRng(u64);

impl FixedRng {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 as u32) as f32 / u32::MAX as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_search_dimensions_match_the_suas_layout() {
        assert_eq!(WORLD_WIDTH_M * WORLD_DEPTH_M, 300_000.0);
        assert_eq!(SEARCH_HALF_WIDTH_M * 2.0, 250.0);
        assert_eq!(SEARCH_HALF_DEPTH_M * 2.0, 160.0);
    }

    #[test]
    fn targets_are_inside_the_search_area_with_margin() {
        for position in target_positions() {
            assert!(target_inside_search(position));
        }
    }

    #[test]
    fn targets_are_independent_and_on_the_existing_scan_line() {
        let [mannequin, tent] = target_positions();

        assert!(mannequin.distance(tent) > 40.0);
        assert_eq!(MANNEQUIN_SCAN_INDEX, 1);
        assert_eq!(TENT_SCAN_INDEX, 3);
    }

    #[test]
    fn vegetation_counts_stay_sparse() {
        let trees: usize = TREE_CLUSTERS.iter().map(|cluster| cluster.count).sum();
        let bushes: usize = BUSH_CLUSTERS.iter().map(|cluster| cluster.count).sum();

        assert!((30..=60).contains(&trees));
        assert!((50..=150).contains(&bushes));
    }
}
