"""Shared helpers for the Blender export scripts (run inside Blender's Python)."""

import math
import sys
from pathlib import Path

import bpy
import numpy as np


def script_args():
    return sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []


def raw_root():
    return Path(__file__).resolve().parents[2] / "assets" / "raw"


def runtime_root():
    return Path(__file__).resolve().parents[2] / "assets" / "environment"


# --------------------------------------------------------------------------- images


def image_to_array(image):
    """Float RGBA array, row 0 = bottom (Blender convention)."""
    w, h = image.size
    pixels = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return pixels.reshape(h, w, 4)


def array_to_image(name, array, colorspace="sRGB"):
    """Create a byte image from a float RGBA array (row 0 = bottom)."""
    h, w, _ = array.shape
    image = bpy.data.images.new(name, w, h, alpha=True, float_buffer=False)
    image.colorspace_settings.name = colorspace
    image.pixels.foreach_set(np.ascontiguousarray(array, dtype=np.float32).ravel())
    return image


def save_image(image, path, file_format="PNG"):
    """Write the image buffer as-is (no view transform) and return a file-backed image."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    image.filepath_raw = str(path)
    image.file_format = file_format
    image.save()
    colorspace = image.colorspace_settings.name
    loaded = bpy.data.images.load(str(path), check_existing=False)
    loaded.colorspace_settings.name = colorspace
    return loaded


def resize_array(array, width, height):
    """Area/bilinear resize of a float image array to (height, width)."""
    h, w, c = array.shape
    # Box-downsample by powers of two first for quality, then bilinear.
    while w >= width * 2 and h >= height * 2 and w % 2 == 0 and h % 2 == 0:
        array = 0.25 * (array[0::2, 0::2] + array[1::2, 0::2] + array[0::2, 1::2] + array[1::2, 1::2])
        h, w, c = array.shape
    if (w, h) == (width, height):
        return array
    ys = (np.arange(height) + 0.5) * h / height - 0.5
    xs = (np.arange(width) + 0.5) * w / width - 0.5
    y0 = np.clip(np.floor(ys).astype(int), 0, h - 1)
    x0 = np.clip(np.floor(xs).astype(int), 0, w - 1)
    y1 = np.clip(y0 + 1, 0, h - 1)
    x1 = np.clip(x0 + 1, 0, w - 1)
    fy = np.clip(ys - y0, 0, 1)[:, None, None]
    fx = np.clip(xs - x0, 0, 1)[None, :, None]
    top = array[y0][:, x0] * (1 - fx) + array[y0][:, x1] * fx
    bottom = array[y1][:, x0] * (1 - fx) + array[y1][:, x1] * fx
    return top * (1 - fy) + bottom * fy


def dilate_rgb(array, alpha_threshold=0.05):
    """Push-pull fill of colour into transparent texels to avoid dark mip fringes."""
    rgb = array[..., :3]
    a = (array[..., 3:4] > alpha_threshold).astype(np.float32)
    levels = [(rgb * a, a)]
    while min(levels[-1][1].shape[:2]) > 1:
        c, w = levels[-1]
        h2, w2 = c.shape[0] // 2 * 2, c.shape[1] // 2 * 2
        c, w = c[:h2, :w2], w[:h2, :w2]
        c = c[0::2, 0::2] + c[1::2, 0::2] + c[0::2, 1::2] + c[1::2, 1::2]
        w = w[0::2, 0::2] + w[1::2, 0::2] + w[0::2, 1::2] + w[1::2, 1::2]
        levels.append((c, w))
    filled = levels[-1][0] / np.maximum(levels[-1][1], 1e-6)
    for c, w in reversed(levels[:-1]):
        up = np.repeat(np.repeat(filled, 2, 0), 2, 1)
        pad_y, pad_x = c.shape[0] - up.shape[0], c.shape[1] - up.shape[1]
        if pad_y > 0 or pad_x > 0:
            up = np.pad(up, ((0, max(pad_y, 0)), (0, max(pad_x, 0)), (0, 0)), mode="edge")
        up = up[: c.shape[0], : c.shape[1]]
        own = c / np.maximum(w, 1e-6)
        filled = np.where(w > 0, own, up)
    out = array.copy()
    out[..., :3] = np.where(a > 0, rgb, filled)
    return out


def linear_to_srgb(x):
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def downscale_image_in_place(image, max_size):
    """Scale a Blender image so its longest side is <= max_size (keeps aspect)."""
    if image is None:
        return
    if image.size[0] == 0:
        try:
            image.pixels[0]  # force a lazy load
        except IndexError:
            return
    w, h = image.size
    if max(w, h) <= max_size:
        return
    scale = max_size / max(w, h)
    image.scale(max(1, int(round(w * scale))), max(1, int(round(h * scale))))


_downscaled = {}


def downscaled_copy(image, max_size, out_dir):
    """File-backed copy of `image` with its longest side <= max_size.

    The glTF exporter re-reads unmodified file images from disk, so an in-memory
    `Image.scale()` alone is not enough: write the reduced image to its own file.
    """
    if image is None:
        return None
    key = (image.name, max_size)
    if key in _downscaled:
        return _downscaled[key]
    work = image.copy()
    downscale_image_in_place(work, max_size)
    array = image_to_array(work)
    colorspace = image.colorspace_settings.name
    if work.is_float and colorspace == "sRGB":
        array = array.copy()
        array[..., :3] = linear_to_srgb(array[..., :3])
    bpy.data.images.remove(work)
    stem = Path(image.filepath or image.name).stem.replace(".", "_")
    out_colorspace = "sRGB" if colorspace == "sRGB" else "Non-Color"
    result = save_image(
        array_to_image(f"{stem}_{max_size}", array, out_colorspace),
        Path(out_dir) / f"{stem}_{max_size}.png",
    )
    _downscaled[key] = result
    return result


_realize_group = None


def realize_group():
    """Geometry-node group that realises all instances (appended as a last modifier)."""
    global _realize_group
    if _realize_group is None:
        group = bpy.data.node_groups.new("realize_all", "GeometryNodeTree")
        group.interface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
        group.interface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
        gin = group.nodes.new("NodeGroupInput")
        gout = group.nodes.new("NodeGroupOutput")
        realize = group.nodes.new("GeometryNodeRealizeInstances")
        group.links.new(gin.outputs[0], realize.inputs[0])
        group.links.new(realize.outputs[0], gout.inputs[0])
        _realize_group = group
    return _realize_group


def add_realize_modifier(obj):
    if any(m.type == "NODES" for m in getattr(obj, "modifiers", [])):
        mod = obj.modifiers.new("realize_all", "NODES")
        mod.node_group = realize_group()


# --------------------------------------------------------------------------- materials


def _tex(nodes, image, colorspace, x, y, uv_socket=None, links=None):
    node = nodes.new("ShaderNodeTexImage")
    node.image = image
    node.location = (x, y)
    if image is not None:
        image.colorspace_settings.name = colorspace
    if uv_socket is not None and links is not None:
        links.new(uv_socket, node.inputs["Vector"])
    return node


def load_image(path):
    path = Path(path)
    for image in bpy.data.images:
        if image.filepath and Path(bpy.path.abspath(image.filepath)).resolve() == path.resolve():
            return image
    return bpy.data.images.load(str(path), check_existing=True)


def pbr_material(
    name,
    base_color=None,
    roughness=None,
    normal=None,
    metallic=None,
    alpha=None,
    base_color_has_alpha=False,
    roughness_value=0.8,
    metallic_value=0.0,
    tint=None,
    alpha_clip=False,
    double_sided=False,
    uv_map="UVMap",
):
    """A clean Principled material the glTF exporter maps 1:1 onto glTF PBR."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    out.location = (400, 0)
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(bsdf.outputs["BSDF"], out.inputs["Surface"])
    uv = nodes.new("ShaderNodeUVMap")
    uv.uv_map = uv_map
    uv.location = (-900, 0)
    uv_out = uv.outputs["UV"]
    bsdf.inputs["Roughness"].default_value = roughness_value
    bsdf.inputs["Metallic"].default_value = metallic_value
    if base_color is not None:
        tex = _tex(nodes, base_color, "sRGB", -500, 300, uv_out, links)
        if tint is not None:
            mix = nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs["Factor"].default_value = 1.0
            links.new(tex.outputs["Color"], mix.inputs["A"])
            mix.inputs["B"].default_value = (*tint, 1.0)
            links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
        else:
            links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
        if base_color_has_alpha:
            links.new(tex.outputs["Alpha"], bsdf.inputs["Alpha"])
    if alpha is not None:
        tex = _tex(nodes, alpha, "Non-Color", -500, -500, uv_out, links)
        links.new(tex.outputs["Color"], bsdf.inputs["Alpha"])
    if roughness is not None:
        tex = _tex(nodes, roughness, "Non-Color", -500, 0, uv_out, links)
        sep = nodes.new("ShaderNodeSeparateColor")
        sep.location = (-200, 0)
        links.new(tex.outputs["Color"], sep.inputs["Color"])
        links.new(sep.outputs["Green" if roughness is metallic else "Red"], bsdf.inputs["Roughness"])
    if metallic is not None and metallic is not roughness:
        tex = _tex(nodes, metallic, "Non-Color", -500, -250, uv_out, links)
        sep = nodes.new("ShaderNodeSeparateColor")
        links.new(tex.outputs["Color"], sep.inputs["Color"])
        links.new(sep.outputs["Red"], bsdf.inputs["Metallic"])
    if normal is not None:
        tex = _tex(nodes, normal, "Non-Color", -500, -750, uv_out, links)
        nmap = nodes.new("ShaderNodeNormalMap")
        nmap.location = (-200, -750)
        nmap.uv_map = uv_map
        links.new(tex.outputs["Color"], nmap.inputs["Color"])
        links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    if alpha_clip or alpha is not None or base_color_has_alpha:
        # glTF exporter reads alpha mode from these settings.
        if hasattr(mat, "surface_render_method"):
            mat.surface_render_method = "DITHERED"
        if hasattr(mat, "blend_method"):
            try:
                mat.blend_method = "CLIP"
            except TypeError:
                pass
        if alpha_clip:
            # A Math node "1 - (alpha < 0.5)" is how the exporter detects MASK mode.
            src = bsdf.inputs["Alpha"].links[0].from_socket if bsdf.inputs["Alpha"].links else None
            if src is not None:
                lt = nodes.new("ShaderNodeMath")
                lt.operation = "LESS_THAN"
                lt.inputs[1].default_value = 0.5
                inv = nodes.new("ShaderNodeMath")
                inv.operation = "SUBTRACT"
                inv.inputs[0].default_value = 1.0
                links.new(src, lt.inputs[0])
                links.new(lt.outputs[0], inv.inputs[1])
                links.new(inv.outputs[0], bsdf.inputs["Alpha"])
    mat.use_backface_culling = not double_sided
    return mat


