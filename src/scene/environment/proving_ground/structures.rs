//! The developed corner of the site: shed, asphalt yard dressing, storage bay,
//! barrels, concrete barriers and the electricity line along the road.
//!
//! Positions are authored in the yard frame (x along the yard's long side towards
//! the road, y away from the clearing) so the whole area moves with the yard.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use super::{
    assets::{EnvironmentAssets, MeshLibrary},
    config::ProvingGroundConfig,
    layout::{yaw_towards, SiteLayout},
    scatter::Rng,
    vegetation::{lod_range, SpawnBatches},
};

struct Placer<'a> {
    layout: &'a SiteLayout,
    library: &'a MeshLibrary,
    batches: SpawnBatches,
    fade: f32,
}

impl Placer<'_> {
    /// Place a library mesh (with or without `_lod0/_lod1`) at yard-local `local`.
    fn yard(&mut self, name: &str, local: Vec2, yaw: f32, lift: f32) {
        let p = self.layout.yard_world(local);
        let transform = Transform::from_translation(self.layout.ground_point(p) + Vec3::Y * lift)
            .with_rotation(self.layout.yard_rotation(yaw));
        self.world(name, transform, 30.0, 220.0);
    }

    fn world(&mut self, name: &str, transform: Transform, switch: f32, cull: f32) {
        if self.library.contains(&format!("{name}_lod0")) {
            for lod in 0..2 {
                let range = lod_range(lod, &[switch], cull, self.fade);
                self.batches.push(self.library, &format!("{name}_lod{lod}"), transform, range, lod == 0);
            }
        } else {
            self.batches.push(self.library, name, transform, lod_range(0, &[], cull, self.fade), true);
        }
    }
}

/// Mission scan targets. North/east are relative to PX4 home, which Bevy puts at
/// the origin (Gazebo ENU x=east, y=north becomes Bevy (-north, up, -east)).
pub fn spawn_scan_targets(commands: &mut Commands, config: &ProvingGroundConfig, asset_server: &AssetServer) {
    for target in &config.site.scan_targets {
        let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(target.asset));
        commands.spawn((
            Name::new(format!("Scan target: {}", target.name)),
            WorldAssetRoot(scene),
            Transform::from_xyz(-target.north, target.ground_clearance, -target.east)
                .with_rotation(Quat::from_rotation_y(target.heading) * Quat::from_rotation_x(target.tilt)),
        ));
    }
}

