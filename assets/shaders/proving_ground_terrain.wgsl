// Proving-ground terrain: five ground layers blended by per-vertex splat weights.
//
// Vertex colour = layer weights (r dry/worn grass, g forest floor, b gravel, a dirt);
// meadow grass takes the remainder. UV0 = (canopy occlusion, mown mask).
// Layers live in two texture arrays (albedo, OpenGL normal) in this order:
//   0 meadow grass, 1 dry grass, 2 forest floor, 3 gravel, 4 dirt trail.
//
// Repetition is hidden by blending each layer with a rotated, 2.7x larger copy
// of itself under low-frequency noise, plus macro brightness/hue variation.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

struct TerrainParams {
    // metres per texture tile, layers 0-3 in [0], layer 4 in [1].x
    layer_scale: array<vec4<f32>, 2>,
    // rgb tint, a = perceptual roughness
    layer_tint: array<vec4<f32>, 5>,
    // x: macro noise frequency (1/m), y: macro brightness variation,
    // z: mowing stripe width (m), w: stripe strength
    macro_params: vec4<f32>,
    // x: normal strength, y: height-blend contrast, z/w: far canopy tint start/full (m)
    blend_params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var albedo_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var normal_sampler: sampler;

const LAYERS: i32 = 5;
// Rotation used for the second, larger sample (~37 degrees).
const ROT: mat2x2<f32> = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash12(i);
    let b = hash12(i + vec2<f32>(1.0, 0.0));
    let c = hash12(i + vec2<f32>(0.0, 1.0));
    let d = hash12(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += amp * value_noise(q);
        q = q * 2.03 + vec2<f32>(17.1, 9.2);
        amp *= 0.5;
    }
    return sum / 0.9375;
}

fn layer_scale(i: i32) -> f32 {
    return terrain.layer_scale[i / 4][i % 4];
}

struct LayerSample {
    albedo: vec3<f32>,
    normal: vec2<f32>,
    height: f32,
}

fn sample_layer(i: i32, wp: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, mix_t: f32) -> LayerSample {
    let s_a = 1.0 / layer_scale(i);
    let s_b = s_a * 0.37;
    let uv_a = wp * s_a;
    let uv_b = (ROT * wp) * s_b + vec2<f32>(0.31, 0.77);
    let dxa = dx * s_a;
    let dya = dy * s_a;
    let dxb = (ROT * dx) * s_b;
    let dyb = (ROT * dy) * s_b;

    let ca = textureSampleGrad(albedo_array, albedo_sampler, uv_a, i, dxa, dya).rgb;
    let cb = textureSampleGrad(albedo_array, albedo_sampler, uv_b, i, dxb, dyb).rgb;
    let na = textureSampleGrad(normal_array, normal_sampler, uv_a, i, dxa, dya).xy * 2.0 - 1.0;
    let nb_raw = textureSampleGrad(normal_array, normal_sampler, uv_b, i, dxb, dyb).xy * 2.0 - 1.0;
    // Bring the rotated sample's tangent-space xy back into the world-aligned frame.
    // Tangent x runs along +u, tangent y along -v (OpenGL normal maps).
    let nb_wp = transpose(ROT) * vec2<f32>(nb_raw.x, -nb_raw.y);
    let nb = vec2<f32>(nb_wp.x, -nb_wp.y);

    var out: LayerSample;
    out.albedo = mix(ca, cb, mix_t) * terrain.layer_tint[i].rgb;
    out.normal = mix(na, nb, mix_t);
    out.height = sqrt(dot(mix(ca, cb, mix_t), vec3<f32>(0.2126, 0.7152, 0.0722)));
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let wp = in.world_position.xz;
    let dx = dpdx(wp);
    let dy = dpdy(wp);

#ifdef VERTEX_COLORS
    let c = clamp(in.color, vec4<f32>(0.0), vec4<f32>(1.0));
#else
    let c = vec4<f32>(0.0);
#endif
#ifdef VERTEX_UVS_A
    let canopy = in.uv.x;
    let mown = in.uv.y;
#else
    let canopy = 1.0;
    let mown = 0.0;
#endif

    var w = array<f32, 5>(max(1.0 - c.r - c.g - c.b - c.a, 0.0), c.r, c.g, c.b, c.a);
    let mix_t = 0.15 + 0.7 * smoothstep(0.3, 0.7, value_noise(wp * 0.045 + vec2<f32>(3.1, 7.7)));

    // Height-aware blending: stones and clumps poke through the neighbouring layer.
    var samples: array<LayerSample, 5>;
    var score = array<f32, 5>(-10.0, -10.0, -10.0, -10.0, -10.0);
    var best = -10.0;
    for (var i = 0; i < LAYERS; i++) {
        if w[i] > 0.004 {
            samples[i] = sample_layer(i, wp, dx, dy, mix_t);
            score[i] = w[i] + samples[i].height * terrain.blend_params.y;
            best = max(best, score[i]);
        }
    }
    var albedo = vec3<f32>(0.0);
    var nxy = vec2<f32>(0.0);
    var roughness = 0.0;
    var total = 0.0;
    var grass = 0.0;
    for (var i = 0; i < LAYERS; i++) {
        let b = max(score[i] - best + 0.22, 0.0);
        if b > 0.0 {
            albedo += samples[i].albedo * b;
            nxy += samples[i].normal * b;
            roughness += terrain.layer_tint[i].a * b;
            total += b;
            if i == 0 {
                grass = b;
            }
        }
    }
    albedo /= max(total, 1e-4);
    nxy /= max(total, 1e-4);
    roughness /= max(total, 1e-4);
    grass /= max(total, 1e-4);

    // Macro variation: broad brightness and slight green/yellow shifts.
    let macro_n = fbm(wp * terrain.macro_params.x);
    albedo *= 1.0 + (macro_n - 0.5) * 2.0 * terrain.macro_params.y;
    let hue_n = value_noise(wp * terrain.macro_params.x * 2.7 + vec2<f32>(41.0, 13.0));
    albedo = mix(albedo, albedo * vec3<f32>(1.08, 1.02, 0.78), grass * smoothstep(0.55, 0.9, hue_n) * 0.6);

    // Mowing stripes in the maintained part of the field.
    if mown > 0.0 {
        let stripe = smoothstep(-0.35, 0.35, sin(wp.x * 3.14159 / terrain.macro_params.z));
        albedo *= 1.0 + terrain.macro_params.w * (stripe * 2.0 - 1.0) * mown * grass;
    }

    // Canopy occlusion: ground under trees is darker and gets less sky light.
    albedo *= mix(0.78, 1.0, canopy);

    // Far ground (beyond the forest ring) tends towards canopy colour.
    let far = smoothstep(terrain.blend_params.z, terrain.blend_params.w, max(abs(wp.x), abs(wp.y)));
    albedo = mix(albedo, vec3<f32>(0.035, 0.05, 0.03), far);

    // Normal: world-aligned tangent frame (u = +X, image-up = -Z) on the geometric normal.
    let n_ts = normalize(vec3<f32>(nxy * terrain.blend_params.x, 1.0));
    let ng = normalize(in.world_normal);
    let t = normalize(vec3<f32>(1.0, 0.0, 0.0) - ng * ng.x);
    let bt = cross(ng, t);
    pbr_input.N = normalize(t * n_ts.x + bt * n_ts.y + ng * n_ts.z);

    pbr_input.material.base_color = vec4<f32>(albedo, 1.0);
    pbr_input.material.perceptual_roughness = roughness;
    pbr_input.diffuse_occlusion = vec3<f32>(mix(0.55, 1.0, canopy));

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
