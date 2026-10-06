"""Turn the Poly Haven procedural pines/firs into runtime-friendly LOD meshes.

    blender -b assets/raw/pine_forest/polyhaven_pine_fir_forest.blend \
        --python tools/blender/export_trees.py

The source trees are geometry-node trees that realise ~2-7 million triangles each
(every twig is a dense needle mesh).  This script:

1. renders every twig variant top-down and side-on (Cycles, emission-only so the
   result is pure albedo / world normal) into a per-species alpha atlas;
2. swaps each twig in the twig collections for a crossed card that samples the
   atlas, so the trees' own node groups rebuild them out of cards;
3. realises each tree, decimates bark/trunk, and builds LOD1/LOD2 by keeping a
   random subset of card islands scaled up to preserve canopy coverage;
4. exports every tree LOD into assets/forest/vegetation/trees.glb with
   shared materials.  Mesh names are "<tree>_lod<n>".
"""

import json
import math
import random
import sys
import tempfile
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402

TILE = 512
ATLAS = 2048
RENDER_SIZE = 1024
SAMPLES = 24

SPECIES = {
    "pine": {
        "twigs": ["pine_twig_02", "pine_twig_03", "pine_twig_04",
                  "pine_twig_05", "pine_twig_06", "pine_twig_07"],
        "twig_collections": ["pine_twigs", "pine_twigs.001"],
        # pine_twig_01 is a curve+GN twig whose evaluated bounds include an offset
        # helper; pine_twig_06 is its realised twin (same 0.54 x 0.72 m spray).
        "aliases": {"pine_twig_01": "pine_twig_06"},
        "twig_material": "pine_twig",
        "bark": "pine_bark",
    },
    "fir": {
        "twigs": ["fir_twig_main_a", "fir_twig_main_b", "fir_twig_main_c",
                  "fir_twig_tip_a", "fir_twig_tip_b", "fir_twig_tip_c", "fir_twig_tip_d"],
        "twig_collections": ["fir_twigs_main", "fir_twigs_tip", "fir_twigs_main.001", "fir_twigs_tip.001"],
        "twig_material": "fir_twig",
        "bark": "fir_bark",
    },
}

TREES = {
    "pine_01": "pine", "pine_02": "pine", "pine_03": "pine", "pine_04": "pine", "pine_05": "pine",
    "silver_fir_01": "fir", "silver_fir_02": "fir", "silver_fir_03": "fir",
    "silver_fir_04": "fir", "silver_fir_05": "fir", "silver_fir_06": "fir",
    "pine_sapling_medium_a": "pine", "pine_sapling_medium_b": "pine", "pine_sapling_medium_c": "pine",
    "fir_sapling_medium_a": "fir", "fir_sapling_medium_b": "fir", "fir_sapling_medium_c": "fir",
}

# (trunk tris, branch tris, min branch length kept [m], dead-branch tris,
#  fraction of card islands kept, card island scale)
LODS = [
    (3000, 4000, 0.0, 1200, 1.00, 1.00),
    (450, 700, 2.5, 0, 0.55, 1.30),
    (100, 0, 0.0, 0, 0.16, 1.95),
]
IMPOSTOR_ATLAS = 2048
IMPOSTOR_CELL = (256, 512)  # side view cell; top views use half cells (256 x 256)

TEX = Path(bpy.path.abspath("//textures"))
OUT = common.runtime_root() / "vegetation"


# --------------------------------------------------------------------------- bake


def local_bbox(obj, scene):
    """Bounding box of everything an object renders (incl. GN instances), object space."""
    scene.view_layers[0].update()
    dg = scene.view_layers[0].depsgraph
    # The evaluated bound box covers geometry-node instances as well.
    pts = [Vector(c) for c in obj.evaluated_get(dg).bound_box]
    xs, ys, zs = zip(*[(p.x, p.y, p.z) for p in pts])
    return Vector((min(xs), min(ys), min(zs))), Vector((max(xs), max(ys), max(zs)))


