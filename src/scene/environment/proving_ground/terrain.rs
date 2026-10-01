//! Ground: one splat-mapped terrain mesh, the asphalt work yard and the launch pad.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use super::{
    assets::EnvironmentAssets,
    config::ProvingGroundConfig,
    layout::SiteLayout,
    scatter::{fbm, smoothstep, value_noise},
    vegetation::VegetationPlan,
};

pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

const SHADER: &str = "shaders/proving_ground_terrain.wgsl";

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainExtension {
    #[uniform(100)]
    pub params: TerrainParams,
    #[texture(101, dimension = "2d_array")]
    #[sampler(102)]
    pub albedo: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    #[sampler(104)]
    pub normal: Handle<Image>,
}

#[derive(ShaderType, Reflect, Debug, Clone)]
pub struct TerrainParams {
    pub layer_scale: [Vec4; 2],
    pub layer_tint: [Vec4; 5],
    pub macro_params: Vec4,
    pub blend_params: Vec4,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// Per-layer tile sizes (m), tints and roughness. Tints pull the five source
/// textures (from three different libraries) into one coherent palette.
fn terrain_params(config: &ProvingGroundConfig) -> TerrainParams {
    TerrainParams {
        layer_scale: [Vec4::new(2.6, 3.2, 4.0, 2.8), Vec4::new(3.5, 0.0, 0.0, 0.0)],
        layer_tint: [
            Vec4::new(0.84, 0.88, 0.78, 0.93), // meadow grass
            Vec4::new(0.5, 0.56, 0.36, 0.95), // dry/worn grass
            Vec4::new(0.82, 0.80, 0.74, 0.96), // forest floor
            Vec4::new(0.8, 0.85, 0.98, 0.88), // gravel
            Vec4::new(0.80, 0.74, 0.66, 0.97), // dirt
        ],
        macro_params: Vec4::new(1.0 / 55.0, 0.16, 3.2, 0.07),
        blend_params: Vec4::new(1.0, 0.55, config.forest.far_half_extent + 30.0, config.forest.far_half_extent + 260.0),
    }
}

pub fn spawn(
    commands: &mut Commands,
    config: &ProvingGroundConfig,
    layout: &SiteLayout,
    plan: &VegetationPlan,
    env: &EnvironmentAssets,
    meshes: &mut Assets<Mesh>,
    terrain_materials: &mut Assets<TerrainMaterial>,
    materials: &mut Assets<StandardMaterial>,
) {
    let material = terrain_materials.add(TerrainMaterial {
        base: StandardMaterial { perceptual_roughness: 0.92, reflectance: 0.25, ..default() },
        extension: TerrainExtension {
            params: terrain_params(config),
            albedo: env.terrain_albedo.clone(),
            normal: env.terrain_normal.clone(),
        },
    });
    commands.spawn((
        Name::new("Proving ground terrain"),
        Mesh3d(meshes.add(terrain_mesh(config, layout, plan))),
        MeshMaterial3d(material),
    ));

    spawn_yard(commands, layout, env, meshes, materials);
    spawn_launch_pad(commands, layout, env, meshes, materials);
}

// --------------------------------------------------------------------------- terrain mesh

fn axis_lines(inner_half: f32, spacing: f32, growth: f32, outer_half: f32) -> Vec<f32> {
    let mut positive = Vec::new();
    let mut x = 0.0;
    while x < inner_half {
        positive.push(x);
        x += spacing;
    }
    let mut step = spacing;
    while x < outer_half {
        positive.push(x);
        step *= growth;
        x += step;
    }
    positive.push(outer_half);
    let mut lines: Vec<f32> = positive.iter().skip(1).rev().map(|v| -v).collect();
    lines.extend(positive);
    lines
}

/// Splat weights (dry grass, forest floor, gravel, dirt) and the mown mask.
pub(super) fn splat(layout: &SiteLayout, p: Vec2, clearing_d: f32, road: (f32, f32)) -> ([f32; 4], f32) {
    let (road_d, road_s) = road;
    let hw = layout.road_width * 0.5;
    let n_fine = value_noise(p * 0.35, 5);
    let n_mid = fbm(p * 0.05, 3, 7);

    // Forest floor: follows the tree line with its own wobble.
    let forest = smoothstep(-4.0, layout.transition_width * 0.75, clearing_d + 7.0 * n_mid);

    // Dry / worn grass.
    let meadow_patch = smoothstep(0.25, 0.6, fbm(p * 0.018, 3, 9)) * 0.75;
    let margin = smoothstep(-26.0, -6.0, clearing_d) * 0.75; // unmown strip at the tree line
    let pad_ring = (1.0 - smoothstep(4.5, 10.0 + 2.0 * n_fine, p.length())) * 0.65;
    let yard_d = layout.yard_distance(p);
    let yard_fringe = (1.0 - smoothstep(1.0, 8.0 + 3.0 * n_mid, yard_d)) * 0.7;
    let dry = meadow_patch.max(margin).max(pad_ring).max(yard_fringe) * (1.0 - forest * 0.85);

    // Gravel road with a patchy grass strip down the middle.
    let edge = hw + 0.35 * n_fine;
    let road_mask = 1.0 - smoothstep(edge - 0.45, edge + 0.35, road_d);
    let strip = (1.0 - smoothstep(0.22, 0.55, road_d)) * smoothstep(0.35, 0.6, value_noise(Vec2::new(road_s * 0.07, 0.0), 13));
    let mut gravel = road_mask * (1.0 - 0.8 * strip);
    // Gravel apron around the asphalt.
    gravel = gravel.max((1.0 - smoothstep(0.1, 1.6 + 0.8 * n_fine, yard_d)) * 0.9);

    // Dirt: road shoulders, the walked path from the yard to the pad, shed apron.
    let shoulder = smoothstep(edge - 0.1, edge + 0.4, road_d) * (1.0 - smoothstep(edge + 0.7, edge + 2.2 + n_mid, road_d));
    let path_d = footpath_distance(layout, p);
    let path = (1.0 - smoothstep(0.25, 0.9, path_d)) * (0.55 + 0.3 * n_fine);
    let (shed_c, shed_h) = layout.shed_footprint();
    let q = (layout.yard_local(p) - shed_c).abs() - shed_h;
    let shed_d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
    let shed_apron = (1.0 - smoothstep(0.0, 2.5 + 1.5 * n_fine, shed_d)) * 0.8;
    let forest_dirt = forest * smoothstep(0.62, 0.8, value_noise(p * 0.11, 17)) * 0.6;
    let dirt = (shoulder * 0.85).max(path).max(shed_apron).max(forest_dirt);

    // Developed surfaces override the vegetation layers; the road's centre strip
    // is short dry grass.
    let developed = gravel.max(dirt).max(road_mask);
    let mut w = [dry * (1.0 - developed) + strip * road_mask * 0.85, forest * (1.0 - developed * 0.95), gravel, dirt];
    let sum: f32 = w.iter().sum();
    if sum > 1.0 {
        for v in &mut w {
            *v /= sum;
        }
    }
    // The maintained (mown) field stops short of the tree line and the yard.
    let mown = (1.0 - smoothstep(-28.0, -16.0, clearing_d)) * smoothstep(3.0, 9.0, yard_d);
    (w, mown)
}

/// The walked path from the front of the yard to the launch pad.
pub(super) fn footpath_distance(layout: &SiteLayout, p: Vec2) -> f32 {
    let start = layout.yard_world(Vec2::new(-4.0, -layout.yard_half_size.y));
    let end = Vec2::new(4.5, 3.0);
    let mid = (start + end) * 0.5 + Vec2::new(-6.0, 4.0);
    let mut best = f32::MAX;
    let mut prev = start;
    for i in 1..=24 {
        let t = i as f32 / 24.0;
        let point = start * (1.0 - t) * (1.0 - t) + mid * 2.0 * t * (1.0 - t) + end * t * t;
        let ab = point - prev;
        let h = ((p - prev).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        best = best.min(p.distance(prev + ab * h));
        prev = point;
    }
    best
}

fn terrain_mesh(config: &ProvingGroundConfig, layout: &SiteLayout, plan: &VegetationPlan) -> Mesh {
    let t = &config.terrain;
    let lines = axis_lines(layout.half_extent + 30.0, t.inner_spacing, t.outer_growth, t.half_extent);
    let n = lines.len();

    // Road distance only matters near the road; skip it elsewhere.
    let (road_min, road_max) = layout
        .road
        .iter()
        .fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let (road_min, road_max) = (road_min - Vec2::splat(40.0), road_max + Vec2::splat(40.0));

    let mut positions = Vec::with_capacity(n * n);
    let mut colors = Vec::with_capacity(n * n);
    let mut uvs = Vec::with_capacity(n * n);
    let mut heights = vec![0.0f32; n * n];
    for (j, &z) in lines.iter().enumerate() {
        for (i, &x) in lines.iter().enumerate() {
            let p = Vec2::new(x, z);
            let clearing_d = layout.clearing_distance(p);
            let road = if p.cmpge(road_min).all() && p.cmple(road_max).all() {
                layout.road_distance(p)
            } else {
                (1e4, 0.0)
            };
            let h = layout.ground_height(p);
            heights[j * n + i] = h;
            let (w, mown) = splat(layout, p, clearing_d, road);
            let canopy = plan.canopy_openness(p);
            positions.push([x, h, z]);
            colors.push(w);
            uvs.push([canopy, mown]);
        }
    }

    let mut normals = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let (i0, i1) = (i.saturating_sub(1), (i + 1).min(n - 1));
            let (j0, j1) = (j.saturating_sub(1), (j + 1).min(n - 1));
            let dx = (heights[j * n + i1] - heights[j * n + i0]) / (lines[i1] - lines[i0]);
            let dz = (heights[j1 * n + i] - heights[j0 * n + i]) / (lines[j1] - lines[j0]);
            normals.push(Vec3::new(-dx, 1.0, -dz).normalize().to_array());
        }
    }