pub fn spawn(
    commands: &mut Commands,
    config: &ProvingGroundConfig,
    layout: &SiteLayout,
    library: &MeshLibrary,
    env: &EnvironmentAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut rng = Rng::new(config.seed ^ 0x5354_5255);
    let mut placer = Placer { layout, library, batches: SpawnBatches::default(), fade: config.lod.crossfade };

    // ---------------------------------------------------------------- shed
    let (shed_center, _) = layout.shed_footprint();
    let shed_world = layout.yard_world(shed_center);
    for part in library.parts("shed_building") {
        commands.spawn((
            Name::new("Shed"),
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            Transform::from_translation(layout.ground_point(shed_world))
                .with_rotation(layout.yard_rotation(FRAC_PI_2)),
            VisibilityRange::abrupt(0.0, 600.0),
        ));
    }

    // Dressing along the front of the shed (the side facing the yard).
    let front = shed_center.y - 2.6;
    placer.yard("outdoor_table", Vec2::new(2.6, front - 1.6), 0.3, 0.0);
    placer.yard("outdoor_chair_a", Vec2::new(1.8, front - 1.0), 0.2, 0.0);
    placer.yard("outdoor_chair_b", Vec2::new(3.4, front - 2.3), PI + 0.4, 0.0);
    placer.yard("plastic_chair", Vec2::new(-6.2, front - 0.8), 2.6, 0.0);
    placer.yard("planter_box_02", Vec2::new(-1.0, front - 0.35), 0.0, 0.0);
    placer.yard("planter_box_01", Vec2::new(-8.6, front - 0.35), 0.0, 0.0);
    placer.yard("compost_bags_stacked", Vec2::new(-10.2, front + 0.6), 0.5, 0.0);
    placer.yard("compost_bags_standing", Vec2::new(-10.4, front - 0.4), 1.2, 0.0);
    placer.yard("watering_can", Vec2::new(-2.4, front - 0.6), 1.1, 0.0);
    placer.yard("clay_pot", Vec2::new(-3.0, front - 0.45), 0.0, 0.0);
    placer.yard("clay_pot", Vec2::new(-3.4, front - 0.5), 2.0, 0.0);
    placer.yard("wooden_stool", Vec2::new(-5.4, front - 1.4), 0.7, 0.0);
    placer.yard("cardboard_box", Vec2::new(5.6, front - 0.4), 0.15, 0.0);
    placer.yard("cardboard_box", Vec2::new(5.7, front - 0.45), 0.4, 0.34);
    placer.yard("hose_reel", Vec2::new(-4.6, front - 0.15), PI, 0.55);
    // Ladder leaning against the end wall.
    let ladder_p = layout.yard_world(Vec2::new(7.4, shed_center.y));
    placer.world(
        "ladder",
        Transform::from_translation(layout.ground_point(ladder_p) + Vec3::Y * 0.02)
            .with_rotation(layout.yard_rotation(FRAC_PI_2) * Quat::from_rotation_x(-0.28)),
        30.0,
        200.0,
    );
    let spade_p = layout.yard_world(Vec2::new(-7.3, front - 0.12));
    placer.world(
        "spade",
        Transform::from_translation(layout.ground_point(spade_p))
            .with_rotation(layout.yard_rotation(0.0) * Quat::from_rotation_x(0.22)),
        20.0,
        120.0,
    );

    // Weeds and nettles along the shed's base.
    for i in 0..14 {
        let x = -7.5 + i as f32 * 1.1 + rng.range(-0.3..0.3);
        let side = if rng.chance(0.5) { front - 0.2 } else { shed_center.y + 2.7 };
        let name = if rng.chance(0.4) { "nettle" } else { "weeds" };
        let local = Vec2::new(x, side + rng.range(-0.15..0.15));
        if rng.chance(0.7) {
            placer.yard(name, local, rng.angle(), 0.0);
        }
    }

    // ---------------------------------------------------------------- barrels
    let barrel_spot = Vec2::new(-layout.yard_half_size.x + 1.6, layout.yard_half_size.y - 1.5);
    for i in 0..7 {
        let local = barrel_spot + Vec2::new((i % 3) as f32 * 0.66, (i / 3) as f32 * 0.66) + Vec2::new(rng.range(-0.06..0.06), rng.range(-0.06..0.06));
        let name = if i % 3 == 1 { "barrel_02" } else { "barrel_01" };
        placer.yard(name, local, rng.angle(), 0.04);
    }
    // One knocked over, a little away from the group.
    let tipped = layout.yard_world(barrel_spot + Vec2::new(2.6, -1.4));
    placer.world(
        "barrel_01",
        Transform::from_translation(layout.ground_point(tipped) + Vec3::Y * 0.31)
            .with_rotation(layout.yard_rotation(0.9) * Quat::from_rotation_z(FRAC_PI_2)),
        30.0,
        200.0,
    );
    placer.yard("barrel_02", Vec2::new(layout.yard_half_size.x + 2.2, layout.yard_half_size.y + 1.6), 0.4, 0.0);

    // ---------------------------------------------------------------- concrete barriers
    // Row protecting the back corner of the yard (barriers are 1.54 m long).
    for i in 0..5 {
        let local = Vec2::new(layout.yard_half_size.x - 0.8 - i as f32 * 1.58, layout.yard_half_size.y + 0.7);
        placer.yard("concrete_barrier", local, 0.0 + rng.range(-0.03..0.03), 0.04);
    }
    // Loose ones along the yard's far end.
    for (local, yaw) in [
        (Vec2::new(-layout.yard_half_size.x - 1.0, -3.0), FRAC_PI_2 + 0.05),
        (Vec2::new(-layout.yard_half_size.x - 1.1, -1.4), FRAC_PI_2 - 0.08),
        (Vec2::new(-layout.yard_half_size.x - 2.6, -6.2), 1.1),
    ] {
        placer.yard("concrete_barrier", local, yaw, 0.0);
    }
    // Chicane where the road enters the site.
    let entry = layout.road.iter().position(|p| layout.in_site(*p)).unwrap_or(0);
    let (p, dir) = layout.road_at(layout.road_length[entry] + 6.0);
    let side = Vec2::new(-dir.y, dir.x);
    for (offset, along) in [(1.9, 0.0), (-1.9, 7.0)] {
        let q = p + side * offset + dir * along;
        placer.world(
            "concrete_barrier",
            Transform::from_translation(layout.ground_point(q)).with_rotation(Quat::from_rotation_y(yaw_towards(dir) + FRAC_PI_2)),
            30.0,
            260.0,
        );
    }

    // ---------------------------------------------------------------- storage bay
    spawn_storage_bay(commands, layout, env, meshes, materials, &mut placer);

    // ---------------------------------------------------------------- ground station by the pad
    let gcs = layout.yard_world(Vec2::new(-4.0, -layout.yard_half_size.y)).normalize() * 26.0;
    let gcs_yaw = yaw_towards(-gcs.normalize());
    let gcs_t = |offset: Vec2, yaw: f32| {
        let rot = Quat::from_rotation_y(gcs_yaw);
        let o = rot * Vec3::new(offset.x, 0.0, offset.y);
        Transform::from_translation(layout.ground_point(gcs + Vec2::new(o.x, o.z))).with_rotation(rot * Quat::from_rotation_y(yaw))
    };
    placer.world("outdoor_table", gcs_t(Vec2::ZERO, 0.0), 30.0, 220.0);
    placer.world("plastic_chair", gcs_t(Vec2::new(0.1, -0.9), PI), 30.0, 220.0);
    placer.world("cardboard_box", gcs_t(Vec2::new(0.9, 0.4), 0.3), 20.0, 150.0);

    // ---------------------------------------------------------------- electricity line
    spawn_power_line(commands, config, layout, meshes, materials, &mut placer);

    placer.batches.spawn(commands);
}

