"""Export Poly Haven props/buildings/vegetation to runtime GLBs.

    blender -b <source.blend> --python tools/blender/export_props.py -- <job>

Jobs (see JOBS below) pick objects from the open .blend, bake modifiers and
geometry-node instances into plain meshes, decimate to a triangle budget,
rebuild materials as clean glTF PBR (textures downscaled), and export one GLB.

Each exported mesh is named "<asset>" or "<asset>_lod<n>" so the Bevy side can
look meshes up by name.  `scale` converts the source's authoring scale to real
metres where Poly Haven scatter sources are authored small.
"""

import json
import math
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402

OUT = common.runtime_root()


def lod_set(name, sources, tris, scale=1.0, **extra):
    """sources: list of source object names (one per LOD) or a single name decimated per LOD."""
    return {"name": name, "sources": sources, "tris": tris, "scale": scale, **extra}


JOBS = {
    # ----------------------------------------------------------------- pine forest nature props
    "forest_props": {
        "out": "vegetation/forest_props.glb",
        "texture_size": 1024,
        "assets": [
            *[lod_set(f"fern_{c}", [f"fern_01_{c}_lod0", f"fern_02_{c}_lod1"], [2400, 240], scale=2.2)
              for c in "abcd"],
            *[lod_set(f"rock_moss_{c}", [f"rock_moss_set_01_{c}_lod0", f"rock_moss_set_01_{c}_lod1"], [2500, 300])
              for c in "abcdef"],
            *[lod_set(f"rock_river_{n}", [f"rock_moss_set_02_rock{n}_lod0", f"rock_moss_set_02_rock{n}_lod1"],
                      [2500, 300]) for n in ("07", "08", "09", "10", "11", "12", "13")],
            *[lod_set(f"dry_branch_{c}", [f"dry_branches_medium_01_{c}_lod0"], [2000, 300]) for c in "abc"],
            lod_set("stump", ["tree_trunk_lod1"], [3000, 500]),
            lod_set("fallen_log", ["dead_tree_lod1"], [4000, 700]),
            lod_set("roots_a", ["tree_roots_a_lod0"], [2500, 400]),
            lod_set("roots_b", ["tree_roots_b_lod0"], [2500, 400]),
            *[lod_set(f"moss_patch_{n}", [f"moss_patch_0{n}_lod1"], [560]) for n in "123"],
        ],
    },
    # ----------------------------------------------------------------- grass clumps
    "grass": {
        "out": "vegetation/grass.glb",
        "texture_size": 1024,
        "assets": [
            lod_set(f"grass_{v}", [f"grass_medium_01_{v}_LOD1", f"grass_medium_01_{v}_LOD2"], [2500, 1000])
            for v in ("large_a", "large_b", "large_c", "mid_a", "mid_b", "mid_c", "small_a", "small_b",
                      "tall_a", "tall_b", "tall_c")
        ],
    },
    # ----------------------------------------------------------------- props
    "barrel": {
        "out": "props/barrel_01.glb",
        "texture_size": 1024,
        "assets": [lod_set("barrel_01", ["Barrel_01"], [2682, 600])],
    },
    "barrier": {
        "out": "props/concrete_barrier.glb",
        "texture_size": 1024,
        "assets": [lod_set("concrete_barrier", ["concrete_road_barrier_LOD2", "concrete_road_barrier_LOD4"],
                           [4722, 1250])],
    },
    "poles": {
        "out": "props/electricity_poles.glb",
        "texture_size": 1024,
        "assets": [
            lod_set(f"pole_{p}", [f"collection:preset_{p}"], [9000, 1500], center_xy=True)
            for p in ("01", "02", "03")
        ],
    },
    # ----------------------------------------------------------------- the shed
    "shed": {
        "out": "buildings/shed.glb",
        "texture_size": 1024,
        "scene": True,
    },
    # Outdoor-usable props from The Shed collection, exported standalone so the
    # environment can dress the shed / parking area with them.
    "shed_props": {
        "out": "props/shed_props.glb",
        "texture_size": 512,
        "assets": [
            lod_set("barrel_02", ["Barrel_02.001"], [2688, 600]),
            lod_set("compost_bags_standing", ["compost_bags_standing"], [2500, 500]),
            lod_set("compost_bags_stacked", ["compost_bags_floorstacked"], [2500, 500]),
            lod_set("planter_box_01", ["planter_box_01.003"], [3000, 600]),
            lod_set("planter_box_02", ["planter_box_02.003"], [3000, 600]),
            lod_set("outdoor_table", ["outdoor_table_chair_set_01_table.001"], [2324, 500]),
            lod_set("outdoor_chair_a", ["outdoor_table_chair_set_01_chair_01.001"], [2500, 500]),
            lod_set("outdoor_chair_b", ["outdoor_table_chair_set_01_chair_02.001"], [2500, 500]),
            lod_set("plastic_chair", ["plastic_monobloc_chair_01.001"], [2000, 500]),
            lod_set("ladder", ["ladder_section_01.001"], [3000, 600]),
            lod_set("wooden_stool", ["wooden_stool_01.002"], [2000, 400]),
            lod_set("watering_can", ["watering_can_metal_01.001"], [2500, 500]),
            lod_set("cardboard_box", ["cardboard_box_01.001"], [1500, 300]),
            lod_set("hose_reel", ["garden_hose.001"], [3000, 600]),
            lod_set("power_box", ["power_box_01_box"], [3000, 600]),
            lod_set("spade", ["rusted_spade_01.001"], [1500, 300]),
            lod_set("clay_pot", ["planter_pot_clay.001"], [1500, 300]),
            lod_set("nettle", ["nettle_plant"], [3000, 600]),
            lod_set("weeds", ["weed_plant_02_A"], [2000, 400]),
        ],
    },
}