def bake_override(material, mode):
    """Copy of a twig material whose surface is emission of albedo or world normal."""
    mat = material.copy()
    mat.name = f"{material.name}_bake_{mode}"
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = next(n for n in nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    out = next(n for n in nodes if n.bl_idname == "ShaderNodeOutputMaterial" and n.is_active_output)
    alpha_src = bsdf.inputs["Alpha"].links[0].from_socket if bsdf.inputs["Alpha"].links else None
    if mode == "albedo":
        color_src = bsdf.inputs["Base Color"].links[0].from_socket
    else:
        if bsdf.inputs["Normal"].links:
            normal_src = bsdf.inputs["Normal"].links[0].from_socket
        else:
            normal_src = nodes.new("ShaderNodeNewGeometry").outputs["Normal"]
        remap = nodes.new("ShaderNodeVectorMath")
        remap.operation = "MULTIPLY_ADD"
        remap.inputs[1].default_value = (0.5, 0.5, 0.5)
        remap.inputs[2].default_value = (0.5, 0.5, 0.5)
        links.new(normal_src, remap.inputs[0])
        color_src = remap.outputs["Vector"]
    emission = nodes.new("ShaderNodeEmission")
    emission.inputs["Strength"].default_value = 1.0
    links.new(color_src, emission.inputs["Color"])
    transparent = nodes.new("ShaderNodeBsdfTransparent")
    mix = nodes.new("ShaderNodeMixShader")
    if alpha_src is not None:
        links.new(alpha_src, mix.inputs["Fac"])
    else:
        mix.inputs["Fac"].default_value = 1.0
    links.new(transparent.outputs[0], mix.inputs[1])
    links.new(emission.outputs[0], mix.inputs[2])
    links.new(mix.outputs[0], out.inputs["Surface"])
    return mat


def setup_bake_scene():
    scene = bpy.data.scenes.new("twig_bake")
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = SAMPLES
    scene.cycles.use_denoising = False
    scene.cycles.max_bounces = 0
    scene.cycles.transparent_max_bounces = 256
    scene.cycles.filter_width = 1.0
    scene.render.film_transparent = True
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.render.image_settings.file_format = "OPEN_EXR"
    scene.render.image_settings.color_depth = "32"
    scene.render.image_settings.color_mode = "RGBA"
    cam_data = bpy.data.cameras.new("bake_cam")
    cam_data.type = "ORTHO"
    cam = bpy.data.objects.new("bake_cam", cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    return scene, cam


def render_view(scene, cam, obj, bbox, view, override, tmpdir):
    mn, mx = bbox
    size = mx - mn
    center = (mn + mx) * 0.5
    if view == "top":
        cam.location = (center.x, center.y, mx.z + 1.0)
        cam.rotation_euler = (0.0, 0.0, 0.0)
        extent = (size.x, size.y)
        depth = size.z
    elif view == "side_y":  # long axis Y, camera on +X looking -X
        cam.location = (mx.x + 1.0, center.y, center.z)
        cam.rotation_euler = (math.radians(90), 0.0, math.radians(90))
        extent = (size.y, size.z)
        depth = size.x
    else:  # side_x: long axis X, camera on -Y looking +Y
        cam.location = (center.x, mn.y - 1.0, center.z)
        cam.rotation_euler = (math.radians(90), 0.0, 0.0)
        extent = (size.x, size.z)
        depth = size.y
    cam.data.ortho_scale = max(extent)
    cam.data.clip_start = 0.01
    cam.data.clip_end = depth + 2.0
    if extent[0] >= extent[1]:
        rx, ry = RENDER_SIZE, max(8, round(RENDER_SIZE * extent[1] / extent[0]))
    else:
        rx, ry = max(8, round(RENDER_SIZE * extent[0] / extent[1])), RENDER_SIZE
    scene.render.resolution_x, scene.render.resolution_y = rx, ry
    scene.render.resolution_percentage = 100
    scene.view_layers[0].material_override = override
    path = Path(tmpdir) / f"{obj.name}_{view}_{override.name}.exr"
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True, scene=scene.name)
    image = bpy.data.images.load(str(path))
    array = common.image_to_array(image).copy()
    bpy.data.images.remove(image)
    return array


def tangent_normal(world_rgb, view):
    n = world_rgb * 2.0 - 1.0
    if view == "top":
        t = n
    elif view == "side_y":
        t = np.stack([n[..., 1], n[..., 2], n[..., 0]], -1)
    else:
        t = np.stack([n[..., 0], n[..., 2], -n[..., 1]], -1)
    t /= np.maximum(np.linalg.norm(t, axis=-1, keepdims=True), 1e-6)
    return t * 0.5 + 0.5


def bake_species(species, config, tmpdir):
    scene, cam = setup_bake_scene()
    twig_mat = bpy.data.materials[config["twig_material"]]
    albedo_override = bake_override(twig_mat, "albedo")
    normal_override = bake_override(twig_mat, "normal")

    atlas_c = np.zeros((ATLAS, ATLAS, 4), np.float32)
    atlas_n = np.zeros((ATLAS, ATLAS, 4), np.float32)
    slots_per_row = ATLAS // TILE
    full_slots = [(i % slots_per_row, i // slots_per_row) for i in range(slots_per_row * slots_per_row)]
    next_full = 0
    half_free = []
    cards = {}
    pad = 4

    for name in config["twigs"]:
        obj = bpy.data.objects[name]
        saved_matrix = obj.matrix_world.copy()
        obj.matrix_world = Matrix.Identity(4)
        scene.collection.objects.link(obj)
        bbox = local_bbox(obj, scene)
        size = bbox[1] - bbox[0]
        side_view = "side_y" if size.y >= size.x else "side_x"
        rects = {}
        for view in ("top", side_view):
            if view == "top":
                sx, sy = full_slots[next_full]
                next_full += 1
                rect = (sx * TILE, sy * TILE, TILE, TILE)
            else:
                if not half_free:
                    sx, sy = full_slots[next_full]
                    next_full += 1
                    half_free += [(sx * TILE, sy * TILE), (sx * TILE, sy * TILE + TILE // 2)]
                x0, y0 = half_free.pop(0)
                rect = (x0, y0, TILE, TILE // 2)
            color = render_view(scene, cam, obj, bbox, view, albedo_override, tmpdir)
            normal = render_view(scene, cam, obj, bbox, view, normal_override, tmpdir)
            x0, y0, w, h = rect
            iw, ih = w - 2 * pad, h - 2 * pad
            color = common.resize_array(color, iw, ih)
            normal = common.resize_array(normal, iw, ih)
            alpha = np.clip(color[..., 3:4], 0, 1)
            rgb = color[..., :3] / np.maximum(alpha, 1e-4)  # un-premultiply film alpha
            nrm = normal[..., :3] / np.maximum(alpha, 1e-4)
            atlas_c[y0 + pad : y0 + pad + ih, x0 + pad : x0 + pad + iw] = np.concatenate(
                [common.linear_to_srgb(rgb), alpha], -1
            )
            atlas_n[y0 + pad : y0 + pad + ih, x0 + pad : x0 + pad + iw] = np.concatenate(
                [tangent_normal(np.clip(nrm, 0, 1), view), alpha], -1
            )
            rects[view] = ((x0 + pad) / ATLAS, (y0 + pad) / ATLAS, iw / ATLAS, ih / ATLAS)
        cards[name] = {"bbox": [list(bbox[0]), list(bbox[1])], "side": side_view, "rects": rects}
        scene.collection.objects.unlink(obj)
        obj.matrix_world = saved_matrix
        print(f"baked {name} bbox {tuple(round(v, 2) for v in size)}")

    atlas_c = common.dilate_rgb(atlas_c)
    atlas_n = common.dilate_rgb(atlas_n)
    flat = np.array([0.5, 0.5, 1.0], np.float32)
    atlas_n[..., :3] = np.where(atlas_n[..., 3:4] > 0.001, atlas_n[..., :3], flat)
    atlas_n[..., 3] = 1.0
    bake_dir = Path(tmpdir).parent
    color_img = common.save_image(
        common.array_to_image(f"{species}_twig_card_diff", atlas_c, "sRGB"),
        bake_dir / f"{species}_twig_card_diff.png",
    )
    normal_img = common.save_image(
        common.array_to_image(f"{species}_twig_card_nor", atlas_n, "Non-Color"),
        bake_dir / f"{species}_twig_card_nor.png",
    )
    return cards, color_img, normal_img


# --------------------------------------------------------------------------- cards


def card_mesh(name, card):
    mn, mx = (Vector(v) for v in card["bbox"])
    c = (mn + mx) * 0.5
    zc = c.z
    top = card["rects"]["top"]
    side = card["rects"][card["side"]]

    def uv(rect, u, v):
        return (rect[0] + u * rect[2], rect[1] + v * rect[3])

    def tu(p):  # top-view uv
        return uv(top, (p.x - mn.x) / (mx.x - mn.x), (p.y - mn.y) / (mx.y - mn.y))

    bm = bmesh.new()
    verts = {}

    def vert(p):
        key = tuple(round(v, 6) for v in p)
        if key not in verts:
            verts[key] = bm.verts.new(p)
        return verts[key]

    quads = []
    if card["side"] == "side_y":
        a, b = Vector((c.x, mn.y, zc)), Vector((c.x, mx.y, zc))
        quads.append(([Vector((mn.x, mn.y, zc)), a, b, Vector((mn.x, mx.y, zc))], tu))
        quads.append(([a, Vector((mx.x, mn.y, zc)), Vector((mx.x, mx.y, zc)), b], tu))

        def su(p):
            return uv(side, (p.y - mn.y) / (mx.y - mn.y), (p.z - mn.z) / (mx.z - mn.z))

        quads.append(([Vector((c.x, mn.y, mn.z)), Vector((c.x, mx.y, mn.z)), b, a], su))
        quads.append(([a, b, Vector((c.x, mx.y, mx.z)), Vector((c.x, mn.y, mx.z))], su))
    else:
        a, b = Vector((mn.x, c.y, zc)), Vector((mx.x, c.y, zc))
        quads.append(([Vector((mn.x, mn.y, zc)), Vector((mx.x, mn.y, zc)), b, a], tu))
        quads.append(([a, b, Vector((mx.x, mx.y, zc)), Vector((mn.x, mx.y, zc))], tu))

        def su(p):
            return uv(side, (p.x - mn.x) / (mx.x - mn.x), (p.z - mn.z) / (mx.z - mn.z))

        quads.append(([Vector((mn.x, c.y, mn.z)), Vector((mx.x, c.y, mn.z)), b, a], su))
        quads.append(([a, b, Vector((mx.x, c.y, mx.z)), Vector((mn.x, c.y, mx.z))], su))

    uv_layer = bm.loops.layers.uv.new("UVMap")
    for points, uv_fn in quads:
        face = bm.faces.new([vert(p) for p in points])
        for loop, p in zip(face.loops, points):
            loop[uv_layer].uv = uv_fn(p)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return mesh


def swap_twigs_for_cards(config, cards, card_material):
    for coll_name in config["twig_collections"]:
        coll = bpy.data.collections[coll_name]
        for obj in list(coll.objects):
            base = obj.name.split(".")[0]
            base = config.get("aliases", {}).get(base, base)
            if base not in cards:
                continue
            mesh = card_mesh(f"{base}_card", cards[base])
            mesh.materials.append(card_material)
            card = bpy.data.objects.new(f"{obj.name}_card", mesh)
            card.matrix_world = obj.matrix_world.copy()
            coll.objects.link(card)
            coll.objects.unlink(obj)


# --------------------------------------------------------------------------- export


def tex(name, max_size=1024):
    return common.downscaled_copy(common.load_image(TEX / name), max_size, common.raw_root() / "_bake" / "trees")


_material_cache = {}


def bark_material(species):
    key = f"{species}_bark"
    if key not in _material_cache:
        prefix = SPECIES[species]["bark"]
        _material_cache[key] = common.pbr_material(
            key,
            base_color=tex(f"{prefix}_diff.png"),
            roughness=tex(f"{prefix}_rough.png"),
            normal=tex(f"{prefix}_nor_gl.png"),
        )
    return _material_cache[key]


def trunk_material(source):
    diff = next(
        Path(n.image.filepath).name
        for n in source.node_tree.nodes
        if n.type == "TEX_IMAGE" and n.image and "trunk" in n.image.filepath and "_diff" in n.image.filepath
    )
    prefix = diff.replace("_diff.png", "")
    if prefix not in _material_cache:
        _material_cache[prefix] = common.pbr_material(
            prefix,
            base_color=tex(f"{prefix}_diff.png"),
            roughness=tex(f"{prefix}_rough.png"),
            normal=tex(f"{prefix}_nor_gl.png"),
        )
    return _material_cache[prefix]


def split_by(mesh, keep_material_indices, name):
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index not in keep_material_indices], context="FACES")
    out = bpy.data.meshes.new(name)
    bm.to_mesh(out)
    bm.free()
    return out


def card_islands(mesh):
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    seen = set()
    islands = []
    for face in bm.faces:
        if face.index in seen:
            continue
        stack, island = [face], []
        seen.add(face.index)
        while stack:
            f = stack.pop()
            island.append(f.index)
            for v in f.verts:
                for g in v.link_faces:
                    if g.index not in seen:
                        seen.add(g.index)
                        stack.append(g)
        islands.append(island)
    bm.free()
    return islands


def drop_small_islands(mesh, min_extent):
    """Remove disconnected pieces shorter than min_extent (small branches hidden by cards)."""
    if min_extent <= 0.0 or len(mesh.polygons) == 0:
        return mesh
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    doomed = []
    for island in card_islands(mesh):
        pts = np.array([v.co[:] for f in island for v in bm.faces[f].verts])
        if np.max(pts.max(0) - pts.min(0)) < min_extent:
            doomed += [bm.faces[f] for f in island]
    bmesh.ops.delete(bm, geom=doomed, context="FACES")
    out = bpy.data.meshes.new(mesh.name + "_big")
    bm.to_mesh(out)
    bm.free()
    return out


def thin_cards(mesh, islands, keep, scale, seed):
    if keep >= 0.999 and abs(scale - 1.0) < 1e-3:
        return mesh.copy()
    rng = random.Random(seed)
    kept = [isl for isl in islands if rng.random() < keep]
    keep_faces = {i for isl in kept for i in isl}
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    island_of = {}
    for idx, isl in enumerate(kept):
        for f in isl:
            island_of[f] = idx
    centers = {}
    for idx, isl in enumerate(kept):
        vs = {v for f in isl for v in bm.faces[f].verts}
        centers[idx] = (sum((v.co for v in vs), Vector()) / len(vs), vs)
    for idx, (center, vs) in centers.items():
        for v in vs:
            v.co = center + (v.co - center) * scale
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.index not in keep_faces], context="FACES")
    out = bpy.data.meshes.new(mesh.name + "_thin")
    bm.to_mesh(out)
    bm.free()
    return out


def join_meshes(*meshes):
    meshes = [m for m in meshes if m is not None and len(m.polygons)]
    if not meshes:
        return None
    bm = bmesh.new()
    for m in meshes:
        bm.from_mesh(m)
    out = bpy.data.meshes.new(meshes[0].name + "_joined")
    bm.to_mesh(out)
    bm.free()
    return out


def join_parts(name, parts):
    """parts: list of (mesh, material). Returns a single object with one slot per part."""
    bm = bmesh.new()
    materials = []
    for mesh, material in parts:
        if mesh is None or len(mesh.polygons) == 0:
            continue
        slot = len(materials)
        materials.append(material)
        tmp = mesh.copy()
        for p in tmp.polygons:
            p.material_index = slot
        bm.from_mesh(tmp)
        bpy.data.meshes.remove(tmp)
    out = bpy.data.meshes.new(name)
    bm.to_mesh(out)
    bm.free()
    for m in materials:
        out.materials.append(m)
    return common.new_object(name, out, export_collection())


_export_coll = None


def export_collection():
    global _export_coll
    if _export_coll is None:
        _export_coll = bpy.data.collections.new("tree_export")
        bpy.data.scenes["trees"].collection.children.link(_export_coll)
    return _export_coll


def decimated(mesh, target, name):
    if mesh is None or target <= 0 or len(mesh.polygons) == 0:
        return None
    obj = common.new_object(name, mesh.copy(), export_collection())
    common.decimate_to(obj, target)
    result = obj.data
    bpy.data.objects.remove(obj)
    return result


def build_tree(tree_name, species, card_material):
    obj = bpy.data.objects[tree_name]
    # Some variants (e.g. silver_fir_b) output twigs as unrealised instances.
    common.add_realize_modifier(obj)
    bpy.context.view_layer.update()
    dg = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
    common.ensure_uv_from_attribute(mesh)
    twig_prefix = SPECIES[species]["twig_material"]

    card_idx, trunk_idx, bark_idx, dead_idx = set(), set(), set(), set()
    trunk_mat = None
    for i, m in enumerate(mesh.materials):
        if m is None:
            bark_idx.add(i)
        elif m.name.startswith(twig_prefix) or m.name.startswith(card_material.name):
            card_idx.add(i)
        elif "dead" in m.name:
            dead_idx.add(i)
        elif "trunk" in m.name:
            trunk_idx.add(i)
            trunk_mat = trunk_material(m)
        else:
            bark_idx.add(i)

    cards = split_by(mesh, card_idx, f"{tree_name}_cards")
    trunk = split_by(mesh, trunk_idx, f"{tree_name}_trunk")
    bark = split_by(mesh, bark_idx, f"{tree_name}_bark")
    dead = split_by(mesh, dead_idx, f"{tree_name}_dead")

    # Put the trunk base at the origin.
    base = trunk if len(trunk.vertices) else mesh
    co = np.array([v.co[:] for v in base.vertices])
    zmin = co[:, 2].min()
    foot = co[co[:, 2] < zmin + 0.3]
    offset = Matrix.Translation(Vector((-foot[:, 0].mean(), -foot[:, 1].mean(), -zmin)))
    for m in (cards, trunk, bark, dead):
        m.transform(offset)

    islands = card_islands(cards)
    stats = {"cards": len(islands), "source_tris": sum(len(p.vertices) - 2 for p in mesh.polygons)}
    objects = []
    for lod, (trunk_tris, bark_tris, min_branch, dead_tris, keep, scale) in enumerate(LODS):
        lod_cards = thin_cards(cards, islands, keep, scale, seed=hash((tree_name, lod)) & 0xFFFF)
        parts = [
            (decimated(trunk, trunk_tris, f"{tree_name}_trunk_lod{lod}"), trunk_mat),
            (join_meshes(
                decimated(drop_small_islands(bark, min_branch), bark_tris, f"{tree_name}_bark_lod{lod}"),
                decimated(dead, dead_tris, f"{tree_name}_dead_lod{lod}"),
            ), bark_material(species)),
            (lod_cards, card_material),
        ]
        out = join_parts(f"{tree_name}_lod{lod}", parts)
        stats[f"lod{lod}_tris"] = common.triangle_count(out)
        objects.append(out)
    co = np.array([v.co[:] for v in objects[0].data.vertices])
    stats["height"] = float(co[:, 2].max())
    stats["radius"] = float(np.linalg.norm(co[:, :2], axis=1).max())
    bpy.data.meshes.remove(mesh)
    print(f"tree {tree_name}: {stats}")
    return objects, stats


# --------------------------------------------------------------------------- impostors


def bake_impostors(lod0_objects, tmpdir):
    """Render each finished tree side-on and top-down into one atlas; build LOD3 meshes."""
    scene, cam = setup_bake_scene()
    cell_w, cell_h = IMPOSTOR_CELL
    cols, rows = IMPOSTOR_ATLAS // cell_w, IMPOSTOR_ATLAS // cell_h
    atlas_c = np.zeros((IMPOSTOR_ATLAS, IMPOSTOR_ATLAS, 4), np.float32)
    atlas_n = np.zeros((IMPOSTOR_ATLAS, IMPOSTOR_ATLAS, 4), np.float32)
    cells = [(c * cell_w, r * cell_h) for r in range(rows) for c in range(cols)]
    half_free = []
    overrides = {}
    pad = 3
    rects = {}

    def place(array, normal, x0, y0, w, h, view):
        iw, ih = w - 2 * pad, h - 2 * pad
        color = common.resize_array(array, iw, ih)
        nrm = common.resize_array(normal, iw, ih)
        alpha = np.clip(color[..., 3:4], 0, 1)
        rgb = color[..., :3] / np.maximum(alpha, 1e-4)
        n = nrm[..., :3] / np.maximum(alpha, 1e-4)
        atlas_c[y0 + pad : y0 + pad + ih, x0 + pad : x0 + pad + iw] = np.concatenate(
            [common.linear_to_srgb(rgb), alpha], -1)
        atlas_n[y0 + pad : y0 + pad + ih, x0 + pad : x0 + pad + iw] = np.concatenate(
            [tangent_normal(np.clip(n, 0, 1), view), alpha], -1)
        return ((x0 + pad) / IMPOSTOR_ATLAS, (y0 + pad) / IMPOSTOR_ATLAS, iw / IMPOSTOR_ATLAS, ih / IMPOSTOR_ATLAS)

    for obj in lod0_objects:
        tree = obj.name.rsplit("_lod", 1)[0]
        bake = obj.copy()
        bake.data = obj.data.copy()
        scene.collection.objects.link(bake)
        mn = Vector(np.array([v.co[:] for v in bake.data.vertices]).min(0))
        mx = Vector(np.array([v.co[:] for v in bake.data.vertices]).max(0))
        # Make the crown roughly symmetric about the trunk so crossed quads line up.
        half = max(abs(mn.x), abs(mx.x), abs(mn.y), abs(mx.y))
        mn.x, mn.y, mx.x, mx.y = -half, -half, half, half
        renders = {}
        for mode in ("albedo", "normal"):
            for i, mat in enumerate(obj.data.materials):
                key = (mat.name, mode)
                if key not in overrides:
                    overrides[key] = bake_override(mat, mode)
                bake.data.materials[i] = overrides[key]
            scene.view_layers[0].material_override = None
            for view in ("side_x", "top"):
                renders[(view, mode)] = render_view_plain(scene, cam, bake, (mn, mx), view, tmpdir, mode)
        x0, y0 = cells.pop(0)
        side_rect = place(renders[("side_x", "albedo")], renders[("side_x", "normal")], x0, y0, cell_w, cell_h,
                          "side_x")
        if not half_free:
            hx, hy = cells.pop(0)
            half_free += [(hx, hy), (hx, hy + cell_h // 2)]
        tx, ty = half_free.pop(0)
        top_rect = place(renders[("top", "albedo")], renders[("top", "normal")], tx, ty, cell_w, cell_h // 2, "top")
        rects[tree] = (mn.copy(), mx.copy(), side_rect, top_rect)
        scene.collection.objects.unlink(bake)
        bpy.data.objects.remove(bake)
        print(f"impostor {tree}")

    atlas_c = common.dilate_rgb(atlas_c)
    atlas_n = common.dilate_rgb(atlas_n)
    flat = np.array([0.5, 0.5, 1.0], np.float32)
    atlas_n[..., :3] = np.where(atlas_n[..., 3:4] > 0.001, atlas_n[..., :3], flat)
    atlas_n[..., 3] = 1.0
    bake_dir = Path(tmpdir).parent
    color_img = common.save_image(common.array_to_image("tree_impostor_diff", atlas_c, "sRGB"),
                                  bake_dir / "tree_impostor_diff.png")
    normal_img = common.save_image(common.array_to_image("tree_impostor_nor", atlas_n, "Non-Color"),
                                   bake_dir / "tree_impostor_nor.png")
    material = common.pbr_material("tree_impostor", base_color=color_img, base_color_has_alpha=True,
                                   normal=normal_img, roughness_value=0.9, alpha_clip=True, double_sided=True)
    objects = []
    for tree, (mn, mx, side, top) in rects.items():
        objects.append(impostor_object(f"{tree}_lod3", mn, mx, side, top, material))
    return objects


def render_view_plain(scene, cam, obj, bbox, view, tmpdir, tag):
    """Like render_view, but with materials already swapped on the object."""
    mn, mx = bbox
    size = mx - mn
    center = (mn + mx) * 0.5
    if view == "top":
        cam.location = (center.x, center.y, mx.z + 1.0)
        cam.rotation_euler = (0.0, 0.0, 0.0)
        extent, depth = (size.x, size.y), size.z
    else:
        cam.location = (center.x, mn.y - 1.0, center.z)
        cam.rotation_euler = (math.radians(90), 0.0, 0.0)
        extent, depth = (size.x, size.z), size.y
    cam.data.ortho_scale = max(extent)
    cam.data.clip_start = 0.01
    cam.data.clip_end = depth + 2.0
    if extent[0] >= extent[1]:
        rx, ry = RENDER_SIZE, max(8, round(RENDER_SIZE * extent[1] / extent[0]))
    else:
        rx, ry = max(8, round(RENDER_SIZE * extent[0] / extent[1])), RENDER_SIZE
    scene.render.resolution_x, scene.render.resolution_y = rx, ry
    path = Path(tmpdir) / f"{obj.name}_{view}_{tag}.exr"
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True, scene=scene.name)
    image = bpy.data.images.load(str(path))
    array = common.image_to_array(image).copy()
    bpy.data.images.remove(image)
    return array


def impostor_object(name, mn, mx, side, top, material):
    """Two crossed vertical quads (side view) plus a horizontal quad (top view)."""
    bm = bmesh.new()
    uv_layer = bm.loops.layers.uv.new("UVMap")
    half = mx.x
    h0, h1 = mn.z, mx.z
    crown_z = mn.z + 0.4 * (mx.z - mn.z)

    def quad(points, rect, uvs):
        face = bm.faces.new([bm.verts.new(p) for p in points])
        for loop, (u, v) in zip(face.loops, uvs):
            loop[uv_layer].uv = (rect[0] + u * rect[2], rect[1] + v * rect[3])

    side_uv = [(0, 0), (1, 0), (1, 1), (0, 1)]
    quad([(-half, 0, h0), (half, 0, h0), (half, 0, h1), (-half, 0, h1)], side, side_uv)
    quad([(0, -half, h0), (0, half, h0), (0, half, h1), (0, -half, h1)], side, side_uv)
    quad([(-half, -half, crown_z), (half, -half, crown_z), (half, half, crown_z), (-half, half, crown_z)], top,
         side_uv)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material)
    return common.new_object(name, mesh, export_collection())


def main():
    args = common.script_args()
    only = set(args) if args else None
    tmp_root = common.raw_root() / "_bake"
    tmp_root.mkdir(parents=True, exist_ok=True)
    tmpdir = tempfile.mkdtemp(dir=tmp_root)

    card_materials = {}
    meta = {"trees": {}}
    for species, config in SPECIES.items():
        cards, color_img, normal_img = bake_species(species, config, tmpdir)
        card_mat = common.pbr_material(
            f"{species}_twig_card",
            base_color=color_img,
            base_color_has_alpha=True,
            normal=normal_img,
            roughness_value=0.85,
            alpha_clip=True,
            double_sided=True,
        )
        card_materials[species] = card_mat
        swap_twigs_for_cards(config, cards, card_mat)

    bpy.context.view_layer.update()
    exported = []
    for tree_name, species in TREES.items():
        if only and tree_name not in only:
            continue
        objects, stats = build_tree(tree_name, species, card_materials[species])
        exported += objects
        meta["trees"][tree_name] = {"species": species, **stats}

    lod0 = [o for o in exported if o.name.endswith("_lod0")]
    exported += bake_impostors(lod0, tmpdir)

    common.export_glb(OUT / "trees.glb", exported)
    meta["lods"] = [
        {"trunk_tris": t, "branch_tris": b, "min_branch_m": m, "dead_branch_tris": d, "card_keep": k,
         "card_scale": s}
        for t, b, m, d, k, s in LODS
    ] + [{"impostor": "3 crossed/top quads"}]
    (OUT / "trees.json").write_text(json.dumps(meta, indent=2))


main()
