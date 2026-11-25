"""The blindfold's dropped-item model and icon, from the male fitted part.

    blender -b -P tools/blender-scripts/build_blindfold.py
"""
import math
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/blender-scripts'))
from icon_render import add_light, render_icon  # noqa: E402

SOURCE = ROOT / 'assets/items/blindfold/tripo_male_v1/fitted/helmet_blindfold.glb'
GAME = ROOT / 'client/public/models/armor/blindfold.glb'
ICON = ROOT / 'client/public/items/armor/blindfold.png'
TEXTURE = 512

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
band = next(o for o in bpy.context.scene.objects if o.type == 'MESH')
for modifier in list(band.modifiers):
    band.modifiers.remove(modifier)
band.parent = None
for other in [o for o in bpy.context.scene.objects if o is not band]:
    bpy.data.objects.remove(other)
band.vertex_groups.clear()
bpy.context.view_layer.objects.active = band
band.select_set(True)
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
points = [v.co for v in band.data.vertices]
low = Vector([min(p[i] for p in points) for i in range(3)])
high = Vector([max(p[i] for p in points) for i in range(3)])
# Origin at the bottom centre, as dropped items sit on the ground.
band.data.transform(Matrix.Translation(-Vector(((low.x + high.x) / 2, (low.y + high.y) / 2, low.z))))
band.name = band.data.name = 'blindfold'
for image in bpy.data.images:
    if image.size[0] > TEXTURE:
        image.scale(TEXTURE, TEXTURE)

bpy.ops.export_scene.gltf(
    filepath=str(GAME), export_format='GLB', use_selection=True, export_animations=False, export_skins=False,
    export_image_format='WEBP', export_image_quality=90, export_yup=True,
)

for location, energy in [((.7, -.9, 1.1), 45), ((-1, -.6, .4), 14), ((-.4, 1, .8), 22)]:
    add_light(location, energy, 1.5)
scene = bpy.context.scene
scene.world = bpy.data.worlds.new('Icon studio')
scene.world.use_nodes = True
scene.world.node_tree.nodes['Background'].inputs[0].default_value = (.55, .57, .62, 1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value = 1.2
render_icon([band], str(ICON), (math.radians(62), 0, math.radians(200)), margin=1.1)
print('triangles', sum(len(p.vertices) - 2 for p in band.data.polygons), 'size', [round(high[i] - low[i], 3) for i in range(3)])
