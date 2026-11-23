"""Size the generated temple column for the game and export it standing and fallen.

blender -b -P tools/blender-scripts/build_temple_column.py

The plinth is brought to 0.9 m so the column stands inside its one-cell
footprint, which puts the top a little above the 3 m dungeon walls. The fallen
one is the same mesh laid along Z over its three cells, resting on the edges
of its plinth and capital.
"""
import json
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/props/temple_column/tripo_v1/model_url.glb'
WORK = ROOT / 'assets/props/temple_column/game_v1/temple_column.blend'
MODELS = ROOT / 'client/public/models/objects'
PLINTH_M = 0.9
TEXTURE = 512

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
bpy.ops.object.select_all(action='DESELECT')
for mesh in meshes:
    mesh.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
if len(meshes) > 1:
    bpy.ops.object.join()
column = bpy.context.view_layer.objects.active
bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

points = [column.matrix_world @ v.co for v in column.data.vertices]
low = Vector(min(p[i] for p in points) for i in range(3))
high = Vector(max(p[i] for p in points) for i in range(3))
scale = PLINTH_M / max(high.x - low.x, high.y - low.y)
centre = (low + high) / 2
for vertex in column.data.vertices:
    vertex.co = Vector(((vertex.co.x - centre.x) * scale, (vertex.co.y - centre.y) * scale,
                        (vertex.co.z - low.z) * scale))
column.name = column.data.name = 'TempleColumn'
for image in bpy.data.images:
    if image.size[0] > TEXTURE:
        image.scale(TEXTURE, TEXTURE)

for other in [o for o in bpy.context.scene.objects if o is not column]:
    bpy.data.objects.remove(other)
WORK.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=str(WORK))


def export(name):
    path = MODELS / f'{name}.glb'
    bpy.ops.object.select_all(action='DESELECT')
    column.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(path), export_format='GLB', use_selection=True, export_animations=False,
        export_image_format='WEBP', export_image_quality=90, export_yup=True,
    )
    return path.stat().st_size


height = (high.z - low.z) * scale
report = {
    'triangles': sum(len(p.vertices) - 2 for p in column.data.polygons),
    'height_m': round(height, 3),
    'width_m': PLINTH_M,
    'temple_column_bytes': export('temple_column'),
}
# Blender +Y is the game's -Z: the capital points along the prop's +Z.
for vertex in column.data.vertices:
    x, y, z = vertex.co
    vertex.co = Vector((x, height / 2 - z, y + PLINTH_M / 2))
report['temple_fallen_column_bytes'] = export('temple_fallen_column')
print(json.dumps(report))