fn spawn_storage_bay(
    commands: &mut Commands,
    layout: &SiteLayout,
    env: &EnvironmentAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    placer: &mut Placer,
) {
    // Three-sided concrete bay, open towards the yard.
    let wall = materials.add(StandardMaterial {
        base_color_texture: Some(env.wall.diffuse.clone()),
        normal_map_texture: Some(env.wall.normal.clone()),
        metallic_roughness_texture: Some(env.wall.roughness.clone()),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        uv_transform: Affine2::from_scale(Vec2::new(2.0, 1.0)),
        ..default()
    });
    let center = Vec2::new(8.0, layout.yard_half_size.y + 3.6);
    let (width, depth, height, thick) = (6.0, 4.2, 2.1, 0.3);
    let walls = [
        (Vec2::new(0.0, depth * 0.5), Vec3::new(width, height, thick)),
        (Vec2::new(-width * 0.5, 0.0), Vec3::new(thick, height, depth)),
        (Vec2::new(width * 0.5, 0.0), Vec3::new(thick, height, depth)),
    ];
    for (offset, size) in walls {
        let p = layout.yard_world(center + offset);
        let mut mesh = Cuboid::new(size.x, size.y, size.z).mesh().build();
        // World-scale UVs: one texture repeat per ~3 m of wall.
        if let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
            for uv in uvs.iter_mut() {
                uv[0] *= size.x.max(size.z) / 3.0;
                uv[1] *= size.y / 3.0;
            }
        }
        mesh.generate_tangents().expect("cuboid has UVs and normals");
        commands.spawn((
            Name::new("Storage bay wall"),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(wall.clone()),
            Transform::from_translation(layout.ground_point(p) + Vec3::Y * (height * 0.5 - 0.05))
                .with_rotation(layout.yard_rotation(0.0)),
        ));
    }
    // Something stored in it.
    placer.yard("compost_bags_stacked", center + Vec2::new(-1.6, 0.9), 0.1, 0.0);
    placer.yard("compost_bags_stacked", center + Vec2::new(-1.5, 0.95), 0.0, 0.19);
    placer.yard("barrel_02", center + Vec2::new(1.9, 1.3), 0.0, 0.0);
    placer.yard("barrel_02", center + Vec2::new(1.3, 1.4), 1.0, 0.0);
}

// --------------------------------------------------------------------------- power line

/// Approximate attachment points of the three conductors on a pole preset,
/// in the pole's local frame (before scaling).
const CONDUCTORS: [Vec3; 3] = [Vec3::new(-0.52, 5.86, 0.0), Vec3::new(0.0, 6.02, 0.0), Vec3::new(0.52, 5.86, 0.0)];

