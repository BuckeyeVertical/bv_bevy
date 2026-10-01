"""Print a summary of a .blend file: collections, objects, poly counts, materials.

Usage: blender -b <file.blend> --python tools/blender/inspect_blend.py
"""

import bpy


def tris(obj):
    if obj.type != "MESH":
        return 0
    mesh = obj.data
    mesh.calc_loop_triangles()
    return len(mesh.loop_triangles)


def walk(collection, depth=0):
    pad = "  " * depth
    print(f"{pad}[C] {collection.name} ({len(collection.objects)} objs)")
    for obj in collection.objects:
        dims = tuple(round(d, 2) for d in obj.dimensions)
        mats = [s.material.name for s in obj.material_slots if s.material]
        extra = ""
        if obj.instance_type == "COLLECTION" and obj.instance_collection:
            extra = f" -> instances {obj.instance_collection.name}"
        mods = [m.type for m in getattr(obj, "modifiers", [])]
        print(
            f"{pad}  - {obj.name} type={obj.type} data={getattr(obj.data, 'name', None)} "
            f"tris={tris(obj)} dims={dims} loc={tuple(round(v, 2) for v in obj.location)} "
            f"mats={mats} mods={mods} hidden={obj.hide_render}{extra}"
        )
    for child in collection.children:
        walk(child, depth + 1)


print("=== SCENES:", [s.name for s in bpy.data.scenes])
for scene in bpy.data.scenes:
    print(f"=== SCENE {scene.name}")
    walk(scene.collection)
print("=== ALL COLLECTIONS:", [c.name for c in bpy.data.collections])
print("=== MATERIALS")
for mat in bpy.data.materials:
    images = []
    if mat.use_nodes and mat.node_tree:
        for node in mat.node_tree.nodes:
            if node.type == "TEX_IMAGE" and node.image:
                images.append(node.image.filepath)
    print(f"  {mat.name}: users={mat.users} images={images}")
