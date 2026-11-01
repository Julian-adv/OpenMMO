import hashlib
import json
from pathlib import Path
import runpy
import sys
import tempfile

import bpy
from mathutils import Matrix
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/items/shadowfang/tripo_v1/source.glb'
LENGTH = 0.42
GRIP = 0.18
# Source x where the guard ends; faces past it are blade.
BLADE_START_X = -0.08
RUNE_COLOR = np.array([0.55, 0.3, 1.0], dtype=np.float32)
RUNE_STRENGTH = 1.5


def blade_uv_mask(mesh, width, height):
    mesh.calc_loop_triangles()
    uv = mesh.uv_layers.active.data
    mask = np.zeros((height, width), dtype=bool)
    for tri in mesh.loop_triangles:
        if sum(mesh.vertices[v].co.x for v in tri.vertices) / 3 <= BLADE_START_X:
            continue
        pts = np.array([[uv[loop].uv.x * width, uv[loop].uv.y * height] for loop in tri.loops])
        x0, y0 = np.maximum(np.floor(pts.min(0)).astype(int), 0)
        x1, y1 = np.minimum(np.ceil(pts.max(0)).astype(int), (width - 1, height - 1))
        xs, ys = np.meshgrid(np.arange(x0, x1 + 1), np.arange(y0, y1 + 1))
        p = np.stack([xs + 0.5, ys + 0.5], -1)
        a, b, c = pts
        w = [(q1[0] - q0[0]) * (p[..., 1] - q0[1]) - (q1[1] - q0[1]) * (p[..., 0] - q0[0])
             for q0, q1 in ((b, c), (c, a), (a, b))]
        inside = ((w[0] >= 0) & (w[1] >= 0) & (w[2] >= 0)) | ((w[0] <= 0) & (w[1] <= 0) & (w[2] <= 0))
        mask[ys[inside], xs[inside]] = True
    return mask


bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
item = next(obj for obj in bpy.context.scene.objects if obj.type == 'MESH')
item.parent = None
item.matrix_world = Matrix.Identity(4)
material = item.data.materials[0]
shader = material.node_tree.nodes.get('Principled BSDF')
color = shader.inputs['Base Color'].links[0].from_node.image
width, height = color.size
pixels = np.empty(width * height * 4, dtype=np.float32)
color.pixels.foreach_get(pixels)
pixels = pixels.reshape(height, width, 4)
red, green, blue = pixels[..., 0], pixels[..., 1], pixels[..., 2]
glow = np.clip(((red + green + blue) / 3 - 0.3) * 5, 0, 1)
glow *= np.clip((blue - green - 0.08) * 12, 0, 1)
glow *= blade_uv_mask(item.data, width, height)
emission_pixels = np.ones((height, width, 4), dtype=np.float32)
emission_pixels[..., :3] = glow[..., None] * RUNE_COLOR
emission = bpy.data.images.new('ShadowfangRunes', width=width, height=height)
emission.colorspace_settings.name = color.colorspace_settings.name
emission.pixels.foreach_set(emission_pixels.ravel())
emission.pack()
node = material.node_tree.nodes.new('ShaderNodeTexImage')
node.image = emission
material.node_tree.links.new(node.outputs['Color'], shader.inputs['Emission Color'])
shader.inputs['Emission Strength'].default_value = RUNE_STRENGTH
material.use_backface_culling = True

with tempfile.TemporaryDirectory(prefix='shadowfang-') as directory:
    prepared = Path(directory) / 'shadowfang.glb'
    bpy.ops.export_scene.gltf(filepath=str(prepared), export_format='GLB', export_animations=False)
    sys.argv = [str(__file__), '--', '--source', str(prepared), '--name', 'shadowfang',
                '--category', 'weapons', '--size', str(LENGTH), '--grip-fraction', str(GRIP),
                '--keep-emission', '--icon-rotation', '90', '0', '40']
    runpy.run_path(str(ROOT / 'tools/blender-scripts/export_item_asset.py'), run_name='__main__')
    (ROOT / 'assets/items/shadowfang/shadowfang-render.png').unlink(missing_ok=True)

item = bpy.data.objects['shadowfang']
generation = json.loads((SOURCE.parent / 'generation.json').read_text())
record = {
    'date': '2026-11-01',
    'source': str(SOURCE.relative_to(ROOT)),
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'generation_record': 'assets/items/shadowfang/tripo_v1/generation.json',
    'concept_record': generation['concept_record'],
    'tool': generation['tool'],
    'tier': generation['tier'],
    'license': generation['license'],
    'method': 'Preserve generated topology and UVs; bake metric size and a grip origin 6.3 cm behind '
              'the guard like dagger.glb; derive violet rune emission from bright violet base color '
              'texels inside the blade UV islands; resize all maps to 512px.',
    'length_m': LENGTH,
    'grip_fraction_from_pommel': GRIP,
    'rune_emission': {'color': RUNE_COLOR.tolist(), 'strength': RUNE_STRENGTH,
                      'blade_start_x_source': BLADE_START_X},
    'triangles': sum(len(poly.vertices) - 2 for poly in item.data.polygons),
    'dimensions_blender_xyz_m': list(item.dimensions),
    'scale': list(item.scale),
    'model': 'client/public/models/weapons/shadowfang.glb',
    'icon': 'client/public/items/weapons/shadowfang.png',
    'blender': 'assets/items/shadowfang/shadowfang.blend',
}
(ROOT / 'doc/assets/shadowfang-model.json').write_text(json.dumps(record, indent=2) + '\n')
print('SHADOWFANG', json.dumps(record))