# Shed: collections whose objects form the building, and props worth keeping.
# Brackets (34k tris of tiny metal parts) and the leafless ivy stems (146k tris that
# do not decimate) are invisible at drone range and are left out.
SHED_BUILDING = ["structure", "planks", "shed_door", "floor", "greenhouse_door", "concrete wall", "join_panels",
                 "Greenhouse_main", "Greenhouse_extended"]
SHED_SKIP_WORDS = ["screw", "bolt", "nail", "web", "dust", "fog", "blocker", "Sicky", "light_fitting"]


# --------------------------------------------------------------------------- materials

def _find_image(mat, socket_name, words):
    """Image feeding a Principled input: trace links first, then fall back to file names."""
    nodes = mat.node_tree.nodes
    bsdf = next((n for n in nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
    if bsdf is not None and socket_name in bsdf.inputs:
        frontier, seen = [bsdf.inputs[socket_name]], set()
        while frontier:
            sock = frontier.pop(0)
            for link in sock.links:
                node = link.from_node
                if node.name in seen:
                    continue
                seen.add(node.name)
                if node.type == "TEX_IMAGE" and node.image:
                    return node.image, link.from_socket.name
                frontier += [i for i in node.inputs if i.is_linked]
    for node in nodes:
        if node.type == "TEX_IMAGE" and node.image:
            fname = Path(node.image.filepath or node.image.name).name.lower()
            if any(w in fname for w in words):
                return node.image, "Color"
    return None, None


def _principled_value(mat, name, default):
    bsdf = next((n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
    if bsdf is None or name not in bsdf.inputs:
        return default
    value = bsdf.inputs[name].default_value
    return tuple(value) if hasattr(value, "__len__") else value


_simplified = {}


def simplify_material(mat, texture_size, bake_dir):
    if mat is None:
        return None
    if mat.name in _simplified:
        return _simplified[mat.name]
    if not mat.use_nodes or mat.node_tree is None:
        new = common.pbr_material(mat.name + "_rt")
        new.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*mat.diffuse_color[:3], 1)
        _simplified[mat.name] = new
        return new
    diff, _ = _find_image(mat, "Base Color", ["_diff", "_col", "albedo", "basecolor", "diffuse"])
    rough, _ = _find_image(mat, "Roughness", ["_rough"])
    metal, _ = _find_image(mat, "Metallic", ["_metal"])
    normal, _ = _find_image(mat, "Normal", ["nor_gl", "_normal", "_nor"])
    alpha, alpha_out = _find_image(mat, "Alpha", ["_alpha", "_opacity"])
    if normal is not None and "nor_dx" in (normal.filepath or "").lower():
        normal = None  # DirectX normals would need a green flip; Poly Haven ships GL too.
    diff, rough, metal, normal, alpha = (
        common.downscaled_copy(image, texture_size, bake_dir) for image in (diff, rough, metal, normal, alpha)
    )

    has_alpha = False
    if alpha is not None and diff is not None:
        if alpha is diff:
            has_alpha = True
        else:
            # glTF needs alpha in the base colour texture: merge them.
            d = common.image_to_array(diff)
            a = common.image_to_array(alpha)
            if a.shape != d.shape:
                a = common.resize_array(a, d.shape[1], d.shape[0])
            merged = d.copy()
            merged[..., 3] = a[..., 0] if alpha_out == "Color" else a[..., 3]
            merged = common.dilate_rgb(merged, 0.5)
            diff = common.save_image(
                common.array_to_image(mat.name + "_diff_alpha", merged, "sRGB"),
                bake_dir / f"{mat.name}_diff_alpha.png",
            )
            has_alpha = True

    base_default = _principled_value(mat, "Base Color", (0.8, 0.8, 0.8, 1.0))
    new = common.pbr_material(
        mat.name + "_rt",
        base_color=diff,
        roughness=rough,
        metallic=metal,
        normal=normal,
        base_color_has_alpha=has_alpha,
        roughness_value=float(_principled_value(mat, "Roughness", 0.7)),
        metallic_value=float(_principled_value(mat, "Metallic", 0.0)),
        alpha_clip=has_alpha,
        double_sided=has_alpha,
    )
    if diff is None:
        new.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = base_default
    _simplified[mat.name] = new
    return new


# --------------------------------------------------------------------------- meshes

def baked_mesh(obj):
    """Evaluated mesh of obj in object space, with GN instances realised."""
    common.add_realize_modifier(obj)
    dg = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(dg), preserve_all_data_layers=True, depsgraph=dg)
    common.ensure_uv_from_attribute(mesh)
    if mesh.uv_layers and mesh.uv_layers.active and mesh.uv_layers.active.name != "UVMap":
        if "UVMap" in mesh.uv_layers:
            mesh.uv_layers.remove(mesh.uv_layers["UVMap"])
        mesh.uv_layers.active.name = "UVMap"
    return mesh


def mesh_with_materials(mesh, texture_size, bake_dir):
    for i, mat in enumerate(mesh.materials):
        mesh.materials[i] = simplify_material(mat, texture_size, bake_dir)
    return mesh


def make_object(name, mesh, coll, target_tris):
    obj = common.new_object(name, mesh, coll)
    if target_tris:
        common.decimate_to(obj, target_tris)
    return obj


def join_into(name, items, coll):
    """items: list of (mesh, matrix). Returns a single mesh."""
    bm = bmesh.new()
    materials = []
    for mesh, matrix in items:
        tmp = mesh.copy()
        tmp.transform(matrix)
        remap = []
        for mat in tmp.materials:
            if mat not in materials:
                materials.append(mat)
            remap.append(materials.index(mat))
        for p in tmp.polygons:
            p.material_index = remap[p.material_index] if p.material_index < len(remap) else 0
        bm.from_mesh(tmp)
        bpy.data.meshes.remove(tmp)
    out = bpy.data.meshes.new(name)
    bm.to_mesh(out)
    bm.free()
    for mat in materials:
        out.materials.append(mat)
    return out


def dims(mesh):
    co = np.array([v.co[:] for v in mesh.vertices]) if len(mesh.vertices) else np.zeros((1, 3))
    return co.min(0), co.max(0)


# --------------------------------------------------------------------------- jobs

def run_asset_job(job, coll, bake_dir):
    exported, meta = [], {}
    for asset in job["assets"]:
        sources = asset["sources"]
        lods = []
        for lod, target in enumerate(asset["tris"]):
            src_name = sources[min(lod, len(sources) - 1)]
            if src_name.startswith("collection:"):
                src_coll = bpy.data.collections[src_name.split(":", 1)[1]]
                items = [(baked_mesh(o), o.matrix_world.copy()) for o in src_coll.all_objects if o.type in ("MESH", "CURVE")]
                mesh = join_into(asset["name"], items, coll)
            else:
                src = bpy.data.objects[src_name]
                mesh = baked_mesh(src)
            mesh_with_materials(mesh, job["texture_size"], bake_dir)
            mn, mx = dims(mesh)
            # Base at z=0, centred in xy on the footprint.
            shift = Vector((-(mn[0] + mx[0]) / 2, -(mn[1] + mx[1]) / 2, -mn[2]))
            if src_name.startswith("collection:") and not asset.get("center_xy"):
                shift.x = shift.y = 0.0
            mesh.transform(Matrix.Scale(asset["scale"], 4) @ Matrix.Translation(shift))
            name = asset["name"] if len(asset["tris"]) == 1 else f"{asset['name']}_lod{lod}"
            obj = make_object(name, mesh, coll, target)
            lods.append(common.triangle_count(obj))
            exported.append(obj)
        mn, mx = dims(exported[-len(lods)].data)
        meta[asset["name"]] = {"lod_tris": lods, "size": [float(v) for v in (mx - mn)]}
        print(f"asset {asset['name']}: tris {lods} size {np.round(mx - mn, 2)}")
    return exported, meta


def run_shed_job(job, coll, bake_dir):
    """Building + ivy with the authored layout; interior props are left out (the
    roof hides them) and selected outdoor props are exported by `shed_props`."""
    keep = []
    for name in SHED_BUILDING:
        c = bpy.data.collections.get(name)
        if c:
            keep += [o for o in c.all_objects if o.type in ("MESH", "CURVE")]
    keep = [o for o in dict.fromkeys(keep) if not any(w.lower() in o.name.lower() for w in SHED_SKIP_WORDS)]

    # Group the many small building parts by material into a few meshes; props stay separate.
    building_items = {}
    for obj in keep:
        mesh = baked_mesh(obj)
        if len(mesh.polygons) == 0:
            continue
        mesh_with_materials(mesh, job["texture_size"], bake_dir)
        group = "ivy" if obj.name in bpy.data.collections["ivy"].all_objects else "building"
        building_items.setdefault(group, []).append((mesh, obj.matrix_world.copy()))

    building = join_into("shed_building", building_items["building"], coll)
    mn, mx = dims(building)
    to_origin = Matrix.Translation(Vector((-(mn[0] + mx[0]) / 2, -(mn[1] + mx[1]) / 2, -mn[2])))
    exported = []
    obj = make_object("shed_building", building, coll, 60000)
    obj.data.transform(to_origin)
    exported.append(obj)
    if "ivy" in building_items:
        ivy = join_into("shed_ivy", building_items["ivy"], coll)
        ivy.transform(to_origin)
        exported.append(make_object("shed_ivy", ivy, coll, 40000))
    total = sum(common.triangle_count(o) for o in exported)
    mn, mx = dims(building)
    meta = {"objects": len(exported), "tris": total, "size": [float(v) for v in (mx - mn)]}
    print(f"shed: {meta}")
    return exported, meta


def main():
    job_name = common.script_args()[0]
    job = JOBS[job_name]
    bake_dir = common.raw_root() / "_bake" / job_name
    bake_dir.mkdir(parents=True, exist_ok=True)
    coll = bpy.data.collections.new("export")
    bpy.context.scene.collection.children.link(coll)
    if job.get("scene"):
        exported, meta = run_shed_job(job, coll, bake_dir)
    else:
        exported, meta = run_asset_job(job, coll, bake_dir)
    out = OUT / job["out"]
    common.export_glb(out, exported)
    out.with_suffix(".json").write_text(json.dumps(meta, indent=2))


main()
