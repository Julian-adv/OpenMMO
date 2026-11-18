"""Cut the guard captain's tabard off the blue mannequin, which wore the fitted hauberk, also blue.

.venv/bin/python tools/split-male-guard-tabard.py [--female]
"""
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.female_boot_pose import heel_pose
from outfits.mannequin import Source

ROOT = Path(__file__).resolve().parents[1]
FEMALE = '--female' in sys.argv
BODY = ROOT / ('assets/modular_human_female_01/rig_v1/female-base.glb' if FEMALE else 'assets/modular_human_male_01/fitted/base.glb')
HAUBERK = ROOT / ('assets/items/female_guard_outfit/top/fitted_v1/top_female_guard.glb' if FEMALE
                  else 'assets/modular_human_male_01/guard/tripo_v1/top_guard.glb')
DIRECTORY = ROOT / f"assets/items/{'female' if FEMALE else 'male'}_guard_outfit/tripo_tabard_v1"
SOURCE = DIRECTORY / 'model_url.glb'

source = Source(SOURCE)
blue = source.face_share(source.warmth < -15, source.faces) > .5
around = source.neighbours()
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2


def points(path, skip=()):
    doc, binary = read_glb(path)
    return np.vstack([accessor(doc, binary, primitive['attributes']['POSITION'])
                      for node in doc['nodes'] if 'mesh' in node and node.get('extras', {}).get('region') not in skip
                      for primitive in doc['meshes'][node['mesh']]['primitives']]).astype(float)


def female_target():
    """The female body in its heel pose standing on the floor, as the concept showed it, with the hauberk lifted to match."""
    body, binary = read_glb(BODY)
    skin = body['meshes'][0]['primitives'][0]['attributes']
    rest = accessor(body, binary, skin['POSITION']).astype(float)
    joints = accessor(body, binary, skin['JOINTS_0']).astype(int)
    weights = accessor(body, binary, skin['WEIGHTS_0']).astype(float)
    deformation = heel_pose(body, binary, degrees=30)[0]
    posed = np.einsum('vj,vjab,vb->va', weights, deformation[joints], np.c_[rest, np.ones(len(rest))])[:, :3]
    lift = -posed[:, 1].min()
    return np.vstack([posed, points(HAUBERK)]) + [0, lift, 0], lift


target, lift = female_target() if FEMALE else (np.vstack([points(BODY, ('boot_ankles',)), points(HAUBERK)]), 0)
labels = source.components(source.faces[blue])
mannequin = np.unique(source.faces[blue][labels == np.bincount(labels).argmax()])
height = target[:, 1].max()
scale = height / np.ptp(source.points[:, 1])
offset = np.array([0, -scale * source.points[:, 1].min(), 0])
fit = source.align(mannequin, target, scale, offset, upright=True)

keep = source.faces[~blue]
labels = source.components(keep)
parts = {'tabard': np.vstack([keep[labels == label] for label in np.unique(labels) if (labels == label).sum() >= 20])}
output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-male-guard-tabard.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'hauberk': str(HAUBERK.relative_to(ROOT)), 'hauberk_sha256': hashlib.sha256(HAUBERK.read_bytes()).hexdigest(),
          'alignment': fit, 'heel_lift_m': lift, 'source_triangles': len(source.faces), 'mannequin_triangles': int(blue.sum()),
          'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