    let mut indices = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for j in 0..n as u32 - 1 {
        for i in 0..n as u32 - 1 {
            let a = j * n as u32 + i;
            let b = a + 1;
            let c = a + n as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

// --------------------------------------------------------------------------- asphalt

/// A flat slab whose outline is a rounded rectangle with a slightly ragged edge,
/// so the asphalt does not look like a perfect CAD rectangle.
fn slab_mesh(half: Vec2, corner: f32, thickness: f32, ragged: f32, seed: u32, uv_tile: f32, to_world: impl Fn(Vec2) -> Vec2) -> Mesh {
    let mut outline = Vec::new();
    let corners = [
        Vec2::new(half.x - corner, half.y - corner),
        Vec2::new(-half.x + corner, half.y - corner),
        Vec2::new(-half.x + corner, -half.y + corner),
        Vec2::new(half.x - corner, -half.y + corner),
    ];
    for (k, c) in corners.iter().enumerate() {
        let start = k as f32 * std::f32::consts::FRAC_PI_2;
        for s in 0..=6 {
            let a = start + s as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
            outline.push(*c + Vec2::from_angle(a) * corner);
        }
        // Subdivide the straight edge to the next corner.
        let next = corners[(k + 1) % 4] + Vec2::from_angle(start + std::f32::consts::FRAC_PI_2) * corner;
        let from = *outline.last().unwrap();
        let steps = (from.distance(next) / 0.6).ceil() as usize;
        for s in 1..steps {
            outline.push(from.lerp(next, s as f32 / steps as f32));
        }
    }
    for (i, p) in outline.iter_mut().enumerate() {
        let jitter = value_noise(Vec2::new(i as f32 * 0.7, 0.0), seed) * ragged;
        *p += p.normalize_or_zero() * jitter;
    }

    let mut positions = vec![];
    let mut normals = vec![];
    let mut uvs = vec![];
    let mut indices: Vec<u32> = vec![];
    // Top fan.
    let center = to_world(Vec2::ZERO);
    positions.push([center.x, thickness, center.y]);
    normals.push([0.0, 1.0, 0.0]);
    uvs.push([center.x / uv_tile, center.y / uv_tile]);
    for p in &outline {
        let w = to_world(*p);
        positions.push([w.x, thickness, w.y]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([w.x / uv_tile, w.y / uv_tile]);
    }
    let m = outline.len() as u32;
    for i in 0..m {
        let a = 1 + i;
        let b = 1 + (i + 1) % m;
        indices.extend_from_slice(&[0, b, a]);
    }
    // Edge skirt down into the ground.
    let base = positions.len() as u32;
    for (i, p) in outline.iter().enumerate() {
        let w = to_world(*p);
        let next = to_world(outline[(i + 1) % outline.len()]);
        let prev = to_world(outline[(i + outline.len() - 1) % outline.len()]);
        let tangent = (next - prev).normalize_or_zero();
        let out = Vec2::new(tangent.y, -tangent.x);
        let side = Vec3::new(out.x, 0.6, out.y).normalize();
        positions.push([w.x, thickness, w.y]);
        positions.push([w.x + out.x * 0.05, -0.12, w.y + out.y * 0.05]);
        normals.push(side.to_array());
        normals.push(side.to_array());
        uvs.push([w.x / uv_tile, w.y / uv_tile]);
        uvs.push([w.x / uv_tile, (w.y + 0.15) / uv_tile]);
    }
    for i in 0..m {
        let a = base + 2 * i;
        let b = base + 2 * ((i + 1) % m);
        indices.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices));
    mesh.generate_tangents().expect("slab has positions, normals and UVs");
    mesh
}

fn asphalt_material(env: &EnvironmentAssets, tint: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: tint,
        base_color_texture: Some(env.asphalt.diffuse.clone()),
        normal_map_texture: Some(env.asphalt.normal.clone()),
        // Greyscale roughness map: G is read as roughness; metallic factor 0 zeroes B.
        metallic_roughness_texture: Some(env.asphalt.roughness.clone()),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.35,
        ..default()
    }
}

fn spawn_yard(
    commands: &mut Commands,
    layout: &SiteLayout,
    env: &EnvironmentAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = slab_mesh(layout.yard_half_size, 1.6, 0.04, 0.22, 31, 3.2, |p| layout.yard_world(p));
    commands.spawn((
        Name::new("Asphalt work yard"),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(asphalt_material(env, Color::srgb(0.95, 0.95, 0.95)))),
    ));

    // Faded parking stall lines along the front edge.
    let paint = materials.add(StandardMaterial {
        base_color: Color::srgba(0.82, 0.82, 0.78, 0.82),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.75,
        depth_bias: 2.0,
        ..default()
    });
    let line = meshes.add(Plane3d::new(Vec3::Y, Vec2::new(0.06, 2.4)));
    for k in 0..6 {
        let local = Vec2::new(-11.0 + k as f32 * 2.8, -layout.yard_half_size.y + 2.8);
        let w = layout.yard_world(local);
        commands.spawn((
            Name::new("Parking line"),
            Mesh3d(line.clone()),
            MeshMaterial3d(paint.clone()),
            Transform::from_xyz(w.x, 0.043, w.y).with_rotation(layout.yard_rotation(0.0)),
        ));
    }
}

fn spawn_launch_pad(
    commands: &mut Commands,
    layout: &SiteLayout,
    env: &EnvironmentAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let half = layout.launch_pad_size * 0.5;
    let slab = slab_mesh(Vec2::splat(half), 0.3, 0.06, 0.03, 47, 3.2, |p| p);
    commands.spawn((
        Name::new("Launch pad"),
        Mesh3d(meshes.add(slab)),
        MeshMaterial3d(materials.add(asphalt_material(env, Color::srgb(0.9, 0.9, 0.9)))),
    ));

    let paint = materials.add(StandardMaterial {
        base_color: Color::srgb(0.86, 0.86, 0.82),
        perceptual_roughness: 0.6,
        depth_bias: 2.0,
        ..default()
    });
    let y = 0.063;
    let ring = meshes.add(Annulus::new(half * 0.72, half * 0.8).mesh().resolution(64));
    commands.spawn((
        Name::new("Launch pad ring"),
        Mesh3d(ring),
        MeshMaterial3d(paint.clone()),
        Transform::from_xyz(0.0, y, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    // "H" marking, upright when viewed from the yard side.
    let bar = meshes.add(Plane3d::new(Vec3::Y, Vec2::new(0.18, 1.1)));
    let cross = meshes.add(Plane3d::new(Vec3::Y, Vec2::new(0.6, 0.16)));
    let heading = Quat::from_rotation_y(layout.yard_heading);
    for (mesh, offset) in [(bar.clone(), Vec3::X * -0.75), (bar, Vec3::X * 0.75), (cross, Vec3::ZERO)] {
        commands.spawn((
            Name::new("Launch pad marking"),
            Mesh3d(mesh),
            MeshMaterial3d(paint.clone()),
            Transform::from_translation(heading * offset + Vec3::Y * y).with_rotation(heading),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::Quality;
    use super::*;

    #[test]
    fn axis_lines_are_symmetric_and_reach_the_edge() {
        let lines = axis_lines(180.0, 1.0, 1.08, 900.0);
        assert_eq!(lines.first().copied(), Some(-900.0));
        assert_eq!(lines.last().copied(), Some(900.0));
        assert!(lines.windows(2).all(|w| w[1] > w[0]));
        let inner = lines.windows(2).filter(|w| w[0] >= -150.0 && w[1] <= 150.0);
        assert!(inner.into_iter().all(|w| (w[1] - w[0] - 1.0).abs() < 1e-3));
    }

    #[test]
    fn road_is_gravel_and_launch_area_is_meadow() {
        let config = ProvingGroundConfig::new(Quality::Medium);
        let layout = SiteLayout::new(&config);
        for t in [0.2, 0.4, 0.6, 0.8] {
            // In the wheel track, 1.1 m off the centre line.
            let (p, dir) = layout.road_at(layout.road_total_length() * t);
            let p = p + Vec2::new(-dir.y, dir.x) * 1.1;
            let (w, _) = splat(&layout, p, layout.clearing_distance(p), layout.road_distance(p));
            assert!(w[2] > 0.7 && w[1] < 0.1, "wheel track should be gravel: {w:?}");
        }
        let q = Vec2::new(-30.0, -25.0);
        let (w, mown) = splat(&layout, q, layout.clearing_distance(q), layout.road_distance(q));
        assert!(w[1] < 0.05 && w[2] < 0.05, "open field should be grass: {w:?}");
        assert!(mown > 0.9);
    }
}
