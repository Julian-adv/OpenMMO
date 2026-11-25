"""Cut the Floating Eye's generated eyelids off the blue mannequin they were generated on.

The mannequin is a render of the eye's own Tripo source, and Tripo returns both in the same normalized frame,
so the lids need no alignment: blue triangles are dropped. The mannequin's own back of the ball is kept as a
second piece, a hood painted in the lids' flesh for them to slide under.

.venv/bin/python tools/split-floating-eye-lids.py
"""
import hashlib
import io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor, add_view

ROOT = Path(__file__).resolve().parents[1]
EYE = ROOT / 'assets/monsters/floating_eye/tripo_v1/model_url.glb'
directory = ROOT / 'assets/monsters/floating_eye/eyelids'
source = directory / 'tripo_v1/model_url.glb'
out = directory / 'fitted_v1'
out.mkdir(exist_ok=True)
# How far forward of the ball's middle the hood reaches over the lids' back edge, as the sine of the angle.
HOOD_LAP = 0.2

def mesh_points(doc, binary):
    node = doc['nodes'][doc['scenes'][0]['nodes'][0]]
    primitive = doc['meshes'][node['mesh']]['primitives'][0]
    points = accessor(doc, binary, primitive['attributes']['POSITION']).astype(float)
    matrix = np.array(node.get('matrix', np.eye(4).ravel().tolist())).reshape(4, 4).T
    return node, primitive, points @ matrix[:3, :3].T + matrix[:3, 3], matrix


doc, binary = read_glb(source)
binary = bytearray(binary)
node, primitive, points, matrix = mesh_points(doc, binary)
node.pop('matrix', None)
attributes = primitive['attributes']
uv = accessor(doc, binary, attributes['TEXCOORD_0']).astype(float)
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3).astype(int)
material = doc['materials'][primitive['material']]
image = doc['images'][doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
colour = np.asarray(Image.open(io.BytesIO(view_bytes(doc, binary, image['bufferView']))).convert('RGB'))
size = colour.shape[0]
# Mannequin texels: blue, and the purple where it met the lids.
blue = colour[:, :, 0].astype(int) - colour[:, :, 2] < 30


def texel(coords):
    pixels = np.clip((coords % 1.0) * size, 0, size - 1).astype(int)
    return blue[pixels[:, 1], pixels[:, 0]]


corners = uv[faces]
centre = corners.mean(1)
samples = [centre] + [centre + (corners[:, i] - centre) * .6 for i in range(3)]
lid_face = np.mean([texel(s) for s in samples], 0) < .2

# Smooth the cut: a triangle stays lid only with lid on two sides.
order = np.lexsort(points.T)
unique = np.zeros(len(points), int)
unique[order] = np.cumsum(np.r_[0, (np.diff(points[order], axis=0) != 0).any(1)])
welded = unique[faces]
sides = {}
for index, triangle in enumerate(welded):
    for i in range(3):
        sides.setdefault(tuple(sorted((triangle[i], triangle[(i + 1) % 3]))), []).append(index)
around = [[] for _ in welded]
for group in sides.values():
    for index in group:
        around[index] += [other for other in group if other != index]
for _ in range(3):
    lid_face = np.array([sum(lid_face[other] for other in near) for near in around]) >= 2

eye_points = mesh_points(*read_glb(EYE))[2]
drift = float(np.abs(np.r_[points.min(0) - eye_points.min(0), points.max(0) - eye_points.max(0)]).max())
assert drift < 0.01, f'the generated mannequin sits {drift:.3f} off the eye'

attributes['POSITION'] = add_accessor(doc, binary, points.astype(np.float32), 'VEC3')
if 'NORMAL' in attributes:
    normals = accessor(doc, binary, attributes['NORMAL']).astype(float) @ matrix[:3, :3].T
    attributes['NORMAL'] = add_accessor(doc, binary, normals.astype(np.float32), 'VEC3')
# The hood: the ball behind the lids, lapping a little forward over their back edge. The model looks along +X.
top = points[:, 1].max()
radius = np.ptp(points[:, 2]) / 2
ball_centre = np.array([0.0, top - radius, 0.0])
face_centre = points[faces].mean(1) - ball_centre
hood_face = ~lid_face & (face_centre[:, 1] > -radius * .93) \
    & (face_centre[:, 0] < HOOD_LAP * np.linalg.norm(face_centre, axis=1))

# Bled blue takes the nearest lid colour; solid blue (the hood) the lids' median.
spread = ndimage.binary_dilation(blue, iterations=3)
distance, nearest = ndimage.distance_transform_edt(spread, return_indices=True)
cleaned = colour[nearest[0], nearest[1]]
cleaned[distance > 6] = np.median(colour[~spread], axis=0).astype(np.uint8)
packed = io.BytesIO()
Image.fromarray(cleaned).save(packed, 'PNG')
image.update(bufferView=add_view(doc, binary, packed.getvalue()), mimeType='image/png')
# The lids are open sheets once the mannequin is gone.
material['doubleSided'] = True
outputs = {}
for name, chosen in [('lids', lid_face), ('hood', hood_face)]:
    primitive['indices'] = add_accessor(doc, binary, faces[chosen].astype(np.uint32).ravel(), 'SCALAR', 5125)
    doc['buffers'][0]['byteLength'] = len(binary)
    target = out / f'{name}-split.glb'
    write_glb(target, doc, binary)
    outputs[name] = str(target.relative_to(ROOT))
report = {'tool': 'tools/split-floating-eye-lids.py', 'source': str(source.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'eye': str(EYE.relative_to(ROOT)),
          'frame_drift': drift, 'source_triangles': len(faces), 'lid_triangles': int(lid_face.sum()),
          'hood_triangles': int(hood_face.sum()), 'blue_texels_filled': int(spread.sum()), 'outputs': outputs}
(out / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
