"""Cut the shopkeeper outfit off the blue mannequin it was generated on and name its parts.

.venv/bin/python tools/split-female-shopkeeper-outfit.py
"""
import hashlib
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.female_boot_pose import heel_pose
from outfits.mannequin import Source

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
# Generated from front and back views: from the front alone the skirt's back grew a second apron panel.
DIRECTORY = ROOT / 'assets/items/female_shopkeeper_outfit/tripo_multiview_v2'
SOURCE = DIRECTORY / 'model_url.glb'
# Red minus blue under which a texel is mannequin, and the triangles a blue piece needs to be more than shading.
MANNEQUIN_BLUE = -15
MANNEQUIN_PIECE = 200
# Pieces reaching above this height (m, lifted floor) are the blouse, the corset and its lacing; the belt and all
# that hangs from it stay below it and go with the skirt.
WAIST = 1.2
# The skirt and apron end in a torn rim under the corset, which shows once the top is off: both are cut level here,
# behind the belt.
RIM = 1.16
# Cloth hanging lower than this is the skirt or the apron.
CLOTH = .6
# The boot shafts run up inside the skirt; they end below the knee.
BOOT_TOP = .43
# Blouse triangles farther than this (m) from the body's centre plane belong to a sleeve.
SLEEVE = .25
# Depth (m) of the sleeve end searched for the disc that closes it, and the share of the cuff wall's radius inside
# which a triangle is part of the disc.
CUFF_END = .07
INSIDE = .8
# Red minus blue above which a sleeve texel is linen, and how much darker the disc is painted, as the sleeve's inside.
LINEN = 20
COOL = 5
SHADING = (.85, 1.1)
SHADE = .85
# Loose pieces above this are the mannequin's mouth.
HEAD = 1.6

source = Source(SOURCE)
faces = source.faces
blue = source.face_share(source.warmth < MANNEQUIN_BLUE, faces) > .5
around = source.neighbours()
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2
# Cool shading between the apron and the skirt also reads as blue; only the big pieces are mannequin.
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
# The head and both forearms show; together they fix the lean a head alone leaves loose.
mannequin = np.unique(faces[blue])
scale = target[:, 1].max() / np.ptp(source.points[:, 1])
offset = np.array([0, target[:, 1].max() - scale * source.points[mannequin][:, 1].max(), 0])
fit = source.align(mannequin, target, scale, offset)
fit['heel_lift_m'] = lift

keep = faces[~blue]
labels = source.components(keep)
parts, components = {}, []
for label in sorted(np.unique(labels), key=lambda label: -(labels == label).sum()):
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) < 20 or low[1] > HEAD:
        continue
    name = 'legs' if low[1] < .1 else 'top' if high[1] > WAIST else 'skirt'
    if name == 'skirt' and low[1] < CLOTH:
        triangles = source.clip(triangles, RIM)
        triangles = triangles[source.points[triangles].mean(1)[:, 1] < RIM]
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}

legs = source.clip(parts.pop('legs'), BOOT_TOP + lift)
legs = legs[source.points[legs].mean(1)[:, 1] < BOOT_TOP + lift]
left = source.points[legs].mean(1)[:, 0] > 0
parts['left_boots'], parts['right_boots'] = legs[left], legs[~left]

# A pale blue-grey disc closes each sleeve end where the mannequin's arm came out. The arm passes through it and
# only its rim shows, so it stays and takes the blouse's colour; the cuff does not survive cutting it out.
top = parts['top']
corners = source.points[top]
facing = np.cross(corners[:, 1] - corners[:, 0], corners[:, 2] - corners[:, 0])
facing /= np.linalg.norm(facing, axis=1)[:, None] + 1e-12
centres = corners.mean(1)
cap = np.zeros(len(top), bool)
for sign in (1, -1):
    side = centres[:, 0] * sign > SLEEVE
    sleeve = source.points[np.unique(top[side])]
    along = np.linalg.svd(sleeve - sleeve.mean(0))[2][0]
    along *= np.sign(along[0]) * sign
    end = side & (centres @ along > (sleeve @ along).max() - CUFF_END)
    # The disc is a shallow cone: its triangles face along the sleeve or lie inside the cuff's wall.
    across = centres - centres[end].mean(0)
    radius = np.linalg.norm(across - np.outer(across @ along, along), axis=1)
    wall = np.median(radius[end & (np.abs(facing @ along) < .3)])
    cap |= end & ((np.abs(facing @ along) > .5) | (radius < wall * INSIDE))


def footprint(triangles):
    size = source.colour.shape[0]
    image = Image.new('1', (size, size))
    draw = ImageDraw.Draw(image)
    for corner in (source.uv[triangles] % 1.0) * size:
        draw.polygon([tuple(c) for c in corner], fill=1, outline=1)
    return np.array(image)


sleeves = np.abs(centres[:, 0]) > SLEEVE
blouse = footprint(top[~cap & sleeves]) & (source.warmth > LINEN)
disc = ndimage.binary_dilation(footprint(top[cap]), iterations=3) & ~blouse
source.colour = source.colour.copy()
light = source.colour.mean(2)
# Flat colour reads as patches on the cuff's rim: the disc keeps its own shading within SHADING.
shading = np.clip(light[disc] / np.median(light[disc]), *SHADING)[:, None]
source.colour[disc] = np.clip(np.median(source.colour[blouse], axis=0) * SHADE * shading, 0, 255).astype(np.uint8)
# The sleeves are warm all over, so a cooler texel left along the rim is mannequin.
specks = ndimage.binary_dilation(footprint(top[sleeves]) & (source.warmth < COOL), iterations=2)
source.colour[specks] = source.colour[disc][0]
source.warmth = source.colour[:, :, 0].astype(int) - source.colour[:, :, 2]

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-shopkeeper-outfit.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'waist_m': WAIST, 'sleeve_cap_triangles': int(cap.sum()), 'sleeve_cap_texels': int(disc.sum()), 'rim_m': RIM, 'boot_top_m': BOOT_TOP + lift, 'sole_height_m': float(source.points[:, 1].min()),
          'components': components[:24], 'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