# --------------------------------------------------------------------------- meshes


def triangle_count(obj):
    mesh = obj.data
    return sum(len(p.vertices) - 2 for p in mesh.polygons)


def apply_decimate(obj, ratio):
    if ratio >= 0.999:
        return
    mod = obj.modifiers.new("decimate", "DECIMATE")
    mod.decimate_type = "COLLAPSE"
    mod.ratio = max(ratio, 0.0005)
    mod.use_collapse_triangulate = True
    with bpy.context.temp_override(object=obj, active_object=obj, selected_objects=[obj]):
        bpy.ops.object.modifier_apply(modifier=mod.name)


def decimate_to(obj, target_tris):
    tris = triangle_count(obj)
    if tris > target_tris:
        apply_decimate(obj, target_tris / tris)


def ensure_uv_from_attribute(mesh, name="UVMap"):
    """Geometry nodes often leave UVs as a generic corner attribute; make it a UV map."""
    if name in mesh.uv_layers:
        return
    attr = mesh.attributes.get(name)
    if attr is None or attr.domain != "CORNER":
        return
    data = np.empty(len(mesh.loops) * (3 if attr.data_type == "FLOAT_VECTOR" else 2), dtype=np.float32)
    attr.data.foreach_get("vector", data)
    comps = 3 if attr.data_type == "FLOAT_VECTOR" else 2
    uv = data.reshape(-1, comps)[:, :2].ravel()
    mesh.attributes.remove(attr)
    layer = mesh.uv_layers.new(name=name)
    layer.data.foreach_set("uv", uv)


