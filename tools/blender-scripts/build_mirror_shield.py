"""Turn the Tripo mirror shield into the game model and its icon.

    blender -b -P tools/blender-scripts/build_mirror_shield.py

The source faces +Z with its base on the ground. The game model takes the
Earth Shield's frame: front -Z (glTF), origin at the rear grip a third of the
way down from the top, the back just behind the origin, 85 cm across.
"""
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/blender-scripts'))
from icon_render import add_light, render_icon  # noqa: E402

sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb  # noqa: E402

SOURCE = ROOT / 'assets/items/mirror_shield/tripo_v1/source/model.glb'
WORK = ROOT / 'assets/items/mirror_shield/tripo_v1/mirror_shield.blend'
GAME = ROOT / 'client/public/models/armor/mirror_shield.glb'
ICON = ROOT / 'client/public/items/armor/mirror_shield.png'
DIAMETER = 0.85
# The Earth Shield's grip sits this share of the height up from the bottom,
# and its back reaches this far behind the grip.
GRIP_SHARE = 0.534 / 0.8
BACK_BEHIND_GRIP = 0.015
TEXTURE = 1024
# Polished bronze; `envReflect` is the reflection strength the client reads.
METALLIC = 0.9
ROUGHNESS = 0.25
REFLECT = 1.4

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
shield = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
for other in [o for o in bpy.context.scene.objects if o is not shield]:
    bpy.data.objects.remove(other)
bpy.context.view_layer.objects.active = shield
shield.select_set(True)
bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def bounds():
    points = [v.co for v in shield.data.vertices]
    return (Vector([min(p[i] for p in points) for i in range(3)]),
            Vector([max(p[i] for p in points) for i in range(3)]))


# Front -Y in Blender (glTF +Z) turns to +Y (glTF -Z); then scale and seat.
low, high = bounds()
height = high.z - low.z
shield.rotation_mode = 'XYZ'
shield.rotation_euler = (0, 0, math.pi)
shield.scale = (DIAMETER / height,) * 3
bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
low, high = bounds()
grip = Vector(((low.x + high.x) / 2, low.y + BACK_BEHIND_GRIP, low.z + (high.z - low.z) * GRIP_SHARE))
shield.data.transform(Matrix.Translation(-grip))
shield.name = shield.data.name = 'mirror_shield'

for image in bpy.data.images:
    if image.size[0] > TEXTURE:
        image.scale(TEXTURE, TEXTURE)

WORK.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=str(WORK))
bpy.ops.export_scene.gltf(
    filepath=str(GAME), export_format='GLB', use_selection=True, export_animations=False,
    export_image_format='WEBP', export_image_quality=90, export_yup=True,
)

gltf, binary = read_glb(GAME)
for material in gltf['materials']:
    pbr = material.setdefault('pbrMetallicRoughness', {})
    pbr['metallicFactor'] = METALLIC
    pbr['roughnessFactor'] = ROUGHNESS
    material.setdefault('extras', {})['envReflect'] = REFLECT
write_glb(GAME, gltf, binary)

for location, energy in [((.7, -.9, 1.1), 45), ((-1, -.6, .4), 14), ((-.4, 1, .8), 22)]:
    add_light(location, energy, 1.5)
scene = bpy.context.scene
scene.world = bpy.data.worlds.new('Icon studio')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.55, .57, .62, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = 1.2
render_icon([shield], str(ICON), (math.radians(78), 0, math.radians(174)), margin=1.1)

low, high = bounds()
print(json.dumps({
    'triangles': sum(len(p.vertices) - 2 for p in shield.data.polygons),
    'size_m': [round(high[i] - low[i], 3) for i in range(3)],
    'bytes': GAME.stat().st_size,
}))
