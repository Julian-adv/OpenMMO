"""Cut the maid bodice, generated alone on a blue bust mannequin, off the mannequin.

The first bodice came with the whole outfit and its collar sat at the trapezius, narrower than the neck there.
This one was drawn alone from the hips up: head, neck, arms and belly are bare mannequin and hold the alignment.

.venv/bin/python tools/split-female-maid-top.py
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
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
# Another outfit's piece generated the same way is split by naming its folder.
DIRECTORY = ROOT / (sys.argv[1] if len(sys.argv) > 1 else 'assets/items/female_maid_outfit/tripo_top_v2')
SOURCE = DIRECTORY / 'model_url.glb'
# Red minus blue under which a texel is mannequin, and the triangles a blue piece needs to be more than shading.
MANNEQUIN_BLUE = -15
MANNEQUIN_PIECE = 200
# The bust was rendered from this height (m, flat feet) up; the figure's crown stands at the body's.
BUST_FLOOR = 1.05
# Small loose pieces wholly above this height (m, lifted floor) are the mannequin's mouth, eyes and lashes; the
# collar frill reaches higher but is larger than LASHES triangles.
HEAD = 1.6
LASHES = 400
# Loose pieces smaller than this are scraps left where the mannequin was cut away.
SCRAP = 12

source = Source(SOURCE)
faces = source.faces
blue = source.face_share(source.warmth < MANNEQUIN_BLUE, faces) > .5
around = source.neighbours()
# A mannequin triangle with cloth on two sides is a notch in the cloth and joins it. Cloth is never given to the
# mannequin: the collar band stands against the neck, and eating its rim back opened holes under the frill.
for _ in range(3):
    blue &= np.array([sum(not blue[other] for other in near) for near in around]) < 2
labels = source.components(faces[blue])
blue[np.flatnonzero(blue)[np.bincount(labels)[labels] < MANNEQUIN_PIECE]] = False

body_doc, body_binary = read_glb(BODY)
skin = body_doc['meshes'][0]['primitives'][0]['attributes']
rest = accessor(body_doc, body_binary, skin['POSITION']).astype(float)
joints = accessor(body_doc, body_binary, skin['JOINTS_0']).astype(int)
weights = accessor(body_doc, body_binary, skin['WEIGHTS_0']).astype(float)
deformation = heel_pose(body_doc, body_binary, degrees=30)[0]
target = np.einsum('vj,vjab,vb->va', weights, deformation[joints], np.c_[rest, np.ones(len(rest))])[:, :3]
lift = -target[:, 1].min()
target[:, 1] += lift
# Only the part of the body the bust shows: the arms run out of the frame, so hands and legs are left out.
target = target[(target[:, 1] > BUST_FLOOR + lift - .05) & (np.abs(target[:, 0]) < .45)]
mannequin = np.unique(faces[blue])
top = target[:, 1].max()
scale = (top - BUST_FLOOR - lift) / np.ptp(source.points[:, 1])
offset = np.array([0, top - scale * source.points[:, 1].max(), 0])
fit = source.align(mannequin, target, scale, offset)
fit['heel_lift_m'] = lift

keep = faces[~blue]
labels = source.components(keep)
parts, components = {}, []
for label in sorted(np.unique(labels), key=lambda label: -(labels == label).sum()):
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) >= SCRAP and (low[1] < HEAD or len(triangles) > LASHES):
        parts.setdefault('top', []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-maid-top.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'components': components[:24], 'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({key: report[key] for key in ('alignment', 'mannequin_triangles', 'components', 'parts')}, indent=1))
