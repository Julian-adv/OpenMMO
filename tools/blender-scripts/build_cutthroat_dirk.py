import hashlib
import json
import math
from pathlib import Path
import runpy
import sys
import tempfile

import bpy
from mathutils import Matrix

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'assets/items/cutthroat_dirk/tripo_v1/source.glb'
LENGTH = 0.42
GRIP = 0.15
ROLL_DEGREES = 38

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.context.preferences.filepaths.save_version = 0
bpy.ops.import_scene.gltf(filepath=str(SOURCE))
item = next(obj for obj in bpy.context.scene.objects if obj.type == 'MESH')
item.parent = None
item.matrix_world = Matrix.Identity(4)
item.data.transform(Matrix.Rotation(math.radians(ROLL_DEGREES), 4, 'X'))
item.data.materials[0].use_backface_culling = True

with tempfile.TemporaryDirectory(prefix='cutthroat-dirk-') as directory:
    prepared = Path(directory) / 'cutthroat_dirk.glb'
    bpy.ops.export_scene.gltf(filepath=str(prepared), export_format='GLB', export_animations=False)
    sys.argv = [str(__file__), '--', '--source', str(prepared), '--name', 'cutthroat_dirk',
                '--category', 'weapons', '--size', str(LENGTH), '--grip-fraction', str(GRIP),
                '--icon-rotation', '90', '0', '40']
    runpy.run_path(str(ROOT / 'tools/blender-scripts/export_item_asset.py'), run_name='__main__')
    (ROOT / 'assets/items/cutthroat_dirk/cutthroat_dirk-render.png').unlink(missing_ok=True)

item = bpy.data.objects['cutthroat_dirk']
generation = json.loads((SOURCE.parent / 'generation.json').read_text())
record = {
    'date': '2026-11-01',
    'source': str(SOURCE.relative_to(ROOT)),
    'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'generation_record': 'assets/items/cutthroat_dirk/tripo_v1/generation.json',
    'concept_record': generation['concept_record'],
    'tool': generation['tool'],
    'tier': generation['tier'],
    'license': generation['license'],
    'method': 'Preserve generated topology and UVs; roll the blade plane onto Blender XZ; '
              'bake metric size and a grip origin 6.3 cm behind the guard like dagger.glb; '
              'resize the PBR maps to 512px.',
    'length_m': LENGTH,
    'grip_fraction_from_pommel': GRIP,
    'roll_degrees_about_x': ROLL_DEGREES,
    'triangles': sum(len(poly.vertices) - 2 for poly in item.data.polygons),
    'dimensions_blender_xyz_m': list(item.dimensions),
    'scale': list(item.scale),
    'model': 'client/public/models/weapons/cutthroat_dirk.glb',
    'icon': 'client/public/items/weapons/cutthroat_dirk.png',
    'blender': 'assets/items/cutthroat_dirk/cutthroat_dirk.blend',
}
(ROOT / 'doc/assets/cutthroat_dirk-model.json').write_text(json.dumps(record, indent=2) + '\n')
print('CUTTHROAT_DIRK', json.dumps(record))
