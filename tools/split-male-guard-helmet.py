"""Cut the guard captain's kettle hat and mail hood off the blue bust they were generated on.

.venv/bin/python tools/split-male-guard-helmet.py [--female]
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
DIRECTORY = ROOT / f"assets/items/{'female' if FEMALE else 'male'}_guard_outfit/tripo_helmet_v1"
SOURCE = DIRECTORY / 'model_url.glb'
# The concept framed the bust from this height (m) up; the model is one unit tall.
FRAME_FLOOR = 1.05 if FEMALE else 1.1
# Height (m) of the base of the neck.
NECK = 1.47 if FEMALE else 1.5

source = Source(SOURCE)
blue = source.face_share(source.warmth < -15, source.faces) > .5
around = source.neighbours()
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2

body, body_binary = read_glb(BODY)


def surface(regions):
    return np.vstack([accessor(body, body_binary, primitive['attributes']['POSITION'])
                      for node in body['nodes'] if node.get('extras', {}).get('region') in regions
                      for primitive in body['meshes'][node['mesh']]['primitives']]).astype(float)


def female_surface(above):
    """The one-piece female body in its heel pose, standing on the floor as the concept showed it."""
    skin = body['meshes'][0]['primitives'][0]['attributes']
    rest = accessor(body, body_binary, skin['POSITION']).astype(float)
    joints = accessor(body, body_binary, skin['JOINTS_0']).astype(int)
    weights = accessor(body, body_binary, skin['WEIGHTS_0']).astype(float)
    deformation = heel_pose(body, body_binary, degrees=30)[0]
    posed = np.einsum('vj,vjab,vb->va', weights, deformation[joints], np.c_[rest, np.ones(len(rest))])[:, :3]
    posed[:, 1] -= posed[:, 1].min()
    return posed[posed[:, 1] > above]


whole = female_surface(FRAME_FLOOR) if FEMALE else surface(('head', 'neck', 'torso', 'upper_arms'))
head = female_surface(NECK) if FEMALE else surface(('head', 'neck'))
bust = np.unique(source.faces[blue])
# Start with the hat's finial a hand above the crown of the head.
scale = (head[:, 1].max() + .1 - FRAME_FLOOR) / np.ptp(source.points[:, 1])
offset = np.array([0, FRAME_FLOOR - scale * source.points[:, 1].min(), 0])
# The whole bust sets the size; a copy takes that fit so the face and neck can then place the helmet.
sized = Source(SOURCE).align(bust, whole, scale, offset, upright=True)
scale, offset = sized['scale'], np.array(sized['translation'])
# The generated shoulders and chest are not the body's.
placed = scale * source.points[bust] @ np.array(sized['rotation']).T + offset
face = bust[(placed[:, 1] > NECK) & (np.abs(placed[:, 0]) < .09)]
fit = source.align(face, head, scale, offset, upright=True, rescale=False)

keep = source.faces[~blue]
labels = source.components(keep)
parts = {'helmet': np.vstack([keep[labels == label] for label in np.unique(labels) if (labels == label).sum() >= 20])}
output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-male-guard-helmet.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(source.faces), 'mannequin_triangles': int(blue.sum()),
          'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