fn spawn_power_line(
    commands: &mut Commands,
    config: &ProvingGroundConfig,
    layout: &SiteLayout,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    placer: &mut Placer,
) {
    let site = &config.site;
    let mut poles: Vec<Transform> = Vec::new();
    let total = layout.road_total_length();
    let mut s = 4.0;
    let mut k = 0;
    while s < total - 8.0 {
        let (p, dir) = layout.road_at(s);
        // On the outer side of the road, away from the clearing.
        let normal = Vec2::new(-dir.y, dir.x);
        let side = if (p + normal).distance(layout.clearing_center) > p.distance(layout.clearing_center) { 1.0 } else { -1.0 };
        let q = p + normal * side * site.pole_offset;
        let lean = Quat::from_rotation_z(0.012 * if k % 2 == 0 { 1.0 } else { -0.7 });
        poles.push(
            Transform::from_translation(layout.ground_point(q) - Vec3::Y * 0.3)
                .with_rotation(Quat::from_rotation_y(yaw_towards(dir)) * lean)
                .with_scale(Vec3::splat(site.pole_scale)),
        );
        s += site.pole_spacing;
        k += 1;
    }
    // Final pole beside the shed's end wall.
    let (shed_center, _) = layout.shed_footprint();
    let last = layout.yard_world(shed_center + Vec2::new(9.5, 1.0));
    let prev = poles.last().map(|t| t.translation.xz()).unwrap_or(last);
    poles.push(
        Transform::from_translation(layout.ground_point(last) - Vec3::Y * 0.3)
            .with_rotation(Quat::from_rotation_y(yaw_towards((last - prev).normalize_or(Vec2::X))))
            .with_scale(Vec3::splat(site.pole_scale)),
    );

    for (i, pole) in poles.iter().enumerate() {
        let preset = ["pole_01", "pole_02", "pole_03"][if i + 1 == poles.len() { 0 } else { 1 + i % 2 }];
        placer.world(preset, *pole, 45.0, 600.0);
    }
    // Transformer box on the last pole.
    let box_t = poles.last().unwrap().mul_transform(Transform::from_xyz(0.0, 1.35, 0.12));
    placer.world("power_box", Transform { scale: Vec3::ONE, ..box_t }, 25.0, 150.0);

    // Conductors with catenary sag, plus a service drop to the shed roof.
    let mut wire = WireBuilder::default();
    for pair in poles.windows(2) {
        for c in CONDUCTORS {
            let a = pair[0].transform_point(c);
            let b = pair[1].transform_point(c);
            wire.span(a, b, 0.45 + 0.012 * a.distance(b));
        }
    }
    let roof = layout.yard_world(shed_center + Vec2::new(6.2, 0.0));
    let drop_from = poles.last().unwrap().transform_point(CONDUCTORS[1] - Vec3::Y * 0.9);
    wire.span(drop_from, layout.ground_point(roof) + Vec3::Y * 2.9, 0.35);

    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.11, 0.11, 0.12),
        perceptual_roughness: 0.45,
        metallic: 0.6,
        ..default()
    });
    commands.spawn((
        Name::new("Power line conductors"),
        Mesh3d(meshes.add(wire.build())),
        MeshMaterial3d(material),
    ));
}

#[derive(Default)]
struct WireBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl WireBuilder {
    const RADIUS: f32 = 0.016;
    const SIDES: u32 = 4;
    const SEGMENTS: u32 = 18;

    /// Thin tube hanging between a and b with the given mid-span sag (m).
    fn span(&mut self, a: Vec3, b: Vec3, sag: f32) {
        let points: Vec<Vec3> = (0..=Self::SEGMENTS)
            .map(|i| {
                let t = i as f32 / Self::SEGMENTS as f32;
                // Parabolic approximation of the catenary.
                a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t)
            })
            .collect();
        let base = self.positions.len() as u32;
        for (i, p) in points.iter().enumerate() {
            let next = points[(i + 1).min(points.len() - 1)];
            let prev = points[i.saturating_sub(1)];
            let tangent = (next - prev).normalize();
            let side = tangent.cross(Vec3::Y).normalize_or(Vec3::X);
            let up = side.cross(tangent);
            for k in 0..Self::SIDES {
                let angle = k as f32 / Self::SIDES as f32 * std::f32::consts::TAU;
                let n = side * angle.cos() + up * angle.sin();
                self.positions.push((*p + n * Self::RADIUS).to_array());
                self.normals.push(n.to_array());
            }
        }
        for i in 0..Self::SEGMENTS {
            for k in 0..Self::SIDES {
                let a = base + i * Self::SIDES + k;
                let b = base + i * Self::SIDES + (k + 1) % Self::SIDES;
                let c = a + Self::SIDES;
                let d = b + Self::SIDES;
                self.indices.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
    }

    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}
