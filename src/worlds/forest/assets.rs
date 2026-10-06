//! Loading the converted Poly Haven assets and turning them into shared handles.
//!
//! Every GLB is loaded once. Meshes are looked up by name ("pine_01_lod0", ...)
//! and each glTF material becomes one tuned `StandardMaterial`, so thousands of
//! instances share a handful of mesh/material handles and batch well.

use std::collections::HashMap;

use bevy::{
    asset::AssetPath,
    gltf::{Gltf, GltfMesh, GltfNode},
    image::{
        ImageAddressMode, ImageArrayLayout, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    },
    prelude::*,
    render::render_resource::TextureFormat,
    tasks::ComputeTaskPool,
};

pub const TREES: &str = "forest/vegetation/trees.glb";
pub const FOREST_PROPS: &str = "forest/vegetation/forest_props.glb";
pub const GRASS: &str = "forest/vegetation/grass.glb";
pub const SHED: &str = "forest/buildings/shed.glb";
pub const SHED_PROPS: &str = "forest/props/shed_props.glb";
pub const BARREL: &str = "forest/props/barrel_01.glb";
pub const BARRIER: &str = "forest/props/concrete_barrier.glb";
pub const POLES: &str = "forest/props/electricity_poles.glb";

const GLB_FILES: [&str; 8] = [TREES, FOREST_PROPS, GRASS, SHED, SHED_PROPS, BARREL, BARRIER, POLES];

pub const TERRAIN_LAYERS: u32 = 5;

#[derive(Resource)]
pub struct EnvironmentAssets {
    gltfs: Vec<Handle<Gltf>>,
    pub sky: Handle<Image>,
    pub ibl: Handle<Image>,
    pub terrain_albedo: Handle<Image>,
    pub terrain_normal: Handle<Image>,
    pub asphalt: SurfaceTextures,
    pub wall: SurfaceTextures,
}

#[derive(Clone)]
pub struct SurfaceTextures {
    pub diffuse: Handle<Image>,
    pub normal: Handle<Image>,
    pub roughness: Handle<Image>,
}

/// One drawable part of a named mesh.
#[derive(Clone)]
pub struct Part {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct MeshLibrary {
    meshes: HashMap<String, Vec<Part>>,
}

impl MeshLibrary {
    pub fn parts(&self, name: &str) -> &[Part] {
        self.meshes
            .get(name)
            .unwrap_or_else(|| panic!("forest: mesh '{name}' missing from environment GLBs"))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.meshes.contains_key(name)
    }
}

pub fn load(asset_server: &AssetServer, sky: &'static str, ibl: &'static str) -> EnvironmentAssets {
    let surface = |name: &str| SurfaceTextures {
        diffuse: load_repeating(asset_server, format!("forest/materials/{name}_diff.jpg"), true, None),
        normal: load_repeating(asset_server, format!("forest/materials/{name}_nor.jpg"), false, None),
        roughness: load_repeating(asset_server, format!("forest/materials/{name}_rough.jpg"), false, None),
    };
    let array = Some(ImageArrayLayout::RowCount { rows: TERRAIN_LAYERS });
    EnvironmentAssets {
        gltfs: GLB_FILES.iter().map(|path| asset_server.load(*path)).collect(),
        sky: asset_server.load(sky),
        ibl: asset_server.load(ibl),
        terrain_albedo: load_repeating(asset_server, "forest/materials/terrain_albedo.jpg".into(), true, array),
        terrain_normal: load_repeating(asset_server, "forest/materials/terrain_normal.jpg".into(), false, array),
        asphalt: surface("asphalt"),
        wall: surface("factory_wall"),
    }
}

fn load_repeating(
    asset_server: &AssetServer,
    path: String,
    is_srgb: bool,
    array_layout: Option<ImageArrayLayout>,
) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = is_srgb;
            settings.array_layout = array_layout;
            settings.sampler = ImageSampler::Descriptor(repeating_sampler(16));
        })
        .load(path)
}

