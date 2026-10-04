//! Forest and ground-cover placement.
//!
//! Placement is planned first (pure data, deterministic from the seed) and
//! spawned afterwards, so the terrain can darken the ground under the canopy.
//! Trees use Poisson-disk spacing thinned by the forest field; everything else is
//! scattered in clusters rather than uniformly.

use std::collections::HashMap;

use bevy::{
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
};
use serde::Deserialize;

use super::{
    assets::MeshLibrary,
    config::ForestConfig,
    layout::SiteLayout,
    scatter::{fbm, poisson_disk, smoothstep, value_noise, Rng, SpatialGrid},
};

const TREES_META: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/forest/vegetation/trees.json"));

#[derive(Deserialize)]
struct TreesMeta {
    trees: HashMap<String, TreeMeta>,
}

#[derive(Deserialize)]
struct TreeMeta {
    species: String,
    height: f32,
    radius: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Species {
    Pine,
    Fir,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SizeClass {
    Large,
    Medium,
}

#[derive(Clone, Debug)]
pub struct TreeVariant {
    pub name: String,
    pub species: Species,
    pub size: SizeClass,
    /// Authored height (m); checked by the real-world-scale test.
    #[cfg_attr(not(test), allow(dead_code))]
    pub height: f32,
    pub radius: f32,
}

pub fn tree_variants() -> Vec<TreeVariant> {
    let meta: TreesMeta = serde_json::from_str(TREES_META).expect("valid trees.json");
    let mut variants: Vec<TreeVariant> = meta
        .trees
        .into_iter()
        .map(|(name, m)| TreeVariant {
            species: if m.species == "pine" { Species::Pine } else { Species::Fir },
            size: if name.contains("sapling") { SizeClass::Medium } else { SizeClass::Large },
            height: m.height,
            radius: m.radius,
            name,
        })
        .collect();
    variants.sort_by(|a, b| a.name.cmp(&b.name));
    variants
}

#[derive(Clone, Debug)]
pub struct TreeInstance {
    pub variant: usize,
    pub position: Vec3,
    pub yaw: f32,
    pub scale: f32,
    /// Closest the camera can get (see SiteLayout::min_view_distance).
    pub min_view: f32,
}

/// A small object with a LOD chain, e.g. "fern_b" -> fern_b_lod0, fern_b_lod1.
#[derive(Clone, Debug)]
pub struct PropInstance {
    pub mesh: String,
    pub lods: u8,
    /// Distance where lod0 hands over to lod1 (ignored for single-LOD props).
    pub switch: f32,
    pub cull: f32,
    pub transform: Transform,
    pub shadows: bool,
}

pub struct VegetationPlan {
    pub variants: Vec<TreeVariant>,
    pub trees: Vec<TreeInstance>,
    pub props: Vec<PropInstance>,
    canopy: SpatialGrid,
}

impl VegetationPlan {
    /// 1 in the open, down to ~0.35 under dense canopy.
    pub fn canopy_openness(&self, p: Vec2) -> f32 {
        let shade = self.canopy.accumulate(p, 9.0, |d, r| 1.0 - smoothstep(r * 0.25, r * 1.25, d));
        1.0 - (shade * 0.32).min(0.65)
    }
}

// --------------------------------------------------------------------------- planning

pub fn plan(config: &ForestConfig, layout: &SiteLayout) -> VegetationPlan {
    let mut rng = Rng::new(config.seed);
    let variants = tree_variants();
    let mut trees = plan_trees(config, layout, &variants, &mut rng.fork(1));
    trees.extend(plan_edge_trees(layout, &variants, &mut rng.fork(2)));

    let mut canopy = SpatialGrid::new(6.0);
    let mut trunks = SpatialGrid::new(4.0);
    for t in &trees {
        let v = &variants[t.variant];
        let p = Vec2::new(t.position.x, t.position.z);
        canopy.insert(p, v.radius * t.scale * 0.85);
        trunks.insert(p, 0.6 * t.scale);
    }

    let mut props = Vec::new();
    let mut prng = rng.fork(3);
    plan_ground_cover(config, layout, &trunks, &mut props, &mut prng);

    VegetationPlan { variants, trees, props, canopy }
}

fn pick_variant(variants: &[TreeVariant], species: Species, size: SizeClass, rng: &mut Rng) -> usize {
    let candidates: Vec<usize> = variants
        .iter()
        .enumerate()
        .filter(|(_, v)| v.species == species && v.size == size)
        .map(|(i, _)| i)
        .collect();
    *rng.pick(&candidates)
}

fn plan_trees(config: &ForestConfig, layout: &SiteLayout, variants: &[TreeVariant], rng: &mut Rng) -> Vec<TreeInstance> {
    let forest = &config.forest;
    let extent = Vec2::splat(forest.far_half_extent);
    let candidates = poisson_disk(-extent, extent, forest.dense_spacing * 0.92, rng);
    let seed = config.seed as u32;
    let mut trees = Vec::with_capacity(candidates.len());

    for p in candidates {
        let edge = layout.clearing_distance(p);
        if edge < -2.0 {
            continue;
        }
        let f = layout.forest_factor(p);
        // Clumped, irregular forest edge: some stands push into the transition band.
        let clump = smoothstep(0.35, 0.68, fbm(p * 0.045, 3, seed ^ 31));
        let mut keep = if f < 1.0 {
            let base = (forest.dense_spacing / forest.transition_spacing).powi(2);
            (base + (1.0 - base) * f.powf(1.6)) * (0.25 + 0.75 * clump) + 0.15 * f
        } else {
            1.0
        };
        // Small natural glades inside the forest (not right at the edge).
        let glade = smoothstep(0.7, 0.8, fbm(p * 0.022, 3, seed ^ 37)) * smoothstep(15.0, 40.0, edge);
        keep *= 1.0 - glade;
        if !layout.in_site(p) {
            keep *= (forest.dense_spacing / forest.far_spacing).powi(2);
        }
        if !rng.chance(keep) {
            continue;
        }

        // Keep the road, yard, shed and pad clear; the road gets a cleared verge
        // and the power line a corridor.
        let (road_d, _) = layout.road_distance(p);
        if layout.developed_distance(p) < 3.0 || road_d < layout.road_width * 0.5 + 3.5 {
            continue;
        }
        if (road_d - config.site.pole_offset).abs() < 3.0 && road_d < 12.0 {
            continue;
        }

        // Pine and fir grow in patches; young trees crowd the edges and glades.
        let pine_bias = config.forest.pine_fraction + 0.45 * fbm(p * 0.012, 2, seed ^ 41);
        let species = if rng.chance(pine_bias.clamp(0.1, 0.9)) { Species::Pine } else { Species::Fir };
        let young = 0.28 + 0.5 * (1.0 - f) + 0.4 * glade;
        let size = if rng.chance(young.min(0.85)) { SizeClass::Medium } else { SizeClass::Large };
        let variant = pick_variant(variants, species, size, rng);
        let stand_age = 0.94 + 0.12 * value_noise(p * 0.03, seed ^ 43);
        let scale = rng.range(forest.scale_range.clone()) * stand_age;
        trees.push(TreeInstance {
            variant,
            position: layout.ground_point(p) - Vec3::Y * 0.15,
            yaw: rng.angle(),
            scale,
            min_view: layout.min_view_distance(p),
        });
    }
    trees
}

/// A few lone trees just inside the tree line make the edge read as natural.
fn plan_edge_trees(layout: &SiteLayout, variants: &[TreeVariant], rng: &mut Rng) -> Vec<TreeInstance> {
    let mut out = Vec::new();
    let mut attempts = 0;
    while out.len() < 7 && attempts < 400 {
        attempts += 1;
        let p = Vec2::new(rng.range(-110.0..110.0), rng.range(-110.0..110.0));
        let edge = layout.clearing_distance(p);
        if !(-22.0..-7.0).contains(&edge) || layout.developed_distance(p) < 15.0 || p.length() < 55.0 {
            continue;
        }
        let species = if rng.chance(0.6) { Species::Pine } else { Species::Fir };
        let size = if rng.chance(0.6) { SizeClass::Medium } else { SizeClass::Large };
        out.push(TreeInstance {
            variant: pick_variant(variants, species, size, rng),
            position: layout.ground_point(p) - Vec3::Y * 0.15,
            yaw: rng.angle(),
            scale: rng.range(0.85..1.1),
            min_view: 0.0,
        });
    }
    out
}

fn cluster_center(layout: &SiteLayout, rng: &mut Rng, accept: impl Fn(Vec2, f32) -> bool) -> Option<Vec2> {
    for _ in 0..60 {
        let h = layout.half_extent + 25.0;
        let p = Vec2::new(rng.range(-h..h), rng.range(-h..h));
        if accept(p, layout.forest_factor(p)) {
            return Some(p);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn push_prop(
    props: &mut Vec<PropInstance>,
    layout: &SiteLayout,
    mesh: impl Into<String>,
    lods: u8,
    switch: f32,
    cull: f32,
    p: Vec2,
    yaw: f32,
    scale: f32,
    sink: f32,
    shadows: bool,
    tilt_to_ground: bool,
) {
    let normal = layout.ground_normal(p);
    let mut rotation = Quat::from_rotation_y(yaw);
    if tilt_to_ground {
        rotation = Quat::from_rotation_arc(Vec3::Y, normal) * rotation;
    }
    props.push(PropInstance {
        mesh: mesh.into(),
        lods,
        switch,
        cull,
        transform: Transform::from_translation(layout.ground_point(p) - Vec3::Y * sink)
            .with_rotation(rotation)
            .with_scale(Vec3::splat(scale)),
        shadows,
    });
}

fn plan_ground_cover(
    config: &ForestConfig,
    layout: &SiteLayout,
    trunks: &SpatialGrid,
    props: &mut Vec<PropInstance>,
    rng: &mut Rng,
) {
    let gc = &config.ground_cover;
    let lod = &config.lod;
    let seed = config.seed as u32;
    let clear = |p: Vec2, r: f32| layout.developed_distance(p) > 1.5 + r && !trunks.overlaps(p, r);

    const FERNS: [&str; 4] = ["fern_a", "fern_b", "fern_c", "fern_d"];
    const ROCKS: [&str; 13] = [
        "rock_moss_a", "rock_moss_b", "rock_moss_c", "rock_moss_d", "rock_moss_e", "rock_moss_f", "rock_river_07",
        "rock_river_08", "rock_river_09", "rock_river_10", "rock_river_11", "rock_river_12", "rock_river_13",
    ];
    const BRANCHES: [&str; 3] = ["dry_branch_a", "dry_branch_b", "dry_branch_c"];
    const GRASS: [&str; 11] = [
        "grass_large_a", "grass_large_b", "grass_large_c", "grass_mid_a", "grass_mid_b", "grass_mid_c",
        "grass_small_a", "grass_small_b", "grass_tall_a", "grass_tall_b", "grass_tall_c",
    ];

    // Ferns: shady, damp patches in the forest and the transition band.
    for _ in 0..gc.fern_clusters {
        let Some(c) = cluster_center(layout, rng, |p, f| f > 0.25 && fbm(p * 0.03, 2, seed ^ 51) > 0.0) else { continue };
        let n = 4 + rng.index(10);
        let spread = rng.range(2.0..5.5);
        let kind = rng.index(FERNS.len());
        for _ in 0..n {
            let p = c + Vec2::new(rng.normal(), rng.normal()) * spread;
            if !clear(p, 0.4) || layout.forest_factor(p) < 0.15 {
                continue;
            }
            let name = if rng.chance(0.7) { FERNS[kind] } else { *rng.pick(&FERNS) };
            push_prop(props, layout, name, 2, 18.0, lod.small_prop_cull, p, rng.angle(), rng.range(0.75..1.35), 0.02, true, true);
        }
    }

    // Rock clusters, mostly in the forest; a few spill into the meadow margin.
    for _ in 0..gc.rock_clusters {
        let Some(c) = cluster_center(layout, rng, |p, f| f > 0.4 || (f > 0.0 && layout.clearing_distance(p) > -10.0)) else {
            continue;
        };
        let n = 3 + rng.index(9);
        let spread = rng.range(1.0..3.5);
        for _ in 0..n {
            let p = c + Vec2::new(rng.normal(), rng.normal()) * spread;
            let scale = if rng.chance(0.2) { rng.range(3.0..5.0) } else { rng.range(1.2..2.8) };
            if !clear(p, 0.25 * scale) {
                continue;
            }
            let cull = if scale > 3.0 { lod.ground_cover_cull } else { lod.small_prop_cull + 20.0 };
            push_prop(props, layout, *rng.pick(&ROCKS), 2, 25.0, cull, p, rng.angle(), scale, 0.04 * scale, true, true);
        }
        // Moss patches hug the rocks.
        for _ in 0..rng.index(4) {
            let p = c + Vec2::new(rng.normal(), rng.normal()) * spread * 1.2;
            if clear(p, 0.3) {
                let name = format!("moss_patch_{}", 1 + rng.index(3));
                push_prop(props, layout, name, 1, 0.0, lod.small_prop_cull, p, rng.angle(), rng.range(2.0..4.0), 0.0, false, true);
            }
        }
    }

    // Boulders along the forest edge.
    for _ in 0..gc.boulders {
        let Some(p) = cluster_center(layout, rng, |p, f| f > 0.2 && f < 0.95 && layout.developed_distance(p) > 8.0) else {
            continue;
        };
        let scale = rng.range(5.0..8.5);
        if trunks.overlaps(p, 0.2 * scale) {
            continue;
        }
        push_prop(props, layout, *rng.pick(&ROCKS[..6]), 2, 45.0, 320.0, p, rng.angle(), scale, 0.06 * scale, true, true);
    }

    // Dead branches on the forest floor.
    for _ in 0..gc.branch_count {
        let Some(p) = cluster_center(layout, rng, |_, f| f > 0.5) else { continue };
        if clear(p, 0.5) {
            push_prop(props, layout, *rng.pick(&BRANCHES), 2, 15.0, lod.small_prop_cull - 10.0, p, rng.angle(), rng.range(0.8..1.7), 0.02, false, true);
        }
    }

    // Stumps: the transition band looks selectively logged.
    for _ in 0..gc.stump_count {
        let Some(p) = cluster_center(layout, rng, |p, f| f > 0.05 && f < 0.9 && layout.clearing_distance(p) > -4.0) else {
            continue;
        };
        if clear(p, 0.6) {
            push_prop(props, layout, "stump", 2, 30.0, lod.ground_cover_cull, p, rng.angle(), rng.range(0.9..1.6), 0.03, true, true);
            if rng.chance(0.3) {
                let q = p + Vec2::from_angle(rng.angle()) * rng.range(1.5..3.0);
                if clear(q, 1.0) {
                    push_prop(props, layout, "fallen_log", 2, 35.0, lod.ground_cover_cull, q, rng.angle(), rng.range(1.2..1.9), 0.08, true, true);
                }
            }
        }
    }

    // Fallen logs in the forest.
    for _ in 0..gc.log_count {
        let Some(p) = cluster_center(layout, rng, |_, f| f > 0.6) else { continue };
        let scale = rng.range(1.1..2.2);
        if clear(p, 1.2 * scale) {
            push_prop(props, layout, "fallen_log", 2, 35.0, lod.ground_cover_cull, p, rng.angle(), scale, 0.06 * scale, true, true);
        }
    }

    // Exposed roots at some trunks along the edge.
    for _ in 0..(gc.stump_count / 2) {
        let Some(p) = cluster_center(layout, rng, |_, f| f > 0.3 && f < 1.0) else { continue };
        if layout.developed_distance(p) > 3.0 {
            let name = if rng.chance(0.5) { "roots_a" } else { "roots_b" };
            push_prop(props, layout, name, 2, 20.0, lod.small_prop_cull, p, rng.angle(), rng.range(1.4..2.4), 0.02, false, true);
        }
    }

    // Tall grass: dense along the unmown margin and the transition band.
    for _ in 0..gc.grass_clusters {
        let Some(c) = cluster_center(layout, rng, |p, f| f < 0.7 && layout.clearing_distance(p) > -26.0) else { continue };
        let (count, spread) = (12 + rng.index(28), rng.range(1.5..4.0));
        grass_cluster(props, layout, rng, c, count, spread, lod.grass_cull, &GRASS, &clear);
    }
    // Sparse tufts in the meadow, kept off the mown launch area.
    for _ in 0..gc.meadow_grass_clusters {
        let Some(c) = cluster_center(layout, rng, |p, f| {
            f == 0.0 && p.length() > layout.launch_clear_radius * 1.6 && fbm(p * 0.02, 2, seed ^ 61) > -0.1
        }) else {
            continue;
        };
        let (count, spread) = (6 + rng.index(16), rng.range(1.0..2.5));
        grass_cluster(props, layout, rng, c, count, spread, lod.grass_cull, &GRASS, &clear);
    }
}

#[allow(clippy::too_many_arguments)]
fn grass_cluster(
    props: &mut Vec<PropInstance>,
    layout: &SiteLayout,
    rng: &mut Rng,
    center: Vec2,
    count: usize,
    spread: f32,
    cull: f32,
    names: &[&str],
    clear: &impl Fn(Vec2, f32) -> bool,
) {
    let kind = rng.index(names.len());
    for _ in 0..count {
        let p = center + Vec2::new(rng.normal(), rng.normal()) * spread;
        if !clear(p, 0.2) {
            continue;
        }
        let name = if rng.chance(0.6) { names[kind] } else { *rng.pick(names) };
        push_prop(props, layout, name, 2, 12.0, cull, p, rng.angle(), rng.range(1.6..3.2), 0.0, false, true);
    }
}

// --------------------------------------------------------------------------- spawning

/// Visibility window for LOD `index` of a chain with hand-over distances `switches`.
pub fn lod_range(index: usize, switches: &[f32], cull: f32, fade: f32) -> VisibilityRange {
    let start = if index == 0 { 0.0 } else { switches[index - 1] };
    let end = if index < switches.len() { switches[index] } else { cull };
    let half = fade * 0.5;
    VisibilityRange {
        start_margin: if index == 0 { 0.0..0.0 } else { (start - half)..(start + half) },
        end_margin: (end - half)..(end + half),
        use_aabb: false,
    }
}

type LodBundle = (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform, VisibilityRange);

#[derive(Default)]
pub struct SpawnBatches {
    pub casters: Vec<LodBundle>,
    pub non_casters: Vec<(Mesh3d, MeshMaterial3d<StandardMaterial>, Transform, VisibilityRange, NotShadowCaster)>,
}

impl SpawnBatches {
    pub fn push(&mut self, library: &MeshLibrary, name: &str, transform: Transform, range: VisibilityRange, shadows: bool) {
        for part in library.parts(name) {
            let bundle = (Mesh3d(part.mesh.clone()), MeshMaterial3d(part.material.clone()), transform, range.clone());
            if shadows {
                self.casters.push(bundle);
            } else {
                self.non_casters.push((bundle.0, bundle.1, bundle.2, bundle.3, NotShadowCaster));
            }
        }
    }

    pub fn spawn(self, commands: &mut Commands) {
        commands.spawn_batch(self.casters);
        commands.spawn_batch(self.non_casters);
    }
}

pub fn spawn(
    commands: &mut Commands,
    config: &ForestConfig,
    plan: &VegetationPlan,
    library: &MeshLibrary,
    meshes: &mut Assets<Mesh>,
) {
    let fade = config.lod.crossfade;
    let switches = config.lod.tree;
    let mut batches = SpawnBatches::default();
    let mut merged: HashMap<(i32, i32), Vec<(usize, Transform)>> = HashMap::new();

    for tree in &plan.trees {
        let variant = &plan.variants[tree.variant];
        let transform = Transform::from_translation(tree.position)
            .with_rotation(Quat::from_rotation_y(tree.yaw))
            .with_scale(Vec3::splat(tree.scale));
        // Trees that can only ever be seen as impostors are merged into chunks.
        if tree.min_view > switches[2] + fade {
            let key = ((tree.position.x / 120.0).floor() as i32, (tree.position.z / 120.0).floor() as i32);
            merged.entry(key).or_default().push((tree.variant, transform));
            continue;
        }
        for lod in 0..4 {
            let range = lod_range(lod, &switches, f32::MAX / 4.0, fade);
            if range.end_margin.end < tree.min_view {
                continue;
            }
            // Impostors and the far LOD do not need to cast shadows.
            let shadows = lod < 2 || (lod == 2 && config.lighting.shadow_distance > switches[1]);
            batches.push(library, &format!("{}_lod{lod}", variant.name), transform, range, shadows);
        }
    }

    for prop in &plan.props {
        if prop.lods == 1 {
            batches.push(library, &prop.mesh, prop.transform, lod_range(0, &[], prop.cull, fade), prop.shadows);
            continue;
        }
        for lod in 0..prop.lods as usize {
            let range = lod_range(lod, &[prop.switch], prop.cull, fade);
            // Distant LODs of small props never need shadows.
            batches.push(library, &format!("{}_lod{lod}", prop.mesh), prop.transform, range, prop.shadows && lod == 0);
        }
    }
    let instance_count = batches.casters.len() + batches.non_casters.len();
    batches.spawn(commands);

    let mut chunk_count = 0;
    for (key, members) in merged {
        let names: Vec<String> = members.iter().map(|(v, _)| format!("{}_lod3", plan.variants[*v].name)).collect();
        let part = &library.parts(&names[0])[0];
        let mut parts = Vec::new();
        for ((_, transform), name) in members.iter().zip(&names) {
            if let Some(mesh) = meshes.get(&library.parts(name)[0].mesh) {
                parts.push((mesh.clone(), *transform));
            }
        }
        if let Some(mesh) = merge_meshes(&parts) {
            commands.spawn((
                Name::new(format!("Far forest chunk {key:?}")),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(part.material.clone()),
                NotShadowCaster,
            ));
            chunk_count += 1;
        }
    }
    info!(
        "forest: {} trees, {} ground-cover props, {} LOD entities, {} merged far-forest chunks",
        plan.trees.len(),
        plan.props.len(),
        instance_count,
        chunk_count
    );
}

/// Bake many instances of (identically laid out) meshes into one static mesh.
fn merge_meshes(parts: &[(Mesh, Transform)]) -> Option<Mesh> {
    let first = &parts.first()?.0;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut tangents: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let has_tangents = first.attribute(Mesh::ATTRIBUTE_TANGENT).is_some();

    for (mesh, transform) in parts {
        let base = positions.len() as u32;
        let matrix = transform.to_matrix();
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { continue };
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { continue };
        positions.extend(p.iter().map(|v| matrix.transform_point3(Vec3::from_array(*v)).to_array()));
        normals.extend(n.iter().map(|v| (transform.rotation * Vec3::from_array(*v)).to_array()));
        uvs.extend_from_slice(uv);
        if has_tangents {
            if let Some(VertexAttributeValues::Float32x4(t)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
                tangents.extend(t.iter().map(|v| {
                    let r = transform.rotation * Vec3::new(v[0], v[1], v[2]);
                    [r.x, r.y, r.z, v[3]]
                }));
            }
        }
        match mesh.indices() {
            Some(Indices::U16(i)) => indices.extend(i.iter().map(|&x| base + x as u32)),
            Some(Indices::U32(i)) => indices.extend(i.iter().map(|&x| base + x)),
            None => indices.extend((0..p.len() as u32).map(|x| base + x)),
        }
    }
    let mut mesh = Mesh::new(first.primitive_topology(), bevy::asset::RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices));
    if has_tangents {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::super::config::Quality;
    use super::*;

    #[test]
    fn variants_cover_both_species_and_sizes() {
        let v = tree_variants();
        assert_eq!(v.len(), 17);
        for species in [Species::Pine, Species::Fir] {
            for size in [SizeClass::Large, SizeClass::Medium] {
                assert!(v.iter().any(|t| t.species == species && t.size == size));
            }
        }
        assert!(v.iter().all(|t| (5.0..25.0).contains(&t.height)), "trees should be real-world sized");
    }

    #[test]
    fn flight_area_stays_open() {
        let config = ForestConfig::new(Quality::Medium);
        let layout = SiteLayout::new(&config);
        let plan = plan(&config, &layout);
        for t in &plan.trees {
            let p = Vec2::new(t.position.x, t.position.z);
            assert!(p.length() > 50.0, "tree at {p} intrudes on the launch area");
            assert!(layout.developed_distance(p) > 2.0, "tree at {p} on developed ground");
        }
        for prop in &plan.props {
            let p = prop.transform.translation.xz();
            assert!(p.length() > layout.launch_clear_radius, "{} at {p} on the launch area", prop.mesh);
        }
        let inside = plan.trees.iter().filter(|t| layout.in_site(t.position.xz())).count();
        assert!((800..4000).contains(&inside), "unexpected tree count in site: {inside}");
    }

    #[test]
    fn lod_ranges_hand_over_without_gaps() {
        let switches = [45.0, 110.0, 190.0];
        for i in 0..3 {
            let a = lod_range(i, &switches, 1e6, 6.0);
            let b = lod_range(i + 1, &switches, 1e6, 6.0);
            assert_eq!(a.end_margin, b.start_margin);
        }
    }
}
