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

SOURCE = ROOT / 'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_v4/bottom_female_valkyrie.glb'
OUTPUT = SOURCE.parent.parent / 'fitted_v5'
OUTPUT.mkdir(exist_ok=True)

doc, binary = read_glb(SOURCE)
raw = bytearray(binary)
primitive = doc['meshes'][0]['primitives'][0]
attributes = {name: accessor(doc, binary, index) for name, index in primitive['attributes'].items()}
positions = attributes['POSITION']
faces = accessor(doc, binary, primitive['indices']).reshape(-1, 3)
_, _, labels = welded_islands(positions, faces)
counts = np.bincount(labels)
central_label = sorted(np.flatnonzero(counts >= 200), key=lambda item: counts[item], reverse=True)[2]
accessories = np.flatnonzero(counts < 200)
selected = (labels[faces[:, 0]] == central_label) | np.isin(labels[faces[:, 0]], accessories)
face_points = positions[faces]
face_normals = np.cross(face_points[:, 1] - face_points[:, 0], face_points[:, 2] - face_points[:, 0])
selected &= (labels[faces[:, 0]] != central_label) | (face_normals[:, 2] > 0)


def clip(polygon, dimension, sign, limit):
    result = []
    for index, current in enumerate(polygon):
        previous = polygon[index - 1]
        a = sign * previous['POSITION'][dimension] - limit
        b = sign * current['POSITION'][dimension] - limit
        if (a >= 0) != (b >= 0):
            t = a / (a - b)
            result.append({key: previous[key] + t * (current[key] - previous[key]) for key in current})
        if b >= 0:
            result.append(current)
    return result


copy_names = [name for name in attributes if name not in ('JOINTS_0', 'WEIGHTS_0')]
extra = {name: [] for name in attributes}
indices = []


def smooth(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


def fit_point(source):
    point = source.copy()
    point[1] = .85 + (source[1] - .85) * 1.05 + .015
    heights = [.85, .91, .96, 1.0, 1.04, 1.08, 1.13]
    depths = [-.09, -.10, -.115, -.12, -.107, -.09, -.075]
    i = int(np.clip(np.searchsorted(heights, point[1]) - 1, 0, len(heights) - 2))
    ratio = smooth((point[1] - heights[i]) / (heights[i + 1] - heights[i]))
    point[2] = depths[i] + ratio * (depths[i + 1] - depths[i])
    point[2] -= .020 * smooth(abs(source[0]) / .065)
    point[2] += .20 * (.13 - source[2]) - .015
    point[2] -= .015 * (1 - smooth((point[1] - .96) / .06))
    return point


def fit_normal_and_tangent(source, normal, tangent, accessory):
    if accessory:
        jacobian = np.diag([1, 1.05, -1])
    else:
        epsilon = 1e-5
        jacobian = np.column_stack([(fit_point(source + np.eye(3)[i] * epsilon) -
                                     fit_point(source - np.eye(3)[i] * epsilon)) / (2 * epsilon)
                                    for i in range(3)])
    normal = np.linalg.solve(jacobian.T, normal)
    normal /= max(np.linalg.norm(normal), 1e-12)
    tangent = tangent.copy()
    tangent[:3] = jacobian @ tangent[:3]
    tangent[:3] -= normal * np.dot(normal, tangent[:3])
    tangent[:3] /= max(np.linalg.norm(tangent[:3]), 1e-12)
    tangent[3] *= -1
    return normal, tangent


def emit(polygon, label):
    start = len(extra['POSITION'])
    for item in polygon:
        vertex = {key: value.copy() for key, value in item.items()}
        source = vertex['POSITION'].copy()
        accessory = label != central_label
        if accessory:
            anchor = positions[labels == label].mean(axis=0)
            point = fit_point(anchor) + (source - anchor) * [1, 1.05, -1]
        else:
            point = fit_point(source)
        normal, tangent = fit_normal_and_tangent(source, vertex['NORMAL'], vertex['TANGENT'], accessory)
        vertex['POSITION'], vertex['NORMAL'], vertex['TANGENT'] = point, normal, tangent
        for name in copy_names:
            extra[name].append(vertex[name])
        extra['JOINTS_0'].append([0, 0, 0, 0])
        extra['WEIGHTS_0'].append([1, 0, 0, 0])
    for i in range(1, len(polygon) - 1):
        indices.extend([start, start + i + 1, start + i])


bands = np.linspace(.85, 1.08, 9)
for face in faces[selected]:
    label = labels[face[0]]
    original = [{name: attributes[name][i].astype(float) for name in copy_names} for i in face]
    for dimension, sign, limit in [(0, 1, -.065), (0, -1, -.065), (2, 1, .085), (1, -1, -1.08)]:
        original = clip(original, dimension, sign, limit)
        if len(original) < 3:
            break
    if len(original) < 3:
        continue
    for lower, upper in zip(bands[:-1], bands[1:]):
        polygon = clip(clip(original, 1, 1, lower), 1, -1, -upper)
        if len(polygon) >= 3:
            emit(polygon, label)

assert len(indices) > 0
arrays = {name: np.array(values) for name, values in extra.items()}
keys = np.round(np.column_stack([arrays[name] for name in attributes]), 6)
_, keep, remap_vertices = np.unique(keys, axis=0, return_index=True, return_inverse=True)
indices = remap_vertices[np.array(indices, dtype=np.uint32)]
extra = {name: values[keep] for name, values in arrays.items()}
original_count = len(positions)
for name, old_index in list(primitive['attributes'].items()):
    old = doc['accessors'][old_index]
    added = np.array(extra[name])
    if old['componentType'] != 5126:
        added = np.rint(added)
    combined = np.concatenate([attributes[name], added.astype(attributes[name].dtype)])
    new_index = add_accessor(doc, raw, combined, old['type'], old['componentType'])
    if old.get('normalized'):
        doc['accessors'][new_index]['normalized'] = True
    primitive['attributes'][name] = new_index
all_indices = np.concatenate([faces.reshape(-1), indices.astype(np.uint32) + original_count])
component = 5123 if all_indices.max() <= 65535 else 5125
if component == 5125:
    raise ValueError('Rear panel requires unsupported 32-bit accessor')
primitive['indices'] = add_accessor(doc, raw, all_indices.reshape(-1, 1), 'SCALAR', component)
doc['buffers'] = [{'byteLength': len(raw)}]
output = OUTPUT / 'bottom_female_valkyrie.glb'
write_glb(output, doc, raw)
mask = json.loads((SOURCE.parent / 'body-occlusion.json').read_text())
mask['bottom_sha256'] = hashlib.sha256(output.read_bytes()).hexdigest()
mask['scope'] = 'Preserve body and underwear; rear panel is geometry.'
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
report = {'front_fitting_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'bottom_sha256': mask['bottom_sha256'], 'source_triangles': len(faces),
          'rear_panel_triangles': len(indices) // 3, 'rear_panel_vertices': len(extra['POSITION']),
          'total_triangles': len(all_indices) // 3, 'status': 'Rear panel fitting candidate; visual and motion review required.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
