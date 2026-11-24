"""The Stone Garden's breakable statue and its rubble, drawn for crates there.

    blender -b -P tools/blender-scripts/build_garden_petrified_adventurer.py

Each Tripo model is scaled to its height, centred on the floor and exported
with 512 px WebP textures. A missing source is skipped.
"""
import json
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
ASSET = ROOT / 'assets/props/garden_petrified_adventurer'
MODELS = ROOT / 'client/public/models/objects'
TEXTURE = 512
PARTS = {
    'garden_petrified_adventurer': ('tripo_statue_v1', 1.05),
    'garden_petrified_adventurer_rubble': ('tripo_rubble_v1', 0.32),
}

report = {}
for name, (folder, height) in PARTS.items():
    source = ASSET / folder / 'source/model.glb'
    if not source.exists():
        continue
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(source))
    meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
    bpy.ops.object.select_all(action='DESELECT')
    for mesh in meshes:
        mesh.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    for other in [o for o in bpy.context.scene.objects if o is not obj]:
        bpy.data.objects.remove(other)
    points = [v.co for v in obj.data.vertices]
    low = Vector([min(p[i] for p in points) for i in range(3)])
    high = Vector([max(p[i] for p in points) for i in range(3)])
    scale = height / (high.z - low.z)
    obj.data.transform(Matrix.Scale(scale, 4)
                       @ Matrix.Translation(-Vector(((low.x + high.x) / 2, (low.y + high.y) / 2, low.z))))
    obj.name = obj.data.name = name
    for image in bpy.data.images:
        if image.size[0] > TEXTURE:
            image.scale(TEXTURE, TEXTURE)
    bpy.ops.wm.save_as_mainfile(filepath=str(ASSET / folder / f'{name}.blend'))
    target = MODELS / f'{name}.glb'
    bpy.ops.export_scene.gltf(
        filepath=str(target), export_format='GLB', use_selection=False, export_animations=False,
        export_image_format='WEBP', export_image_quality=90, export_yup=True,
    )
    report[name] = {
        'triangles': sum(len(p.vertices) - 2 for p in obj.data.polygons),
        'size_m': [round((high[i] - low[i]) * scale, 3) for i in range(3)],
        'bytes': target.stat().st_size,
    }
print('PETRIFIED', json.dumps(report))
