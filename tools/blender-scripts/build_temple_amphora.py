"""Split the generated amphora sheet into the jar and its remains, and export both.

blender -b -P tools/blender-scripts/build_temple_amphora.py

One Tripo model holds the intact jar and the smashed one side by side. The jar
is the tallest loose part with its handles, which come as parts of their own
high on its sides; the remains are the parts beside it. A shard the generator
left at the jar's foot belongs to neither and is dropped.
"""
import json
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/props/temple_amphora/tripo_v1/model_url.glb'
WORK = ROOT / 'assets/props/temple_amphora/game_v1/temple_amphora.blend'
MODELS = ROOT / 'client/public/models/objects'
JAR_HEIGHT_M = 0.95
TEXTURE = 512


def box(objects):
    points = [o.matrix_world @ v.co for o in objects for v in o.data.vertices]
    return (Vector(min(p[i] for p in points) for i in range(3)),
            Vector(max(p[i] for p in points) for i in range(3)))


def settle(objects, name, scale):
    """Join `objects`, centre them over the origin on the floor and scale."""
    bpy.ops.object.select_all(action='DESELECT')
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    if len(objects) > 1:
        bpy.ops.object.join()
    joined = bpy.context.view_layer.objects.active
    low, high = box([joined])
    centre = (low + high) / 2
    for vertex in joined.data.vertices:
        vertex.co = Vector(((vertex.co.x - centre.x) * scale, (vertex.co.y - centre.y) * scale,
                            (vertex.co.z - low.z) * scale))
    joined.name = joined.data.name = name
    return joined


def export(obj, name):
    path = MODELS / f'{name}.glb'
    bpy.ops.object.select_all(action='DESELECT')
    obj.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(path), export_format='GLB', use_selection=True, export_animations=False,
        export_image_format='WEBP', export_image_quality=90, export_yup=True,
    )
    low, high = box([obj])
    return {
        'triangles': sum(len(p.vertices) - 2 for p in obj.data.polygons),
        'size_m': [round(v, 3) for v in high - low],
        'bytes': path.stat().st_size,
    }


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
source = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
bpy.context.view_layer.objects.active = source
source.select_set(True)
bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.select_all(action='SELECT')
bpy.ops.mesh.remove_doubles(threshold=1e-5)
bpy.ops.mesh.separate(type='LOOSE')
bpy.ops.object.mode_set(mode='OBJECT')
parts = [o for o in bpy.context.scene.objects if o.type == 'MESH']

boxes = {o: box([o]) for o in parts}
low, high = box(parts)
across = 0 if high.x - low.x > high.y - low.y else 1
jar = max(parts, key=lambda o: boxes[o][1].z - boxes[o][0].z)
jar_low, jar_high = boxes[jar]


def beside(part):
    centre = (boxes[part][0] + boxes[part][1]) / 2
    return not jar_low[across] < centre[across] < jar_high[across]


remains = [o for o in parts if o is not jar and beside(o)]
near = [o for o in parts if o is not jar and not beside(o)]
handles = [o for o in near if boxes[o][0].z > jar_low.z + (jar_high.z - jar_low.z) * 0.4]
strays = [o for o in near if o not in handles]
for stray in strays:
    bpy.data.objects.remove(stray)

scale = JAR_HEIGHT_M / (jar_high.z - jar_low.z)
jar = settle([jar, *handles], 'TempleAmphora', scale)
remains = settle(remains, 'TempleAmphoraBroken', scale)
for image in bpy.data.images:
    if image.size[0] > TEXTURE:
        image.scale(TEXTURE, TEXTURE)
WORK.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=str(WORK))
print(json.dumps({
    'strays_dropped': len(strays),
    'temple_amphora': export(jar, 'temple_amphora'),
    'temple_amphora_broken': export(remains, 'temple_amphora_broken'),
}))
