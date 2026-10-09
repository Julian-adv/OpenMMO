"""Repair the front undershirt layer without repainting the source atlas."""
import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('rogue', ROOT / 'tools/fit-tripo-rogue.py')
rogue = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rogue)
source = rogue.OUTPUT / 'top_rogue-undershirt-before-v1.glb'
assert hashlib.sha256(source.read_bytes()).hexdigest() == '8fe1cbf6c51b8e9e0b47a5c565390aca257972086c1f02dd65f7fce4e869dc83'
doc, raw = rogue.fit.read_glb(source)
images = [rogue.view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
primitive = doc['meshes'][0]['primitives'][0]
attrs = primitive['attributes']
points = rogue.io.accessor(doc, raw, attrs['POSITION'])
uv = rogue.io.accessor(doc, raw, attrs['TEXCOORD_0']).copy()
faces = rogue.io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
patch = (uv[:, 0] > .065) & (uv[:, 0] < .235) & (uv[:, 1] > .475) & (uv[:, 1] < .54)
_, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
linen = (np.bincount(welded, weights=patch) > 0)[welded]
amount = rogue.io.smoothstep((points[:, 1] - 1.04) / .04)
amount *= 1 - rogue.io.smoothstep((points[:, 1] - 1.16) / .05)
amount *= 1 - rogue.io.smoothstep((abs(points[:, 0]) - .16) / .04)
amount *= rogue.io.smoothstep((points[:, 2] - .015) / .035) * linen
result = points.copy()
result[:, 2] -= .012 * amount
centers = points[faces].mean(1)
face_normals = np.cross(points[faces[:, 1]] - points[faces[:, 0]], points[faces[:, 2]] - points[faces[:, 0]])
face_normals /= np.maximum(np.linalg.norm(face_normals, axis=1, keepdims=True), 1e-9)
removed = (face_normals[:, 2] < -.5) & (centers[:, 2] > .035)
removed &= (centers[:, 1] > 1.075) & (centers[:, 1] < 1.18) & (abs(centers[:, 0]) < .18)
retained_faces = faces[~removed]
remapped = patch & (abs(points[:, 0]) < .2) & (points[:, 1] < 1.20) & (points[:, 1] > 1.04)
uv[remapped, 0] = .69921875 + np.clip((points[remapped, 0] + .18) / .36, 0, 1) * .046875
uv[remapped, 1] = .49365234375 + np.clip((1.16 - points[remapped, 1]) / .12, 0, 1) * .046875
changed = np.any(result != points, axis=1)
normal_changed = changed.copy()
normal_changed[faces[np.any(changed[faces], axis=1) | removed].ravel()] = True
normals = rogue.io.smooth_normals(result, retained_faces)
original_normals = rogue.io.accessor(doc, raw, attrs['NORMAL'])
normals[~normal_changed] = original_normals[~normal_changed]
binary = bytearray(raw)
attrs['POSITION'] = rogue.io.add_accessor(doc, binary, result, 'VEC3')
attrs['NORMAL'] = rogue.io.add_accessor(doc, binary, normals, 'VEC3')
attrs['TEXCOORD_0'] = rogue.io.add_accessor(doc, binary, uv, 'VEC2')
primitive['indices'] = rogue.io.add_accessor(doc, binary, retained_faces.reshape(-1, 1), 'SCALAR', 5125)
if 'TANGENT' in attrs:
    tangents = rogue.io.tangents(result, normals, uv, retained_faces)
    tangents[~(normal_changed | remapped)] = rogue.io.accessor(doc, raw, attrs['TANGENT'])[~(normal_changed | remapped)]
    attrs['TANGENT'] = rogue.io.add_accessor(doc, binary, tangents, 'VEC4')
target = rogue.OUTPUT / 'top_rogue.glb'
rogue.fit.write_glb(target, doc, rogue.io.compact(doc, binary))
updated, data = rogue.fit.read_glb(target)
assert images == [rogue.view_bytes(updated, data, image['bufferView']) for image in updated['images']]
assert np.array_equal(result[:, :2], points[:, :2])
report = dict(date='2026-10-09', source=dict(path=str(source.relative_to(ROOT)), sha256=hashlib.sha256(source.read_bytes()).hexdigest()),
              output=dict(path=str(target.relative_to(ROOT)), sha256=hashlib.sha256(target.read_bytes()).hexdigest()),
              method='Inset front linen layer up to 12mm, remove overlapping inward-facing front leather caps, and remap the shadow-baked front linen UV patch to existing clean linen in the same unchanged atlas',
              modified_vertices=int(changed.sum()), remapped_uv_vertices=int(remapped.sum()),
              removed_face_indices=np.flatnonzero(removed).tolist(), final_triangles=len(retained_faces),
              linen_atlas_patch=[.69921875, .49365234375, .74609375, .54052734375],
              preservation=['X/Y coordinates', 'Skin indices and weights', 'Rig and bind matrices', 'Embedded image bytes and material', 'UVs outside the front linen patch', 'Upper torso, sleeves, collar and back geometry', 'Normals and tangents outside the affected neighborhood'],
              validation=rogue.fit.validate(target), new_ai_generation_or_paid_calls=False)
(ROOT / 'doc/assets/modular-rogue-undershirt-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({key: report[key] for key in ['modified_vertices', 'remapped_uv_vertices', 'removed_face_indices', 'final_triangles']}))
