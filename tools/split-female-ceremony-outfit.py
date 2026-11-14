"""Cut the ceremony outfit off the blue mannequin it was generated on and name its parts.

The mannequin is a render of the female body on 30° heels, so its generated copy is aligned to that pose
(rotation, uniform scale, translation) and the outfit comes along. Blue triangles are dropped.

.venv/bin/python tools/split-female-ceremony-outfit.py [--version v2]
"""
import argparse
import hashlib
import io as bytes_io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
# v1 wears the veil, which hid the bodice back; v2 was generated without it for the bodice and skirt.
parser = argparse.ArgumentParser()
parser.add_argument('--version', choices=['v1', 'v2'], default='v1')
DIRECTORY = ROOT / f'assets/items/female_ceremony_outfit/tripo_multiview_{parser.parse_args().version}'
SOURCE = DIRECTORY / 'model_url.glb'
# Height above the lifted floor where the stockings end, inside the skirt.
STOCKING_TOP = .78
# Metres above the gold belt's top edge where the bodice is cut from the skirt.
BELT_MARGIN = .002
# Metres past the gold cuff band where each glove is cut.
CUFF_MARGIN = .001

doc, binary = read_glb(SOURCE)
binary = bytearray(binary)
node = doc['nodes'][doc['scenes'][0]['nodes'][0]]
primitive = doc['meshes'][node['mesh']]['primitives'][0]
attributes = primitive['attributes']
matrix = np.array(node.get('matrix', np.eye(4).ravel().tolist())).reshape(4, 4).T
points = accessor(doc, binary, attributes['POSITION']).astype(float) @ matrix[:3, :3].T + matrix[:3, 3]
normals = accessor(doc, binary, attributes['NORMAL']).astype(float) @ matrix[:3, :3].T
uv = accessor(doc, binary, attributes['TEXCOORD_0']).astype(float)
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3).astype(int)
material = doc['materials'][primitive['material']]
image = doc['images'][doc['textures'][material['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
view = doc['bufferViews'][image['bufferView']]
start = view.get('byteOffset', 0)
colour = np.asarray(Image.open(bytes_io.BytesIO(bytes(binary[start:start + view['byteLength']]))).convert('RGB'))
size = colour.shape[0]
warmth = colour[:, :, 0].astype(int) - colour[:, :, 2]
# The outfit is white and gold; only the mannequin is clearly bluer than it is red.
blue_texel = warmth < -15


def face_share(mask, triangles):
    corners = uv[triangles]
    centre = corners.mean(1)
    samples = [centre] + [centre + (corners[:, i] - centre) * .6 for i in range(3)] \
        + [(corners[:, i] + corners[:, (i + 1) % 3]) / 2 * .7 + centre * .3 for i in range(3)]

    def texel(coords):
        pixels = np.clip((coords % 1.0) * size, 0, size - 1).astype(int)
        return mask[pixels[:, 1], pixels[:, 0]]
    return np.mean([texel(s) for s in samples], 0)


def weld(positions):
    return np.unique(np.round(positions, 5), axis=0, return_inverse=True)[1].ravel()


def components(positions, triangles):
    welded = weld(positions)[triangles]
    count = welded.max() + 1
    graph = coo_matrix((np.ones(len(welded) * 2), (np.r_[welded[:, 0], welded[:, 1]], np.r_[welded[:, 1], welded[:, 2]])),
                       shape=(count, count))
    return connected_components(graph, directed=False)[1][welded[:, 0]]


share = face_share(blue_texel, faces)
blue = share > .5
# Even out the cut: a mannequin triangle with outfit on two sides is a notch and joins the outfit, and an outfit
# triangle with outfit on one side at most is a spike and goes.
sides = {}
for index, triangle in enumerate(weld(points)[faces]):
    for i in range(3):
        sides.setdefault(tuple(sorted((triangle[i], triangle[(i + 1) % 3]))), []).append(index)
around = [[] for _ in faces]
for group in sides.values():
    for index in group:
        around[index] += [other for other in group if other != index]
# Beside the mannequin a triangle with any blue in it goes too: part-blue ones stayed on the chest as white teeth
# along the neckline. Elsewhere cool shading on the white cloth reads as blue, and dropping it tore the hem band.
# Only beside real mannequin: specks of cool shading are not seeds.
labels = components(points, faces[blue])
beside = np.zeros(len(faces), bool)
beside[np.flatnonzero(blue)[np.bincount(labels)[labels] >= 200]] = True
for _ in range(3):
    beside |= (share > 0) & np.array([any(beside[other] for other in near) for near in around])
blue |= beside
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2

# Align the mannequin to the body standing on heels with its soles on the floor. The source faces +X and the body +Z.
body_doc, body_binary = read_glb(BODY)
skin = body_doc['meshes'][0]['primitives'][0]['attributes']
rest = accessor(body_doc, body_binary, skin['POSITION']).astype(float)
joints = accessor(body_doc, body_binary, skin['JOINTS_0']).astype(int)
weights = accessor(body_doc, body_binary, skin['WEIGHTS_0']).astype(float)
deformation = heel_pose(body_doc, body_binary, degrees=30)[0]
homogeneous = np.c_[rest, np.ones(len(rest))]
target = np.einsum('vj,vjab,vb->va', weights, deformation[joints], homogeneous)[:, :3]
lift = -target[:, 1].min()
target[:, 1] += lift
tree = cKDTree(target)
labels = components(points, faces[blue])
mannequin = np.unique(faces[blue][labels == np.bincount(labels).argmax()])
moving = points[mannequin]
rotation = np.array([[0, 0, -1], [0, 1, 0], [1, 0, 0]], float)
scale = (target[:, 1].max() - .0) / (np.ptp(points[:, 1]) * .97)
offset = np.array([0, target[:, 1].max() - scale * moving[:, 1].max(), 0])
for step in range(80):
    placed = scale * moving @ rotation.T + offset
    distance, nearest = tree.query(placed)
    keep = distance < (.06 if step < 30 else .015)
    a, b = moving[keep], target[nearest[keep]]
    a0, b0 = a.mean(0), b.mean(0)
    u, s, vt = np.linalg.svd((b - b0).T @ (a - a0))
    sign = np.diag([1, 1, np.sign(np.linalg.det(u @ vt))])
    rotation = u @ sign @ vt
    scale = (s * np.diag(sign)).sum() / ((a - a0) ** 2).sum()
    offset = b0 - scale * rotation @ a0
distance = tree.query(scale * moving @ rotation.T + offset)[0]
points = scale * points @ rotation.T + offset
normals = normals @ rotation.T
fit = {'scale': scale, 'rotation': rotation.tolist(), 'translation': offset.tolist(), 'heel_lift_m': lift,
       'mannequin_vertices': len(mannequin), 'mean_distance_m': float(distance.mean()),
       'p95_distance_m': float(np.percentile(distance, 95))}


def clip(triangles, height, normal=(0, 1, 0)):
    """Split triangles crossing the plane `normal · p = height` so each lies on one side of it."""
    global points, normals, uv
    normal = np.asarray(normal, float)
    side = points[triangles] @ normal > height
    crossing = side.any(1) & ~side.all(1)
    made, added = {}, []

    def cut(a, b):
        key = (min(a, b), max(a, b))
        if key not in made:
            t = (height - points[a] @ normal) / ((points[b] - points[a]) @ normal)
            shading = normals[a] + t * (normals[b] - normals[a])
            added.append((points[a] + t * (points[b] - points[a]), shading / np.linalg.norm(shading), uv[a] + t * (uv[b] - uv[a])))
            made[key] = len(points) + len(added) - 1
        return made[key]

    result = [triangles[~crossing]]
    for triangle, above in zip(triangles[crossing], side[crossing]):
        # Rotate so `a` is the corner alone on its side.
        a, b, c = np.roll(triangle, -int(np.flatnonzero(above == (above.sum() == 1))[0]))
        ab, ac = cut(a, b), cut(a, c)
        result.append(np.array([[a, ab, ac], [ab, b, c], [ab, c, ac]]))
    if added:
        points = np.vstack([points, [x[0] for x in added]])
        normals = np.vstack([normals, [x[1] for x in added]])
        uv = np.vstack([uv, [x[2] for x in added]])
    return np.vstack(result)


keep = faces[~blue]
labels = components(points, keep)
order = sorted(np.unique(labels), key=lambda label: -(labels == label).sum())
extent = {label: (points[np.unique(keep[labels == label])].min(0), points[np.unique(keep[labels == label])].max(0))
          for label in order}
parts = {}
report_components = []
gold_texel = warmth > 40
for label in order:
    triangles = keep[labels == label]
    low, high = extent[label]
    centre = (low + high) / 2
    report_components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) < 20:
        continue
    if low[1] < .1:
        name = 'legs'
    elif high[1] > 1.6:
        name = 'helmet'
    elif abs(centre[0]) > .3 and np.ptp([low[0], high[0]]) < .45:
        name = 'left_glove' if centre[0] > 0 else 'right_glove'
    elif len(triangles) < 1000 and low[1] < 1.15:
        # Scraps of the hips and thighs left under the skirt once the mannequin between them is gone.
        continue
    else:
        name = 'dress'
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}
legs = parts['legs']
if np.abs(points[np.unique(legs)][:, 0]).max() > .45:
    # v2 fused the skirt to the stockings: what stands off the legs above the shoes, or lies above the legs, is skirt.
    # Loose triangles of the same lump up at the chest are not: they showed as shards inside the neckline.
    centres = points[legs].mean(1)
    skirt = ((centres[:, 1] > .9 + lift) | ((tree.query(centres)[0] > .03) & (centres[:, 1] > .4))) \
        & (centres[:, 1] < 1.3 + lift)
    parts['dress'] = np.vstack([parts['dress'], legs[skirt]])
    parts['legs'] = legs[~skirt]

dress = parts.pop('dress')
gold = face_share(gold_texel, dress) > .5
heights = points[dress].mean(1)[:, 1]
band = gold & (heights > 1.05 + lift) & (heights < 1.25 + lift)
belt_top = float(np.percentile(points[dress[band]][:, :, 1].max(1), 90)) + BELT_MARGIN
dress = clip(dress, belt_top)
above = points[dress].mean(1)[:, 1] > belt_top
parts['top'], parts['skirt'] = dress[above], dress[~above]

# Loose scraps: pieces of a few triangles joined to nothing else.
labels = components(points, parts['top'])
parts['top'] = parts['top'][np.bincount(labels)[labels] >= 20]

# Pale mannequin triangles stayed on the arm above each glove: the glove ends at the plane of its gold band's far edge.
for name in ('left_glove', 'right_glove'):
    glove = parts[name]
    band = np.unique(glove[face_share(gold_texel, glove) > .5])
    centre = points[band].mean(0)
    across = np.linalg.svd(points[band] - centre)[2][2]
    across *= np.sign((centre - points[np.unique(glove)].mean(0)) @ across)
    edge = float(np.percentile(points[band] @ across, 98)) + CUFF_MARGIN
    glove = clip(glove, edge, across)
    parts[name] = glove[points[glove].mean(1) @ across < edge]

legs = clip(parts.pop('legs'), STOCKING_TOP + lift)
legs = legs[points[legs].mean(1)[:, 1] < STOCKING_TOP + lift]
left = points[legs].mean(1)[:, 0] > 0
parts['left_boots'], parts['right_boots'] = legs[left], legs[~left]

# Blue that bled into the outfit's texels takes the nearest outfit colour.
spread = ndimage.binary_dilation(warmth < -40, iterations=3)
nearest = ndimage.distance_transform_edt(spread, return_distances=False, return_indices=True)
packed = bytes_io.BytesIO()
Image.fromarray(colour[nearest[0], nearest[1]]).save(packed, 'PNG')
binary.extend(b'\0' * (-len(binary) % 4))
doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': packed.getbuffer().nbytes})
binary += packed.getvalue()
image.update(bufferView=len(doc['bufferViews']) - 1, mimeType='image/png')

doc['meshes'], doc['nodes'] = [], []
for name, triangles in parts.items():
    used, compact = np.unique(triangles, return_inverse=True)
    doc['meshes'].append({'name': name, 'primitives': [{
        'attributes': {'POSITION': add_accessor(doc, binary, points[used], 'VEC3'),
                       'NORMAL': add_accessor(doc, binary, normals[used], 'VEC3'),
                       'TEXCOORD_0': add_accessor(doc, binary, uv[used], 'VEC2')},
        'indices': add_accessor(doc, binary, compact.ravel(), 'SCALAR', 5125),
        'material': primitive['material']}]})
    doc['nodes'].append({'name': name, 'mesh': len(doc['meshes']) - 1})
doc['scenes'][0]['nodes'] = list(range(len(doc['nodes'])))
doc['buffers'][0]['byteLength'] = len(binary)
output = DIRECTORY / 'segmented.glb'
write_glb(output, doc, binary)
report = {'tool': 'tools/split-female-ceremony-outfit.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'belt_top_m': belt_top, 'stocking_top_m': STOCKING_TOP + lift, 'components': report_components[:12],
          'parts': {name: {'triangles': len(t), 'min': points[np.unique(t)].min(0).round(3).tolist(),
                           'max': points[np.unique(t)].max(0).round(3).tolist()} for name, t in parts.items()},
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
