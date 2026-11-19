"""Cut the women's guard skirt off the blue mannequin it was generated on alone.

.venv/bin/python tools/split-female-guard-skirt.py
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
DIRECTORY = ROOT / (sys.argv[1] if len(sys.argv) > 1 else 'assets/items/female_guard_outfit/tripo_skirt_v1')
SOURCE = DIRECTORY / 'model_url.glb'
# Red minus blue under which a texel is mannequin, and the triangles a blue piece needs to be more than shading.
MANNEQUIN_BLUE = -15
MANNEQUIN_PIECE = 200
# Loose pieces above this are the mannequin's mouth; a second argument lowers it to drop the swimwear's back tie too.
HEAD = float(sys.argv[2]) if len(sys.argv) > 2 else 1.6

source = Source(SOURCE)
faces = source.faces
blue = source.face_share(source.warmth < MANNEQUIN_BLUE, faces) > .5
around = source.neighbours()
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2
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
# The whole body shows but the hips, so it sets the size as well as the place.
mannequin = np.unique(faces[blue])
scale = target[:, 1].max() / np.ptp(source.points[:, 1])
offset = np.array([0, target[:, 1].max() - scale * source.points[mannequin][:, 1].max(), 0])
fit = source.align(mannequin, target, scale, offset, upright=True)
fit['heel_lift_m'] = lift

keep = faces[~blue]
labels = source.components(keep)
components = [{'triangles': int((labels == label).sum())} for label in np.unique(labels)]
pieces = [keep[labels == label] for label in np.unique(labels)
          if (labels == label).sum() >= 20 and source.extent(keep[labels == label])[0][1] < HEAD]
# A third argument `boots` names the pieces by leg instead: stockings and shoes generated the same way.
if sys.argv[3:4] == ['boots']:
    sides = [sum(source.extent(piece)).__getitem__(0) > 0 for piece in pieces]
    parts = {name: np.vstack([piece for piece, left in zip(pieces, sides) if left == (name == 'left_boots')])
             for name in ('left_boots', 'right_boots')}
    # A fourth argument cuts the stocking tops level at that height (m): a lace band lost to the mannequin's
    # colour left them ragged.
    if len(sys.argv) > 4:
        top = float(sys.argv[4])
        for name, triangles in parts.items():
            triangles = source.clip(triangles, top)
            parts[name] = triangles[source.points[triangles].mean(1)[:, 1] < top]
else:
    parts = {'skirt': np.vstack(pieces)}

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-guard-skirt.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'sole_height_m': float(source.points[:, 1].min()),
          'components': components[:24], 'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