def new_object(name, mesh, collection=None):
    obj = bpy.data.objects.new(name, mesh)
    (collection or bpy.context.scene.collection).objects.link(obj)
    return obj


def export_glb(path, objects, *, jpeg_quality=88):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        if obj.type == "MESH":
            obj.data.name = obj.name  # glTF mesh name = object name
        obj.hide_set(False)
        obj.hide_render = False
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=True,
        # Multi-scene .blend files otherwise produce empty glTF scenes without
        # `nodes`, which Bevy's glTF loader rejects.
        use_active_scene=True,
        export_apply=True,
        export_yup=True,
        export_texcoords=True,
        export_normals=True,
        export_tangents=False,
        export_materials="EXPORT",
        export_image_format="JPEG",
        export_image_quality=jpeg_quality,
        export_cameras=False,
        export_lights=False,
        export_animations=False,
        export_skins=False,
        export_morph=False,
        export_extras=False,
    )
    print(f"exported {path} ({path.stat().st_size / 1e6:.1f} MB)")


def clear_scene_objects(scene):
    for obj in list(scene.collection.all_objects):
        scene.collection.objects.unlink(obj) if obj.name in scene.collection.objects else None
    for child in list(scene.collection.children):
        scene.collection.children.unlink(child)


def deg(x):
    return math.radians(x)
