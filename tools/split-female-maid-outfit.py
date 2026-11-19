"""Cut the maid outfit off the blue mannequin it was generated on and name its parts.

.venv/bin/python tools/split-female-maid-outfit.py
"""
import hashlib
import json
from pathlib import Path

import numpy as np

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.female_boot_pose import heel_pose
from outfits.mannequin import FACING, Source

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
DIRECTORY = ROOT / 'assets/items/female_maid_outfit/tripo_multiview_v1'
SOURCE = DIRECTORY / 'model_url.glb'
# Red minus blue under which a texel is mannequin. Only the face, a band of each upper arm and of each thigh show,
# in pieces of a few dozen triangles.
MANNEQUIN_BLUE = -15
MANNEQUIN_PIECE = 30
# Pieces lying wholly beyond this distance (m) from the centre plane are gloves, with their frills and bows.
ARM = .22
# Pieces wholly above this height (m) are the headband, or the mannequin's mouth when small.
HEAD = 1.66
HEADBAND = 100
# The apron's white waistband is painted on the dress shell up to some height within this reach (m) above the
# apron panel's top edge; the dress is cut this far above the band, which goes with the skirt.
BAND_REACH = .1
BAND_MARGIN = .002
# The top keeps this much (m) of the band as well and tucks under the skirt's: where the two only met, the skin
# cuts at their edges left a line of belly between them.
TUCK = .035
# The bodice in the game was generated apart (tools/split-female-maid-top.py) and ends where the band begins: the
# skirt keeps this much (m) of the corset above its band, under that bodice, so no belly shows between them.
RISE = .03
# Mean of red, green and blue above which a texel is the band's white.
WHITE = 120
# Loose pieces whose centre lies no higher than this above the cut (m) and behind the body are the apron's bow.
BOW = .1

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
mannequin = np.unique(faces[blue])
# Size comes from fingertip to fingertip, which the thin gloves barely change. Floor to headband made the figure 1%
# small, since the frill stands above the crown, and fitting the size to the blue parts shrank it to 0.57.
# The figure also leans back 2.4 degrees against the body: turned about the vertical only, the collar sat 3 cm
# behind the neck and the headband 5 cm behind the hairline.
scale = np.ptp(target[:, 0]) / np.ptp((source.points @ FACING.T)[:, 0])
offset = np.array([0, -scale * source.points[:, 1].min(), 0])
fit = source.align(mannequin, target, scale, offset, rescale=False)
fit['heel_lift_m'] = lift

keep = faces[~blue]
labels = source.components(keep)
pieces = []
for label in sorted(np.unique(labels), key=lambda label: -(labels == label).sum()):
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    pieces.append((triangles, low, high, (low + high) / 2))
# The apron is the largest piece hanging wholly between knee and chest.
apron = next(piece for piece in pieces if piece[1][1] > .6 and piece[2][1] < 1.3)
dress = max(pieces, key=lambda piece: len(piece[0]))[0]
heights = source.points[dress].mean(1)[:, 1]
white = source.face_share(source.colour.mean(2) > WHITE, dress) > .5
steps = np.arange(apron[2][1] - .02, apron[2][1] + BAND_REACH, .005)
bands = [(step, white[(heights >= step) & (heights < step + .005)]) for step in steps]
cut = max(step + .005 for step, band in bands if band.size and band.mean() > .5) + BAND_MARGIN

parts, components = {}, []
for triangles, low, high, centre in pieces:
    components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    side = 'left' if centre[0] > 0 else 'right'
    if low[1] > HEAD:
        if len(triangles) < HEADBAND:
            continue
        name = 'helmet'
    elif min(abs(low[0]), abs(high[0])) > ARM and low[0] * high[0] > 0:
        name = f'{side}_glove'
    elif low[1] < .1:
        name = f'{side}_boots'
    elif low[1] < cut < high[1] and len(triangles) > 2000:
        # The dress is one shell from collar to hem.
        triangles = source.clip(source.clip(triangles, cut + RISE), cut - TUCK)
        heights = source.points[triangles].mean(1)[:, 1]
        parts.setdefault('skirt', []).append(triangles[heights < cut + RISE])
        triangles = triangles[heights > cut - TUCK]
        name = 'top'
    elif centre[1] < cut or (centre[2] < 0 and centre[1] < cut + BOW):
        name = 'skirt'
    else:
        name = 'top'
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-maid-outfit.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'waist_cut_m': float(cut), 'sole_height_m': float(source.points[:, 1].min()),
          'components': components[:32], 'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({key: report[key] for key in ('alignment', 'mannequin_triangles', 'waist_cut_m', 'sole_height_m', 'parts')},
                 indent=1))