fn repeating_sampler(anisotropy: u16) -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: anisotropy,
        ..default()
    }
}

impl EnvironmentAssets {
    pub fn all_loaded(&self, asset_server: &AssetServer) -> bool {
        let images = [
            &self.sky,
            &self.ibl,
            &self.terrain_albedo,
            &self.terrain_normal,
            &self.asphalt.diffuse,
            &self.asphalt.normal,
            &self.asphalt.roughness,
            &self.wall.diffuse,
            &self.wall.normal,
            &self.wall.roughness,
        ];
        self.gltfs.iter().all(|h| asset_server.is_loaded_with_dependencies(h))
            && images.iter().all(|h| asset_server.is_loaded_with_dependencies(*h))
    }

    pub fn surface_images(&self) -> Vec<Handle<Image>> {
        vec![
            self.terrain_albedo.clone(),
            self.terrain_normal.clone(),
            self.asphalt.diffuse.clone(),
            self.asphalt.normal.clone(),
            self.asphalt.roughness.clone(),
            self.wall.diffuse.clone(),
            self.wall.normal.clone(),
            self.wall.roughness.clone(),
        ]
    }
}

/// Build the mesh library from the loaded GLBs, tune their materials, and give
/// every texture a mip chain.
pub fn build_library(
    env: &EnvironmentAssets,
    asset_server: &AssetServer,
    gltfs: &Assets<Gltf>,
    gltf_nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> MeshLibrary {
    let mut library = MeshLibrary::default();
    let mut tuned: HashMap<AssetPath<'static>, Handle<StandardMaterial>> = HashMap::new();
    let mut mip_jobs: HashMap<AssetId<Image>, Option<f32>> = HashMap::new();

    for gltf_handle in &env.gltfs {
        let gltf = gltfs.get(gltf_handle).expect("environment GLB loaded");
        // Keyed by node (= Blender object) name: glTF mesh names carry Blender
        // data-block names such as "Mesh.001".
        for (name, node_handle) in &gltf.named_nodes {
            let Some(mesh_handle) = gltf_nodes.get(node_handle).and_then(|node| node.mesh.as_ref()) else {
                continue;
            };
            let gltf_mesh = gltf_meshes.get(mesh_handle).expect("glTF mesh loaded");
            let mut parts = Vec::new();
            for primitive in &gltf_mesh.primitives {
                let Some(material_path) = primitive.material.as_ref().and_then(|m| m.path()) else {
                    continue;
                };
                let material = tuned
                    .entry(material_path.clone_owned())
                    .or_insert_with(|| {
                        let std_path = material_path
                            .clone_owned()
                            .with_label(format!("{}/std", material_path.label().unwrap_or_default()));
                        let source: Handle<StandardMaterial> = asset_server.load(std_path);
                        let mut material = materials
                            .get(&source)
                            .cloned()
                            .unwrap_or_else(|| panic!("material {material_path} not loaded"));
                        tune_material(name, &mut material);
                        let cutoff = match material.alpha_mode {
                            AlphaMode::Mask(cutoff) => Some(cutoff),
                            _ => None,
                        };
                        for (image, is_base) in [
                            (&material.base_color_texture, true),
                            (&material.normal_map_texture, false),
                            (&material.metallic_roughness_texture, false),
                            (&material.occlusion_texture, false),
                        ] {
                            if let Some(image) = image {
                                let entry = mip_jobs.entry(image.id()).or_insert(None);
                                if is_base && cutoff.is_some() {
                                    *entry = cutoff;
                                }
                            }
                        }
                        materials.add(material)
                    })
                    .clone();
                parts.push(Part { mesh: primitive.mesh.clone(), material });
            }
            library.meshes.insert(name.to_string(), parts);
        }
    }

    for image in env.surface_images() {
        mip_jobs.entry(image.id()).or_insert(None);
    }
    generate_mipmaps(images, &mip_jobs);
    library
}

/// Physically sensible defaults for the different asset families.
fn tune_material(mesh_name: &str, material: &mut StandardMaterial) {
    let foliage = matches!(material.alpha_mode, AlphaMode::Mask(_) | AlphaMode::Blend);
    if foliage {
        // Cards, ferns, grass: thin, slightly translucent, never shiny.
        material.alpha_mode = AlphaMode::Mask(0.5);
        material.double_sided = true;
        material.cull_mode = None;
        material.diffuse_transmission = 0.28;
        material.perceptual_roughness = material.perceptual_roughness.max(0.75);
        material.reflectance = 0.3;
    } else {
        material.reflectance = material.reflectance.min(0.5);
    }
    if mesh_name.starts_with("grass_") {
        // The Poly Haven clump texture is a little cold next to the meadow terrain.
        material.base_color = Color::srgb(0.92, 0.95, 0.8);
    }
    if mesh_name.contains("_lod3") {
        // Impostors: no specular sparkle at distance.
        material.diffuse_transmission = 0.15;
        material.reflectance = 0.2;
    }
}

// --------------------------------------------------------------------------- mipmaps

struct MipJob {
    id: AssetId<Image>,
    data: Vec<u8>,
    width: u32,
    height: u32,
    layers: u32,
    srgb: bool,
    cutoff: Option<f32>,
    result: Option<(Vec<u8>, u32)>,
}

/// Bevy does not build mip chains for PNG/JPEG textures; without them foliage
/// and ground shimmer badly at distance. Box-filter in linear space and, for
/// alpha-tested textures, rescale alpha so coverage is preserved at every level.
fn generate_mipmaps(images: &mut Assets<Image>, jobs: &HashMap<AssetId<Image>, Option<f32>>) {
    let mut work: Vec<MipJob> = Vec::new();
    for (&id, &cutoff) in jobs {
        let Some(mut image) = images.get_mut(id) else { continue };
        let format = image.texture_descriptor.format;
        let srgb = match format {
            TextureFormat::Rgba8UnormSrgb => true,
            TextureFormat::Rgba8Unorm => false,
            _ => continue,
        };
        if image.texture_descriptor.mip_level_count > 1 {
            continue;
        }
        let Some(data) = image.data.take() else { continue };
        work.push(MipJob {
            id,
            data,
            width: image.width(),
            height: image.height(),
            layers: image.texture_descriptor.size.depth_or_array_layers,
            srgb,
            cutoff,
            result: None,
        });
    }

    ComputeTaskPool::get().scope(|scope| {
        for job in work.iter_mut() {
            scope.spawn(async move {
                job.result = Some(build_mip_chain(&job.data, job.width, job.height, job.layers, job.srgb, job.cutoff));
            });
        }
    });

    for job in work {
        let Some(mut image) = images.get_mut(job.id) else { continue };
        let (data, levels) = job.result.expect("mip job ran");
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = levels;
        let anisotropy = 8;
        image.sampler = match &image.sampler {
            ImageSampler::Descriptor(d) => ImageSampler::Descriptor(ImageSamplerDescriptor {
                mipmap_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mag_filter: ImageFilterMode::Linear,
                anisotropy_clamp: d.anisotropy_clamp.max(anisotropy),
                ..d.clone()
            }),
            ImageSampler::Default => ImageSampler::Descriptor(repeating_sampler(anisotropy)),
        };
    }
}

fn srgb_to_linear_table() -> [f32; 256] {
    let mut table = [0.0; 256];
    for (i, v) in table.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    }
    table
}

