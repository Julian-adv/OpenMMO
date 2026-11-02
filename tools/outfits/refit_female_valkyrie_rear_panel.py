import hashlib
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mesh_components import welded_islands

SOURCE = ROOT / 'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_v5/bottom_female_valkyrie.glb'
OUTPUT = SOURCE.parent.parent / 'fitted_v7'
OUTPUT.mkdir(exist_ok=True)
doc, binary = read_glb(SOURCE)
raw = bytearray(binary)
primitive = doc['meshes'][0]['primitives'][0]
attrs = {name: accessor(doc, binary, index) for name, index in primitive['attributes'].items()}
positions = attrs['POSITION']
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
assert len(positions) == 4217 and len(faces) == 3571
_, _, labels = welded_islands(positions, faces)
central = sorted(np.unique(labels[:3534]), key=lambda label: np.count_nonzero(labels[:3534] == label), reverse=True)[2]
panel = sorted(np.unique(labels[3534:]), key=lambda label: np.count_nonzero(labels[3534:] == label), reverse=True)
panel_main = panel[0]


def smooth(value):
    t = np.clip(value, 0, 1)
    return t * t * (3 - 2 * t)


def shape(point):
    point = np.asarray(point).copy()
    x, y, z = point
    point[0] = x * (1 - .24 * smooth((y - 1.0) / .10))
    point[1] = y + .015
    point[2] = z + .008 * smooth((y - .90) / .20)
    return point


for label in panel:
    ids = np.flatnonzero(labels == label)
    ids = ids[ids >= 3534]
    if label != panel_main:
        anchor = positions[ids].mean(axis=0)
        attrs['POSITION'][ids] += shape(anchor) - anchor
        continue
    for index in ids:
        point = positions[index].copy()
        eps = 1e-4
        jacobian = np.column_stack([(shape(point + np.eye(3)[axis] * eps) -
                                     shape(point - np.eye(3)[axis] * eps)) / (2 * eps)
                                    for axis in range(3)])
        normal = np.linalg.solve(jacobian.T, attrs['NORMAL'][index])
        normal /= np.linalg.norm(normal)
        tangent = jacobian @ attrs['TANGENT'][index, :3]
        tangent -= normal * np.dot(normal, tangent)
        tangent /= np.linalg.norm(tangent)
        attrs['POSITION'][index] = shape(point)
        attrs['NORMAL'][index] = normal
        attrs['TANGENT'][index, :3] = tangent

extra = {name: [] for name in attrs}


def interpolated(a, b, t):
    vertex = {}
    for name in attrs:
        if name not in ('JOINTS_0', 'WEIGHTS_0'):
            vertex[name] = (1 - t) * attrs[name][a] + t * attrs[name][b]
    influences = {}
    for index, factor in ((a, 1 - t), (b, t)):
        for joint, weight in zip(attrs['JOINTS_0'][index], attrs['WEIGHTS_0'][index]):
            influences[int(joint)] = influences.get(int(joint), 0) + factor * float(weight)
    weights = sorted(influences.items(), key=lambda pair: pair[1], reverse=True)[:4]
    vertex['JOINTS_0'] = np.array([joint for joint, _ in weights] + [0] * (4 - len(weights)))
    vertex['WEIGHTS_0'] = np.array([weight for _, weight in weights] + [0] * (4 - len(weights)))
    vertex['WEIGHTS_0'] /= sum(vertex['WEIGHTS_0'])
    for name in ('NORMAL', 'TANGENT'):
        vertex[name][:3] /= max(np.linalg.norm(vertex[name][:3]), 1e-12)
    return vertex


def intersection(a, b):
    t = (-.07 - positions[a, 2]) / (positions[b, 2] - positions[a, 2])
    value = interpolated(a, b, t)
    index = len(positions) + len(extra['POSITION'])
    for name in attrs:
        extra[name].append(value[name])
    return index


new_faces = []
strips = {586, 587, 588, 589, 2144, 2145, 2146}
for face_index, face in enumerate(faces):
    if face_index in strips:
        continue
    if face_index >= 2805 or labels[face[0]] != central:
        new_faces.append(face.tolist())
        continue
    polygon = []
    for i, current in enumerate(face):
        previous = face[i - 1]
        inside = positions[current, 2] >= -.07
        previous_inside = positions[previous, 2] >= -.07
        if inside != previous_inside:
            polygon.append(intersection(int(previous), int(current)))
        if inside:
            polygon.append(int(current))
    for i in range(1, len(polygon) - 1):
        new_faces.append([polygon[0], polygon[i], polygon[i + 1]])

for name, old_index in list(primitive['attributes'].items()):
    old = doc['accessors'][old_index]
    added = np.array(extra[name], dtype=attrs[name].dtype).reshape(-1, attrs[name].shape[1])
    values = np.concatenate([attrs[name], added])
    index = add_accessor(doc, raw, values, old['type'], old['componentType'])
    if old.get('normalized'):
        doc['accessors'][index]['normalized'] = True
    primitive['attributes'][name] = index
indices = np.asarray(new_faces, dtype=np.uint16).reshape(-1, 1)
primitive['indices'] = add_accessor(doc, raw, indices, 'SCALAR', 5123)
doc['buffers'] = [{'byteLength': len(raw)}]
output = OUTPUT / 'bottom_female_valkyrie.glb'
write_glb(output, doc, raw)
mask = json.loads((SOURCE.parent / 'body-occlusion.json').read_text())
mask['bottom_sha256'] = hashlib.sha256(output.read_bytes()).hexdigest()
body = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
assert hashlib.sha256(body.read_bytes()).hexdigest() == mask['body_sha256']
body_doc, body_raw = read_glb(body)
body_primitive = body_doc['meshes'][0]['primitives'][0]
body_points = accessor(body_doc, body_raw, body_primitive['attributes']['POSITION'])
body_faces = accessor(body_doc, body_raw, body_primitive['indices']).reshape(-1, 3)
_, _, body_labels = welded_islands(body_points, body_faces)
underwear_label = body_labels[body_faces[835, 0]]
center = body_points[body_faces].mean(axis=1)
underwear = ((body_labels[body_faces[:, 0]] == underwear_label) & (center[:, 2] < -.035) &
             (center[:, 1] > .96) & (np.abs(center[:, 0]) < .18))
skin = [4789, 4802, 4845, 4847, 4850, 4851, 4854, 4860, 4863, 4864, 4869, 4872, 4873,
        4912, 4915, 4962, 8968, 8972, 8973, 9002, 9003, 9004, 9007, 9008, 9009, 9016,
        9029, 9030, 9031, 9040, 9084, 14700, 14713, 14756, 14757, 14758, 14761, 14762,
        14765, 14769, 14771, 14780, 14783, 14784, 14823, 14827, 18883, 18884, 18913,
        18914, 18915, 18919, 18940, 18941, 18942, 18951, 18995, 19044]
assert np.count_nonzero(underwear) == 364 and not underwear[skin].any()
mask['hide_triangle_indices'] = sorted(set(np.flatnonzero(underwear).tolist()) | set(skin))
mask['scope'] = 'Hide rear underwear and body faces intersecting the armor during sampled motions.'
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
report = {'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'bottom_sha256': mask['bottom_sha256'], 'triangles': len(new_faces),
          'removed_strip_triangles': len(strips), 'clipped_vertices': len(extra['POSITION']),
          'status': 'Rear panel fitting candidate; motion and visual review required.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
