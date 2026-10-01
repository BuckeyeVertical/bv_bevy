"""Build runtime ground/surface textures.

    blender -b --python tools/blender/export_textures.py

Outputs (assets/environment/materials/):
  terrain_albedo.jpg   5 layers stacked vertically (1024 px each), sRGB
  terrain_normal.jpg   matching OpenGL-convention normal maps, linear
     layer order: 0 meadow grass, 1 dry grass, 2 forest floor, 3 gravel, 4 dirt trail
  asphalt_{diff,nor,rough}.jpg, factory_wall_{diff,nor,rough}.jpg

Bevy loads the stacked images as 2D texture arrays (ImageArrayLayout::RowCount)
and generates mip chains at load time (see proving_ground/mipmaps.rs).
Roughness is not packed for the terrain: ground is uniformly rough, so the shader
uses per-layer constants.  JPEG chroma subsampling would also smear packed data.
"""

import sys
from pathlib import Path

import bpy
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402

RAW = common.raw_root()
REPO_TEX = RAW.parent / "textures"
PF = RAW / "pine_forest" / "textures"
OUT = common.runtime_root() / "materials"
LAYER = 1024

TERRAIN_LAYERS = [
    ("meadow_grass", REPO_TEX / "grass004" / "color.jpg", REPO_TEX / "grass004" / "normal_gl.jpg"),
    ("dry_grass", REPO_TEX / "withered_grass" / "color.jpg", REPO_TEX / "withered_grass" / "normal_gl.jpg"),
    ("forest_floor", PF / "forest_ground_04_diff.png", PF / "forest_ground_04_nor_gl.png"),
    ("gravel", RAW / "gravel_road_4k/textures/gravel_road_diff_4k.jpg",
     RAW / "gravel_road_4k/textures/gravel_road_nor_gl_4k.exr"),
    ("dirt_trail", PF / "rocky_trail_diff.png", PF / "rocky_trail_nor_gl.png"),
]

SURFACES = {
    "asphalt": ("asphalt_floor_4k/textures/asphalt_floor_diff_4k.jpg",
                "asphalt_floor_4k/textures/asphalt_floor_nor_gl_4k.exr",
                "asphalt_floor_4k/textures/asphalt_floor_rough_4k.exr", 2048),
    "factory_wall": ("factory_wall_4k/textures/factory_wall_diff_4k.jpg",
                     "factory_wall_4k/textures/factory_wall_nor_gl_4k.exr",
                     "factory_wall_4k/textures/factory_wall_rough_4k.jpg", 1024),
}


def load_rgb(path, size, srgb):
    """Load any Blender-readable image as float RGB (row 0 = bottom), resized to size."""
    image = bpy.data.images.load(str(path), check_existing=False)
    image.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
    common.downscale_image_in_place(image, size * 2)
    array = common.image_to_array(image)[..., :3].copy()
    if image.is_float and srgb:
        array = common.linear_to_srgb(array)  # float buffers are scene-linear
    bpy.data.images.remove(image)
    array = common.resize_array(array, size, size)
    return np.clip(array, 0.0, 1.0)


def save_jpeg(array_rgb, path, srgb, quality=90):
    h, w, _ = array_rgb.shape
    rgba = np.concatenate([array_rgb, np.ones((h, w, 1), np.float32)], -1)
    image = common.array_to_image(path.stem, rgba, "sRGB" if srgb else "Non-Color")
    path.parent.mkdir(parents=True, exist_ok=True)
    image.filepath_raw = str(path)
    image.file_format = "JPEG"
    bpy.context.scene.render.image_settings.quality = quality
    image.save(quality=quality)
    print(f"wrote {path} {w}x{h} ({path.stat().st_size / 1e6:.1f} MB)")


def main():
    albedo, normal = [], []
    for name, diff, nor in TERRAIN_LAYERS:
        albedo.append(load_rgb(diff, LAYER, True))
        normal.append(load_rgb(nor, LAYER, False))
        print(f"layer {name}: mean albedo {albedo[-1].mean(axis=(0, 1)).round(3)}")
    # Images are stored bottom row first; stack so layer 0 ends up at the top of the file,
    # which is where Bevy's RowCount array layout expects it.
    save_jpeg(np.concatenate(albedo[::-1], 0), OUT / "terrain_albedo.jpg", True)
    save_jpeg(np.concatenate(normal[::-1], 0), OUT / "terrain_normal.jpg", False, quality=95)

    for name, (diff, nor, rough, size) in SURFACES.items():
        save_jpeg(load_rgb(RAW / diff, size, True), OUT / f"{name}_diff.jpg", True)
        save_jpeg(load_rgb(RAW / nor, size, False), OUT / f"{name}_nor.jpg", False, quality=95)
        save_jpeg(load_rgb(RAW / rough, size // 2, False), OUT / f"{name}_rough.jpg", False)


main()