fn linear_to_srgb_u8(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

fn coverage(alpha: &[f32], cutoff: f32, scale: f32) -> f32 {
    alpha.iter().filter(|&&a| (a * scale).min(1.0) >= cutoff).count() as f32 / alpha.len().max(1) as f32
}

/// Returns layer-major data (all mips of layer 0, then layer 1, ...) and the level count.
pub(super) fn build_mip_chain(
    data: &[u8],
    width: u32,
    height: u32,
    layers: u32,
    srgb: bool,
    cutoff: Option<f32>,
) -> (Vec<u8>, u32) {
    let levels = 32 - width.max(height).leading_zeros();
    let to_linear = srgb_to_linear_table();
    let layer_bytes = (width * height * 4) as usize;
    let mut out = Vec::with_capacity(layer_bytes * layers as usize * 4 / 3 + 64);

    for layer in 0..layers as usize {
        let src = &data[layer * layer_bytes..(layer + 1) * layer_bytes];
        out.extend_from_slice(src);
        // Work in linear float.
        let mut cur: Vec<[f32; 4]> = src
            .chunks_exact(4)
            .map(|p| {
                let c = |v: u8| if srgb { to_linear[v as usize] } else { v as f32 / 255.0 };
                [c(p[0]), c(p[1]), c(p[2]), p[3] as f32 / 255.0]
            })
            .collect();
        let target = cutoff.map(|c| {
            let alpha: Vec<f32> = cur.iter().map(|p| p[3]).collect();
            coverage(&alpha, c, 1.0)
        });
        let (mut w, mut h) = (width as usize, height as usize);
        for _ in 1..levels {
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![[0.0f32; 4]; nw * nh];
            for y in 0..nh {
                for x in 0..nw {
                    let mut acc = [0.0f32; 4];
                    let mut weight = 0.0;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        let p = cur[sy * w + sx];
                        // Alpha-weight colour so transparent texels do not bleed in.
                        let a = if cutoff.is_some() { p[3].max(1e-3) } else { 1.0 };
                        for c in 0..3 {
                            acc[c] += p[c] * a;
                        }
                        acc[3] += p[3];
                        weight += a;
                    }
                    next[y * nw + x] = [acc[0] / weight, acc[1] / weight, acc[2] / weight, acc[3] * 0.25];
                }
            }
            if let (Some(cutoff), Some(target)) = (cutoff, target) {
                let alpha: Vec<f32> = next.iter().map(|p| p[3]).collect();
                let (mut lo, mut hi) = (0.0f32, 4.0f32);
                for _ in 0..12 {
                    let mid = 0.5 * (lo + hi);
                    if coverage(&alpha, cutoff, mid) < target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                for p in next.iter_mut() {
                    p[3] = (p[3] * hi).min(1.0);
                }
            }
            for p in &next {
                if srgb {
                    out.extend_from_slice(&[
                        linear_to_srgb_u8(p[0]),
                        linear_to_srgb_u8(p[1]),
                        linear_to_srgb_u8(p[2]),
                        (p[3] * 255.0 + 0.5) as u8,
                    ]);
                } else {
                    out.extend(p.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
                }
            }
            cur = next;
            w = nw;
            h = nh;
        }
    }
    (out, levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_has_expected_size() {
        let data = vec![200u8; 8 * 4 * 4 * 2];
        let (out, levels) = build_mip_chain(&data, 8, 4, 2, true, None);
        assert_eq!(levels, 4);
        // per layer: 8x4 + 4x2 + 2x1 + 1x1 texels
        assert_eq!(out.len(), (32 + 8 + 2 + 1) * 4 * 2);
        assert!(out.iter().all(|&v| v.abs_diff(200) <= 1));
    }

    #[test]
    fn alpha_coverage_is_preserved() {
        // Checkerboard of opaque/transparent texels: 50% coverage at every level.
        let mut data = Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let a = if (x + y) % 2 == 0 { 255 } else { 0 };
                data.extend_from_slice(&[10, 120, 10, a]);
            }
        }
        let (out, _) = build_mip_chain(&data, 16, 16, 1, true, Some(0.5));
        let level1 = &out[16 * 16 * 4..16 * 16 * 4 + 8 * 8 * 4];
        let covered = level1.chunks_exact(4).filter(|p| p[3] >= 128).count();
        assert!(covered >= 16, "coverage collapsed: {covered}/64");
    }
}
