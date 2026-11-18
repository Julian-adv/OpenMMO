"""Cut the guard captain's outfit off the blue mannequin it was generated on and name its parts.

The bare head and neck place the model, kept upright; the gloves sit 3 cm off the body's hands and would pull it askew.

.venv/bin/python tools/split-male-guard.py
"""
import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.spatial import cKDTree

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.mannequin import FACING, Source

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_male_01/fitted/base.glb'
DIRECTORY = ROOT / 'assets/items/male_guard_outfit/tripo_body_v1'
SOURCE = DIRECTORY / 'model_url.glb'
# Height (m) of the top of the linen wraps, where the boots end and the trousers begin.
WRAP_TOP = .505

source = Source(SOURCE)
blue = source.face_share(source.warmth < -15, source.faces) > .5
around = source.neighbours()
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2

body, body_binary = read_glb(BODY)
target = np.vstack([accessor(body, body_binary, primitive['attributes']['POSITION'])
                    for node in body['nodes'] if node.get('extras', {}).get('region') not in (None, 'boot_ankles')
                    for primitive in body['meshes'][node['mesh']]['primitives']]).astype(float)
labels = source.components(source.faces[blue])
head = np.unique(source.faces[blue][labels == np.bincount(labels).argmax()])
height = target[:, 1].max()
scale = height / np.ptp(source.points[:, 1])
offset = np.array([0, height - scale * source.points[head][:, 1].max(), 0])
fit = source.align(head, target, scale, offset, upright=True)
points = source.points

keep = source.faces[~blue]
labels = source.components(keep)
parts, components = {}, []
for label in sorted(np.unique(labels), key=lambda label: -(labels == label).sum()):
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    centre = (low + high) / 2
    components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) < 20:
        continue
    side = 'left' if centre[0] > 0 else 'right'
    if low[1] < .2 and high[1] > WRAP_TOP:
        # Boots, wraps and trousers came as one shell.
        triangles = source.clip(triangles, WRAP_TOP)
        middle = source.points[triangles].mean(1)
        for name, chosen in [('pants', middle[:, 1] > WRAP_TOP), ('left_boots', (middle[:, 1] < WRAP_TOP) & (middle[:, 0] > 0)),
                             ('right_boots', (middle[:, 1] < WRAP_TOP) & (middle[:, 0] < 0))]:
            parts.setdefault(name, []).append(triangles[chosen])
        continue
    if high[1] < WRAP_TOP:
        name = f'{side}_boots'
    elif high[1] < 1.0:
        # Trousers and the knee cops with their straps.
        name = 'pants'
    elif abs(centre[0]) > .35 and high[1] < 1.2:
        name = f'{side}_glove'
    elif low[1] > 1.55:
        # Loose bits of the mannequin's face.
        continue
    else:
        # Hauberk with its mantle and belt.
        name = 'top'
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

tree = cKDTree(target)
points = source.points
checks = {name: float(np.percentile(tree.query(points[np.unique(parts[name])])[0], 50))
          for name in ('left_glove', 'right_glove', 'left_boots', 'right_boots') if name in parts}
output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-male-guard.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'median_distance_to_body_m': checks, 'sole_height_m': float(points[:, 1].min()),
          'source_triangles': len(source.faces), 'mannequin_triangles': int(blue.sum()), 'components': components[:16],
          'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
