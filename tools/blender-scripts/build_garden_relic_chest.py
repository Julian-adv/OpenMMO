"""Cut the lid off the kneeling excavator's strongbox and rig it to open.

blender -b -P tools/blender-scripts/build_garden_relic_chest.py

The Tripo model is one shell. Faces above the lid seam inside the box's
footprint become the lid, hinged on the box's back edge. It opens only to 80°,
short of the bowed head and arms. Caps of the same stone, shaded darker, close
the box mouth and the lid's underside, which the generator left hollow. The
clip is named `ChestOpen` like the plain chest's.
"""
import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb  # noqa: E402
SOURCE = ROOT / 'assets/props/garden_relic_chest/tripo/relic_tripo.glb'
WORK = ROOT / 'assets/props/garden_relic_chest/game_v1/garden_relic_chest.blend'
OUT = ROOT / 'client/public/models/objects/garden_relic_chest.glb'
HEIGHT_M = 1.15
TEXTURE = 512
# Source units, before scaling: the groove under the lid, a height above the
# lid and below the bowed head, the box's back edge measured from the front
# face, and its half width inside the gripping hands.
SEAM_Z = 0.232
LID_TOP_Z = 0.38
BOX_DEPTH = 0.30
BOX_HALF_WIDTH = 0.33
OPEN_DEG = 80
OPEN_SECONDS = 0.8
# The caps reuse the stone texture, darkened as the inside of a shut box.
INNER_MATERIAL = 'relic_inner'
INNER_SHADE = 0.35
FPS = 30


def stone_patch(obj):
    """UV box of the lid's largest upward face: plain stone to tile the caps."""
    uv = obj.data.uv_layers.active.data
    face = max((p for p in obj.data.polygons if p.normal.z > 0.9), key=lambda p: p.area)
    us = [uv[i].uv for i in face.loop_indices]
    return (min(u.x for u in us), min(u.y for u in us), max(u.x for u in us), max(u.y for u in us))


def cap(name, z, x0, x1, y0, y1, material, patch, down=False):
    mesh = bpy.data.meshes.new(name)
    face = (3, 2, 1, 0) if down else (0, 1, 2, 3)
    mesh.from_pydata([(x0, y0, z), (x1, y0, z), (x1, y1, z), (x0, y1, z)], [], [face])
    u0, v0, u1, v1 = patch
    layer = mesh.uv_layers.new()
    for loop, uv in zip(mesh.loops, [(u0, v0), (u1, v0), (u1, v1), (u0, v1)]):
        layer.data[loop.index].uv = uv
    mesh.materials.append(material)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def join(target, extra):
    bpy.ops.object.select_all(action='DESELECT')
    extra.select_set(True)
    target.select_set(True)
    bpy.context.view_layer.objects.active = target
    bpy.ops.object.join()


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
body = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
bpy.context.view_layer.objects.active = body
body.select_set(True)
bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
for obj in [o for o in bpy.context.scene.objects if o is not body]:
    bpy.data.objects.remove(obj)

front = min(v.co.y for v in body.data.vertices)
back = front + BOX_DEPTH

bm = bmesh.new()
bm.from_mesh(body.data)
geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
bmesh.ops.bisect_plane(bm, geom=geom, plane_co=(0, 0, SEAM_Z), plane_no=(0, 0, 1))
for face in bm.faces:
    c = face.calc_center_median()
    face.select = SEAM_Z < c.z < LID_TOP_Z and c.y < back and abs(c.x) < BOX_HALF_WIDTH
bm.to_mesh(body.data)
bm.free()

bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.separate(type='SELECTED')
bpy.ops.object.mode_set(mode='OBJECT')
lid = next(o for o in bpy.context.scene.objects if o.type == 'MESH' and o is not body)
body.name = body.data.name = 'relic_body'
lid.name = lid.data.name = 'relic_lid'

inset = 0.02
inner = body.data.materials[0].copy()
inner.name = INNER_MATERIAL
rect = (-BOX_HALF_WIDTH + inset, BOX_HALF_WIDTH - inset, front + inset, back - inset)
patch = stone_patch(lid)
join(body, cap('relic_mouth', SEAM_Z - 0.03, *rect, inner, patch))
join(lid, cap('relic_lid_under', SEAM_Z + 0.005, *rect, inner, patch, down=True))

# Scale and seat on the floor, centred, front toward −Y (glTF +Z).
everything = [body, lid]
low = Vector(min(v.co[i] for o in everything for v in o.data.vertices) for i in range(3))
high = Vector(max(v.co[i] for o in everything for v in o.data.vertices) for i in range(3))
scale = HEIGHT_M / (high.z - low.z)
centre = (low + high) / 2
hinge = Vector(((0 - centre.x) * scale, (back - centre.y) * scale, (SEAM_Z - low.z) * scale))
for obj in everything:
    for v in obj.data.vertices:
        v.co = Vector(((v.co.x - centre.x) * scale, (v.co.y - centre.y) * scale,
                       (v.co.z - low.z) * scale))
for v in lid.data.vertices:
    v.co -= hinge
lid.location = hinge
lid.parent = body

for image in bpy.data.images:
    if image.size[0] > TEXTURE:
        image.scale(TEXTURE, TEXTURE)

scene = bpy.context.scene
scene.render.fps = FPS
lid.rotation_mode = 'XYZ'
lid.animation_data_create()
action = bpy.data.actions.new('ChestOpen')
lid.animation_data.action = action
end = round(OPEN_SECONDS * FPS)
lid.rotation_euler = (0, 0, 0)
lid.keyframe_insert('rotation_euler', frame=1)
lid.rotation_euler = (-math.radians(OPEN_DEG), 0, 0)
lid.keyframe_insert('rotation_euler', frame=1 + end)
scene.frame_start, scene.frame_end = 1, 1 + end

WORK.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=str(WORK))
bpy.ops.object.select_all(action='DESELECT')
body.select_set(True)
lid.select_set(True)
bpy.ops.export_scene.gltf(
    filepath=str(OUT), export_format='GLB', use_selection=True, export_animations=True,
    export_image_format='WEBP', export_image_quality=90, export_yup=True,
)


def shade_inner(path):
    """Set the inner material's base colour factor in the exported GLB."""
    gltf, binary = read_glb(path)
    for material in gltf['materials']:
        if material.get('name') == INNER_MATERIAL:
            pbr = material.setdefault('pbrMetallicRoughness', {})
            pbr['baseColorFactor'] = [INNER_SHADE, INNER_SHADE, INNER_SHADE, 1.0]
    write_glb(path, gltf, binary)


shade_inner(OUT)
print(json.dumps({
    'triangles': sum(len(p.vertices) - 2 for o in everything for p in o.data.polygons),
    'lid_triangles': sum(len(p.vertices) - 2 for p in lid.data.polygons),
    'size_m': [round((high[i] - low[i]) * scale, 3) for i in range(3)],
    'bytes': OUT.stat().st_size,
}))
