from pathlib import Path
import math
import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[2]
LENGTH = 1.85
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(ROOT / 'assets/skeleton_greatsword/source.glb'))
mesh = next(obj for obj in bpy.context.scene.objects if obj.type == 'MESH')
mesh.data.transform(mesh.matrix_world)
mesh.matrix_world = Matrix.Identity(4)
low = min(v.co.z for v in mesh.data.vertices)
high = max(v.co.z for v in mesh.data.vertices)
factor = LENGTH / (high - low)
transform = Matrix.Rotation(math.pi / 2, 4, 'Y') @ Matrix.Scale(factor, 4) @ Matrix.Translation(Vector((0, 0, -low - (high-low)*.20)))
mesh.data.transform(transform)
mesh.name = 'SkeletonGreatsword'
for obj in list(bpy.data.objects):
    if obj != mesh:
        bpy.data.objects.remove(obj, do_unlink=True)
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT / 'assets/skeleton_greatsword/skeleton_greatsword.blend'))
bpy.ops.export_scene.gltf(filepath=str(ROOT / 'client/public/models/weapons/skeleton_greatsword.glb'), export_format='GLB', export_animations=False)
print('SWORD', len(mesh.data.polygons), 'faces', LENGTH, 'm; blade tip +X=1.48, grip origin=0')
