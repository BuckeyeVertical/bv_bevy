"""Convert an equirectangular HDRI into a Bevy-ready KTX2 cubemap.

Run with Blender's Python (it ships numpy and can read .hdr/.exr):

    blender -b --python tools/blender/hdri_to_cubemap.py -- \
        assets/raw/sky/meadow_2_4k.hdr assets/environment/sky/meadow_2 --face 1024

Outputs:
  <out>_cubemap.ktx2  RGB9E5 cube, full mip chain, zstd supercompressed (skybox).
  <out>_ibl.ktx2      same at 128 px for GeneratedEnvironmentMapLight.
                      The sun is clamped out so a DirectionalLight can supply it
                      without double-counting it in image-based lighting.
  <out>.json          Sun direction (Bevy world space, pointing *towards* the sun),
                      mean sky radiance and the clamp value used.

Orientation: the image centre maps to Bevy -Z and image-right maps to +X, so the
sky looks the same as standing in the HDRI facing its centre.  Bevy samples cube
maps with z negated (see skybox.wgsl), which this script accounts for.
"""

import json
import math
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import bpy
import numpy as np

KTX2_IDENTIFIER = bytes([0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A])
VK_FORMAT_E5B9G9R9_UFLOAT_PACK32 = 123
SUPERCOMPRESSION_ZSTD = 2


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    face = 1024
    if "--face" in argv:
        i = argv.index("--face")
        face = int(argv[i + 1])
        del argv[i : i + 2]
    return Path(argv[0]), Path(argv[1]), face


def load_equirect(path):
    image = bpy.data.images.load(str(path.resolve()))
    width, height = image.size
    pixels = np.empty(width * height * 4, dtype=np.float32)
    image.pixels.foreach_get(pixels)
    # Blender stores rows bottom-to-top; flip so row 0 is the top (zenith).
    return pixels.reshape(height, width, 4)[::-1, :, :3].copy()


def world_to_equirect_uv(d):
    """Bevy world direction (Y up) -> equirect uv in [0,1], v=0 at zenith."""
    u = 0.5 + np.arctan2(d[..., 0], -d[..., 2]) / (2.0 * math.pi)
    v = np.arccos(np.clip(d[..., 1], -1.0, 1.0)) / math.pi
    return u, v


def equirect_uv_to_world(u, v):
    phi = (u - 0.5) * 2.0 * math.pi
    theta = v * math.pi
    return np.array([math.sin(theta) * math.sin(phi), math.cos(theta), -math.sin(theta) * math.cos(phi)])


def sample_bilinear(img, u, v):
    h, w, _ = img.shape
    x = (u * w - 0.5) % w
    y = np.clip(v * h - 0.5, 0, h - 1)
    x0 = np.floor(x).astype(np.int64)
    y0 = np.floor(y).astype(np.int64)
    x1 = (x0 + 1) % w
    y1 = np.minimum(y0 + 1, h - 1)
    fx = (x - x0)[..., None]
    fy = (y - y0)[..., None]
    top = img[y0, x0] * (1 - fx) + img[y0, x1] * fx
    bottom = img[y1, x0] * (1 - fx) + img[y1, x1] * fx
    return top * (1 - fy) + bottom * fy


def face_directions(face_index, size):
    """Cube-space direction per texel using the D3D/Vulkan/wgpu face convention."""
    coords = (np.arange(size) + 0.5) / size * 2.0 - 1.0
    sc, tc = np.meshgrid(coords, coords)  # tc grows downwards (row index)
    one = np.ones_like(sc)
    if face_index == 0:    # +X
        d = np.stack([one, -tc, -sc], -1)
    elif face_index == 1:  # -X
        d = np.stack([-one, -tc, sc], -1)
    elif face_index == 2:  # +Y
        d = np.stack([sc, one, tc], -1)
    elif face_index == 3:  # -Y
        d = np.stack([sc, -one, -tc], -1)
    elif face_index == 4:  # +Z
        d = np.stack([sc, -tc, one], -1)
    else:                  # -Z
        d = np.stack([-sc, -tc, -one], -1)
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


