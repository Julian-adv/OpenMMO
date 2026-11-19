"""Build the wall mirror object: a wooden frame around a polished plate, hung like the wall torch.

blender -b --python-exit-code 1 --python tools/blender-scripts/build_wall_mirror.py

Origin at the bottom centre of the back, front along +Z, metres.
"""
from pathlib import Path

import bmesh
import bpy

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'client/public/models/objects/wall_mirror.glb'
WIDTH, HEIGHT, DEPTH, RAIL = .8, 1.2, .05, .07

bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, colour, metallic, roughness):
    result = bpy.data.materials.new(name)
    result.use_nodes = True
    shader = result.node_tree.nodes['Principled BSDF']
    shader.inputs['Base Color'].default_value = (*colour, 1)
    shader.inputs['Metallic'].default_value = metallic
    shader.inputs['Roughness'].default_value = roughness
    return result


def box(mesh, low, high, slot):
    corners = [mesh.verts.new((x, y, z)) for x in (low[0], high[0]) for y in (low[1], high[1]) for z in (low[2], high[2])]
    for a, b, c, d in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
        mesh.faces.new([corners[a], corners[b], corners[c], corners[d]]).material_index = slot


# Blender is Z up with -Y to the front; the exporter turns that into glTF's Y up and +Z front.
mesh = bmesh.new()
half = WIDTH / 2
for low, high in (((-half, -DEPTH, 0), (half, 0, RAIL)), ((-half, -DEPTH, HEIGHT - RAIL), (half, 0, HEIGHT)),
                  ((-half, -DEPTH, RAIL), (-half + RAIL, 0, HEIGHT - RAIL)), ((half - RAIL, -DEPTH, RAIL), (half, 0, HEIGHT - RAIL))):
    box(mesh, low, high, 0)
box(mesh, (-half + RAIL, -DEPTH * .5, RAIL), (half - RAIL, 0, HEIGHT - RAIL), 1)
bmesh.ops.recalc_face_normals(mesh, faces=mesh.faces)
data = bpy.data.meshes.new('wall_mirror')
mesh.to_mesh(data)
data.materials.append(material('Mirror frame', (.16, .09, .05), 0, .7))
# Not metal: without reflections to pick up, a metal plate renders nearly black.
data.materials.append(material('Mirror plate', (.74, .84, .9), 0, .08))
mirror = bpy.data.objects.new('WallMirror', data)
bpy.context.scene.collection.objects.link(mirror)
OUT.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.export_scene.gltf(filepath=str(OUT), export_format='GLB', export_apply=True)
print('wrote', OUT, len(data.polygons), 'faces')
