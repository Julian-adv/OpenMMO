"""Cut the ceremony tuxedo off the blue mannequin it was generated on and name its parts.

Only the head and neck of the mannequin show, so they and the gloves place the model, kept upright.

.venv/bin/python tools/split-male-tuxedo.py
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
DIRECTORY = ROOT / 'assets/items/male_ceremony_tuxedo/tripo_front_v1'
SOURCE = DIRECTORY / 'model_url.glb'
# Height (m) above which a loose piece is part of the mannequin's head.
HEAD = 1.66

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
# The gloves lie on the hands, so they steady the scale and the turn that the head alone leaves loose.
hands = np.unique(source.faces[~blue][np.abs((source.points @ FACING.T)[source.faces[~blue]].mean(1)[:, 0]) * scale > .4])
offset = np.array([0, height - scale * source.points[head][:, 1].max(), 0])
fit = source.align(np.r_[head, hands], target, scale, offset, upright=True)
points = source.points

keep = source.faces[~blue]
white = (source.colour.min(2) > 150) & (source.warmth > -15)
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
    if high[1] < .3:
        name = f'{side}_boots'
    elif low[1] < .4:
        name = 'pants'
    elif abs(centre[0]) > .35 and len(triangles) > 1000 and source.face_share(white, triangles).mean() > .5:
        name = f'{side}_glove'
    elif low[1] > HEAD:
        # The mannequin's mouth, which is not blue.
        continue
    else:
        # Jacket with the shirt front, collar, bow tie, buttons and pin; the shirt cuffs and cuff buttons hang apart.
        name = 'top'
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

tree = cKDTree(target)
checks = {name: float(np.percentile(tree.query(points[np.unique(parts[name])])[0], 50))
          for name in ('left_glove', 'right_glove', 'left_boots', 'right_boots')}
output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-male-tuxedo.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'median_distance_to_body_m': checks, 'sole_height_m': float(points[:, 1].min()),
          'source_triangles': len(source.faces), 'mannequin_triangles': int(blue.sum()), 'components': components[:12],
          'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
