"""Cut the women's guard outfit off the blue mannequin it was generated on and name its parts.

.venv/bin/python tools/split-female-guard.py
"""
import hashlib
import json
from pathlib import Path

import numpy as np

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.female_boot_pose import heel_pose
from outfits.mannequin import Source

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
DIRECTORY = ROOT / 'assets/items/female_guard_outfit/tripo_body_v1'
SOURCE = DIRECTORY / 'model_url.glb'
# Red minus blue under which a texel is mannequin, and the triangles a blue piece needs to be more than shading.
MANNEQUIN_BLUE = -15
MANNEQUIN_PIECE = 200
# Pieces reaching above this height (m, lifted floor) are the hauberk with its collar and belt.
WAIST = 1.2
# Height (m) of the mail's hem, where the leather straps of the skirt begin.
HEM = .81
# Pieces lying wholly beyond this distance (m) from the centre plane are gloves.
ARM = .3
# Pieces ending below this height (m) off the centre are knee cops and their straps.
KNEE = .75
# Loose pieces above this are the mannequin's mouth.
HEAD = 1.6

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
# The head, the neck and a band of each thigh show.
mannequin = np.unique(faces[blue])
scale = target[:, 1].max() / np.ptp(source.points[:, 1])
offset = np.array([0, target[:, 1].max() - scale * source.points[mannequin][:, 1].max(), 0])
# The figure stands floor to crown as the body does, so that sets the size. Fitting the size to the blue parts
# shrank it 8%: the generated head is larger than the body's and the thigh bands slide along the thighs.
fit = source.align(mannequin, target, scale, offset, upright=True, rescale=False)
fit['heel_lift_m'] = lift

keep = faces[~blue]
labels = source.components(keep)
parts, components = {}, []
for label in sorted(np.unique(labels), key=lambda label: -(labels == label).sum()):
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    centre = (low + high) / 2
    components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) < 20 or low[1] > HEAD:
        continue
    side = 'left' if centre[0] > 0 else 'right'
    if min(abs(low[0]), abs(high[0])) > ARM and low[0] * high[0] > 0:
        name = f'{side}_glove'
    elif low[1] < .1 or (high[1] < KNEE and abs(centre[0]) > .03):
        # The knee cops and their straps go with the boots they are strapped over.
        name = f'{side}_boots'
    elif high[1] > WAIST:
        if low[1] < HEM:
            # The leather straps hang from the mail's hem as one shell with it.
            triangles = source.clip(triangles, HEM)
            below = source.points[triangles].mean(1)[:, 1] < HEM
            parts.setdefault('skirt', []).append(triangles[below])
            triangles = triangles[~below]
        name = 'top'
    else:
        # A mottled inner shell around the hips, under the mail.
        continue
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-guard.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'sole_height_m': float(source.points[:, 1].min()),
          'components': components[:24], 'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
