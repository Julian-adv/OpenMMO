"""Cut a generated hair off the blue mannequin it was generated on and place it on the female body.

The mannequin is a render of our own body, so its generated copy is aligned to the body (rotation, uniform
scale, translation) and the hair comes along. Blue triangles are dropped; blue texels are filled with hair.

.venv/bin/python tools/split-female-long-hair.py --style side
"""
import argparse
import hashlib
import io as bytes_io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/faces/tripo_ranger_v1/base_ranger.glb'
STYLES = {'side': 'tripo_long_side_v3', 'bangs': 'tripo_long_bangs_v2'}
parser = argparse.ArgumentParser()
parser.add_argument('--style', choices=STYLES, required=True)
directory = ROOT / 'assets/modular_human_female_01/hair' / STYLES[parser.parse_args().style]
source = directory / 'model_url.glb'
out = directory / 'fitted_v1'
out.mkdir(exist_ok=True)

doc, binary = read_glb(source)
binary = bytearray(binary)
node = doc['nodes'][doc['scenes'][0]['nodes'][0]]
primitive = doc['meshes'][node['mesh']]['primitives'][0]
attributes = primitive['attributes']
points = accessor(doc, binary, attributes['POSITION']).astype(float)
matrix = np.array(node.pop('matrix', np.eye(4).ravel().tolist())).reshape(4, 4).T
points = points @ matrix[:3, :3].T + matrix[:3, 3]
uv = accessor(doc, binary, attributes['TEXCOORD_0']).astype(float)
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3).astype(int)
material = doc['materials'][primitive['material']]
image = doc['images'][doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
view = doc['bufferViews'][image['bufferView']]
start = view.get('byteOffset', 0)
colour = np.asarray(Image.open(bytes_io.BytesIO(bytes(binary[start:start + view['byteLength']]))).convert('RGB'))
size = colour.shape[0]
# Hair is blond (red well above blue); the mannequin is blue.
blue = colour[:, :, 0].astype(int) - colour[:, :, 2] < 30


def texel(coords):
    pixels = np.clip((coords % 1.0) * size, 0, size - 1).astype(int)
    return blue[pixels[:, 1], pixels[:, 0]]


corners = uv[faces]
centre = corners.mean(1)
samples = [centre] + [centre + (corners[:, i] - centre) * .6 for i in range(3)] \
    + [(corners[:, i] + corners[:, (i + 1) % 3]) / 2 * .7 + centre * .3 for i in range(3)]
# A triangle with any real share of blue is the mannequin's: half-blue ones left pale flaps along the hairline.
hair_face = np.mean([texel(s) for s in samples], 0) < .2

# Drop specks: hair pieces of a few triangles on the mannequin.
order = np.lexsort(points.T)
unique = np.zeros(len(points), int)
unique[order] = np.cumsum(np.r_[0, (np.diff(points[order], axis=0) != 0).any(1)])
welded = unique[faces]
parent = np.arange(unique.max() + 1)


def find(i):
    while parent[i] != i:
        parent[i] = parent[parent[i]]
        i = parent[i]
    return i


for a, b, c in welded[hair_face]:
    parent[find(a)] = find(b)
    parent[find(c)] = find(b)
roots = np.array([find(i) for i in welded[:, 0]])
counts = np.bincount(roots[hair_face], minlength=len(parent))
hair_face &= counts[roots] >= 40

# Even out the cut: a mannequin triangle with hair on two sides is a notch and joins the hair, and a hair
# triangle with hair on one side at most is a spike and goes.
sides = {}
for index, triangle in enumerate(welded):
    for i in range(3):
        sides.setdefault(tuple(sorted((triangle[i], triangle[(i + 1) % 3]))), []).append(index)
around = [[] for _ in welded]
for group in sides.values():
    for index in group:
        around[index] += [other for other in group if other != index]
for _ in range(3):
    hair_face = np.array([sum(hair_face[other] for other in near) for near in around]) >= 2

# Align the mannequin to the body. The source faces +X and the body +Z.
body_doc, body_binary = read_glb(BODY)
target = np.concatenate([accessor(body_doc, body_binary, p['attributes']['POSITION']).astype(float)
                         for mesh in body_doc['meshes'] for p in mesh['primitives']])
tree = cKDTree(target)
mannequin = np.unique(faces[~hair_face])
mannequin = mannequin[~np.isin(mannequin, faces[hair_face])]
moving = points[mannequin]
rotation = np.array([[0, 0, -1], [0, 1, 0], [1, 0, 0]], float)
scale, offset = .94, np.array([0, 1.34, -.02])
for step in range(60):
    placed = scale * moving @ rotation.T + offset
    distance, nearest = tree.query(placed)
    # The head and neck decide once the fit is close; arms and the cut waist differ.
    keep = distance < (.05 if step < 20 else .012)
    if step >= 30:
        keep &= placed[:, 1] > 1.52
    a, b = moving[keep], target[nearest[keep]]
    a0, b0 = a.mean(0), b.mean(0)
    u, s, vt = np.linalg.svd((b - b0).T @ (a - a0))
    sign = np.diag([1, 1, np.sign(np.linalg.det(u @ vt))])
    rotation = u @ sign @ vt
    scale = (s * np.diag(sign)).sum() / ((a - a0) ** 2).sum()
    offset = b0 - scale * rotation @ a0
placed = scale * moving @ rotation.T + offset
distance = tree.query(placed)[0]
head = placed[:, 1] > 1.52
fit = {'scale': scale, 'rotation': rotation.tolist(), 'translation': offset.tolist(),
       'mannequin_vertices': len(mannequin), 'head_vertices': int(head.sum()),
       'head_mean_distance_m': float(distance[head].mean()), 'head_p95_distance_m': float(np.percentile(distance[head], 95)),
       'all_mean_distance_m': float(distance.mean())}

points = scale * points @ rotation.T + offset
attributes['POSITION'] = add_accessor(doc, binary, points.astype(np.float32), 'VEC3')
doc['accessors'][attributes['POSITION']].update(min=points.min(0).tolist(), max=points.max(0).tolist())
turn = rotation @ matrix[:3, :3]
for name in ('NORMAL', 'TANGENT'):
    if name in attributes:
        values = accessor(doc, binary, attributes[name]).astype(float).copy()
        values[:, :3] = values[:, :3] @ turn.T
        attributes[name] = add_accessor(doc, binary, values.astype(np.float32), 'VEC3' if name == 'NORMAL' else 'VEC4')
primitive['indices'] = add_accessor(doc, binary, faces[hair_face].astype(np.uint32).ravel(), 'SCALAR', 5125)

# Blue that bled into the hair's texels takes the nearest hair colour.
spread = ndimage.binary_dilation(blue, iterations=3)
nearest = ndimage.distance_transform_edt(spread, return_distances=False, return_indices=True)
cleaned = colour[nearest[0], nearest[1]]
packed = bytes_io.BytesIO()
Image.fromarray(cleaned).save(packed, 'PNG')
while len(binary) % 4:
    binary.append(0)
doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': packed.getbuffer().nbytes})
binary += packed.getvalue()
image.update(bufferView=len(doc['bufferViews']) - 1, mimeType='image/png')
# The hair is a single open sheet once the mannequin is gone.
material['doubleSided'] = True
doc['buffers'][0]['byteLength'] = len(binary)
target_path = out / 'hair-split.glb'
write_glb(target_path, doc, binary)
report = {'tool': 'tools/split-female-long-hair.py', 'source': str(source.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'hair_triangles': int(hair_face.sum()),
          'blue_texels_filled': int(spread.sum()), 'output': str(target_path.relative_to(ROOT))}
(out / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