def find_sun(img):
    h, w, _ = img.shape
    lum = img @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    upper = lum[: h // 2]
    # Centroid of the brightest texels gives a stable sun position.
    threshold = np.percentile(upper, 99.995)
    ys, xs = np.nonzero(upper >= threshold)
    weights = upper[ys, xs]
    # Average on the unit sphere to handle wrap-around in u.
    dirs = np.array([equirect_uv_to_world((x + 0.5) / w, (y + 0.5) / h) for x, y in zip(xs, ys)])
    sun = (dirs * weights[:, None]).sum(0)
    sun /= np.linalg.norm(sun)
    return sun, float(lum.max()), lum


def box_blur(a, r):
    """Separable box blur (wraps horizontally) of a 2D array."""
    k = 2 * r + 1
    c = np.cumsum(np.pad(a, ((0, 0), (r + 1, r)), mode="wrap"), axis=1)
    a = (c[:, k:] - c[:, :-k]) / k
    c = np.cumsum(np.pad(a, ((r + 1, r), (0, 0)), mode="edge"), axis=0)
    return (c[k:] - c[:-k]) / k


def build_sky(img, sun_dir):
    """Replace the upper hemisphere with a smooth sky fitted to the HDRI.

    Meadow 2 is a cloudless sky framed by photographed trees (one reaches ~58
    deg). Patching around them leaves seams, so the skybox becomes: per-elevation
    sky colour (median of clean sky texels) x the HDRI's measured sun-halo
    profile (median colour ratio by angle from the sun). Near the horizon it
    fades into haze, and a band below the horizon continues the haze so the sky
    meets the fogged far terrain cleanly.
    """
    h, w, _ = img.shape
    horizon = h // 2
    lum = img @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    sky_ref = np.percentile(lum[: h // 6], 50)
    blueness = img[..., 2] / np.maximum(img[..., 1], 1e-4)
    sky = ((lum > 0.55 * sky_ref) & (blueness > 1.05)).astype(np.float32)
    sky[horizon:] = 0.0
    sky = box_blur(sky, 10) > 0.995  # erode away leaf edges

    # Angle of every texel from the sun.
    v = (np.arange(h) + 0.5) / h
    u = (np.arange(w) + 0.5) / w
    theta = v * math.pi
    phi = (u - 0.5) * 2.0 * math.pi
    st = np.sin(theta)[:, None]
    dirs = np.stack([st * np.sin(phi)[None, :], np.repeat(np.cos(theta)[:, None], w, 1), -st * np.cos(phi)[None, :]], -1)
    sun_angle = np.degrees(np.arccos(np.clip(dirs @ sun_dir.astype(np.float32), -1, 1)))

    # Row model from texels well away from the sun.
    far_from_sun = sky & (sun_angle > 50)
    model = np.zeros((h, 3), np.float32)
    last = None
    for y in range(horizon):
        m = far_from_sun[y]
        if m.sum() > 16:
            last = np.median(img[y, m], axis=0)
        if last is not None:
            model[y] = last
    first = next(y for y in range(horizon) if model[y].any())
    model[:first] = model[first]
    # Row medians are noisy; smooth across elevation to avoid banding.
    k = 25
    padded = np.pad(model[:horizon], ((k, k), (0, 0)), mode="edge")
    c = np.cumsum(np.vstack([np.zeros((1, 3), np.float32), padded]), axis=0)
    model[:horizon] = (c[2 * k + 1 :] - c[: -2 * k - 1]) / (2 * k + 1)

    # Sun halo: per-channel ratio to the row model, binned by angle.
    bins = np.arange(0, 91)
    ratio = np.ones((len(bins), 3), np.float32)
    ys, xs = np.nonzero(sky)
    a = sun_angle[ys, xs]
    r = img[ys, xs] / np.maximum(model[ys], 1e-4)
    for i in range(len(bins) - 1):
        sel = (a >= bins[i]) & (a < bins[i + 1])
        if sel.sum() > 32:
            ratio[i] = np.median(r[sel], axis=0)
        elif i > 0:
            ratio[i] = ratio[i - 1]
    ratio = np.maximum(ratio, 1.0)
    for _ in range(3):  # smooth
        ratio[1:-1] = (ratio[:-2] + ratio[1:-1] + ratio[2:]) / 3
    glow = ratio[np.clip(sun_angle.astype(np.int64), 0, 90)]

    haze = model[int(horizon * 0.75)] * 0.4 + np.array([0.86, 0.89, 0.93], np.float32) * float(sky_ref) * 1.15
    elev = 1.0 - np.arange(horizon) / horizon
    t = (np.clip(1.0 - elev / 0.2, 0.0, 1.0) ** 1.6)[:, None]
    rows = model[:horizon] * (1 - t) + haze * t

    out = img.copy()
    out[:horizon] = rows[:, None, :] * glow[:horizon]
    # Below the horizon: haze down to ~-15 deg (seen past the terrain's far
    # edge from altitude), then a darkened blend into the photographed ground.
    band = h // 12
    out[horizon : horizon + band] = haze * glow[horizon : horizon + band] ** 0.5
    fade = h // 12
    k = (np.arange(fade) / fade)[:, None, None]
    lo = horizon + band
    out[lo : lo + fade] = haze * (1 - k) + img[lo : lo + fade] * k
    print(f"sky model: zenith {model[0].round(3)}, haze {haze.round(3)}, halo peak x{ratio[3].round(2)}")
    return out


def downsample(face):
    return 0.25 * (face[0::2, 0::2] + face[1::2, 0::2] + face[0::2, 1::2] + face[1::2, 1::2])


def pack_rgb9e5(rgb):
    """Pack float RGB into the shared-exponent E5B9G9R9 format (KTX/GL spec algorithm)."""
    n, b, e_max = 9, 15, 31
    max_value = (2**n - 1) / 2**n * 2 ** (e_max - b)
    c = np.clip(rgb.astype(np.float64), 0.0, max_value)
    max_c = c.max(-1)
    exp_p = np.maximum(-b - 1, np.floor(np.log2(np.maximum(max_c, 1e-30)))) + 1 + b
    max_s = np.floor(max_c / 2.0 ** (exp_p - b - n) + 0.5)
    exp = np.where(max_s >= 2**n, exp_p + 1, exp_p)
    scale = 2.0 ** (exp - b - n)
    mant = np.floor(c / scale[..., None] + 0.5).astype(np.uint32)
    return (mant[..., 0] | (mant[..., 1] << 9) | (mant[..., 2] << 18) | (exp.astype(np.uint32) << 27)).astype("<u4")


def zstd(data):
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "level.bin"
        src.write_bytes(data)
        subprocess.run(["zstd", "-q", "-19", "-f", str(src), "-o", str(src) + ".zst"], check=True)
        return (Path(tmp) / "level.bin.zst").read_bytes()


def write_ktx2(path, levels_faces, size):
    """levels_faces: list (per mip) of list of 6 float32 HxWx3 arrays."""
    level_count = len(levels_faces)
    raw_levels = []
    for faces in levels_faces:
        chunk = bytearray()
        for face in faces:
            chunk += pack_rgb9e5(face).tobytes()
        raw_levels.append(bytes(chunk))
    compressed = [zstd(level) for level in raw_levels]

    # Basic data format descriptor for E5B9G9R9 (KDF 1.3): 3 mantissas + 3 exponent samples.
    samples = b""
    for channel in range(3):
        samples += struct.pack("<HBBI", channel * 9, 8, channel, 0) + struct.pack("<II", 0, 8448)
    for channel in range(3):
        samples += struct.pack("<HBBI", 27, 4, channel | 0x20, 0) + struct.pack("<II", 15, 31)
    descriptor_block_size = 24 + len(samples)
    dfd_body = struct.pack(
        "<IHHBBBBBBBB8B",
        0,  # vendorId(17) | descriptorType(15)
        2,  # versionNumber
        descriptor_block_size,
        1,  # colorModel RGBSDA
        1,  # colorPrimaries BT709
        1,  # transferFunction linear
        0,  # flags
        0, 0, 0, 0,  # texelBlockDimension
        4, 0, 0, 0, 0, 0, 0, 0,  # bytesPlane
    ) + samples
    dfd = struct.pack("<I", 4 + len(dfd_body)) + dfd_body

    header_size = 12 + 4 * 9 + 4 * 4 + 8 * 2
    level_index_size = 24 * level_count
    dfd_offset = header_size + level_index_size
    data_start = dfd_offset + len(dfd)
    data_start += (-data_start) % 8

    # Level data is stored smallest mip first.
    offsets = [0] * level_count
    cursor = data_start
    for level in reversed(range(level_count)):
        offsets[level] = cursor
        cursor += len(compressed[level])

    out = bytearray()
    out += KTX2_IDENTIFIER
    out += struct.pack(
        "<9I",
        VK_FORMAT_E5B9G9R9_UFLOAT_PACK32,
        4,  # typeSize
        size,
        size,
        0,  # pixelDepth
        0,  # layerCount
        6,  # faceCount
        level_count,
        SUPERCOMPRESSION_ZSTD,
    )
    out += struct.pack("<4I", dfd_offset, len(dfd), 0, 0)  # dfd, kvd
    out += struct.pack("<2Q", 0, 0)  # sgd
    for level in range(level_count):
        out += struct.pack("<3Q", offsets[level], len(compressed[level]), len(raw_levels[level]))
    out += dfd
    out += b"\0" * (data_start - len(out))
    for level in reversed(range(level_count)):
        out += compressed[level]
    path.write_bytes(bytes(out))


def main():
    src, out_prefix, size = parse_args()
    out_prefix.parent.mkdir(parents=True, exist_ok=True)
    img = load_equirect(src)
    h, w, _ = img.shape
    print(f"loaded {src} {w}x{h}")

    sun_dir, peak, lum = find_sun(img)
    sky = lum[: int(h * 0.45)]
    sky_mean = float(sky.mean())
    sky_p99 = float(np.percentile(sky, 99.0))
    # Clamp everything brighter than the brightest clouds: removes the sun disc
    # and its hot halo from the IBL while keeping cloud detail.
    clamp = max(sky_p99 * 1.5, 1.0)
    elevation = math.degrees(math.asin(sun_dir[1]))
    azimuth = math.degrees(math.atan2(sun_dir[0], -sun_dir[2]))
    print(f"sun dir {sun_dir} elevation {elevation:.1f} deg azimuth(from -Z towards +X) {azimuth:.1f} deg")
    print(f"peak {peak:.1f} sky mean {sky_mean:.3f} sky p99 {sky_p99:.3f} clamp {clamp:.3f}")

    # Meadow 2 has real trees around its horizon that would float above our own
    # forest; replace the upper hemisphere with a fitted sky (see build_sky).
    img = build_sky(img, sun_dir)
    # The photographed sky is very saturated; at full strength it tints every
    # shaded surface (foliage especially) blue. Pull saturation back a little.
    luma = (img @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32))[..., None]
    img = luma + (img - luma) * 0.9

    clamped = img.copy()
    clamped_lum = lum[..., None]
    scale = np.minimum(1.0, clamp / np.maximum(clamped_lum, 1e-6))
    clamped *= scale

    # The terrain hides everything below the horizon, so replace the HDRI ground with a
    # heavily blurred copy: it keeps the right bounce-light colour for IBL and the
    # cubemap compresses far better without grass-level noise.
    blurred = clamped
    while blurred.shape[0] > 32:
        blurred = 0.25 * (blurred[0::2, 0::2] + blurred[1::2, 0::2] + blurred[0::2, 1::2] + blurred[1::2, 1::2])

    write_cubemap(out_prefix.with_name(out_prefix.name + "_cubemap.ktx2"), clamped, blurred, size)
    # Bevy re-filters GeneratedEnvironmentMapLight every frame, so lighting gets
    # a small cubemap of its own; the skybox keeps the full-resolution one.
    write_cubemap(out_prefix.with_name(out_prefix.name + "_ibl.ktx2"), clamped, blurred, 128)
    meta = {
        "source": src.name,
        "face_size": size,
        "sun_direction": [float(c) for c in sun_dir],
        "sun_elevation_deg": elevation,
        "sun_azimuth_deg": azimuth,
        "sky_mean_radiance": sky_mean,
        "ibl_clamp": clamp,
    }
    out_prefix.with_suffix(".json").write_text(json.dumps(meta, indent=2))


def write_cubemap(path, clamped, blurred, size):
    faces = []
    for index in range(6):
        d_cube = face_directions(index, size)
        d_world = d_cube * np.array([1.0, 1.0, -1.0])
        u, v = world_to_equirect_uv(d_world)
        sharp = sample_bilinear(clamped, u, v)
        soft = sample_bilinear(blurred, u, v)
        t = np.clip((-d_world[..., 1] - 0.3) / 0.15, 0.0, 1.0)[..., None]
        t = t * t * (3.0 - 2.0 * t)
        faces.append((sharp * (1.0 - t) + soft * t).astype(np.float32))

    levels = [faces]
    while levels[-1][0].shape[0] > 1:
        levels.append([downsample(f) for f in levels[-1]])

    write_ktx2(path, levels, size)
    print(f"wrote {path} ({path.stat().st_size / 1e6:.1f} MB)")


main()
