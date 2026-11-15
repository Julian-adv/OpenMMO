"""Cut the ceremony outfit off the blue mannequin it was generated on and name its parts.

The mannequin is a render of the female body on 30° heels, so its generated copy is aligned to that pose
(rotation, uniform scale, translation) and the outfit comes along. Blue triangles are dropped.

.venv/bin/python tools/split-female-ceremony-outfit.py [--version v2]
"""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree

from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.female_boot_pose import heel_pose
from outfits.mannequin import Source

ROOT = Path(__file__).resolve().parents[1]
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
# v1 wears the veil, which hid the bodice back; v2 was generated without it for the bodice and skirt.
parser = argparse.ArgumentParser()
parser.add_argument('--version', choices=['v1', 'v2'], default='v1')
DIRECTORY = ROOT / f'assets/items/female_ceremony_outfit/tripo_multiview_{parser.parse_args().version}'
SOURCE = DIRECTORY / 'model_url.glb'
# Height above the lifted floor where the stockings end, inside the skirt.
STOCKING_TOP = .78
# Metres above the gold belt's top edge where the bodice is cut from the skirt.
BELT_MARGIN = .002
# Open-edge loops at the front neckline shorter than this (m) are slits and get closed.
SLIT_LOOP = .5
# Rings of triangles the front neckline may lose.
NECKLINE_PEEL = 4
# Metres past the gold cuff band where each glove is cut.
CUFF_MARGIN = .001

source = Source(SOURCE)
faces, warmth = source.faces, source.warmth
# The outfit is white and gold; only the mannequin is clearly bluer than it is red.
blue_texel = warmth < -15

share = source.face_share(blue_texel, faces)
blue = share > .5
# Even out the cut: a mannequin triangle with outfit on two sides is a notch and joins the outfit, and an outfit
# triangle with outfit on one side at most is a spike and goes.
around = source.neighbours()
# Beside the mannequin a triangle with any blue in it goes too: part-blue ones stayed on the chest as white teeth
# along the neckline. Elsewhere cool shading on the white cloth reads as blue, and dropping it tore the hem band.
# Only beside real mannequin: specks of cool shading are not seeds.
labels = source.components(faces[blue])
beside = np.zeros(len(faces), bool)
beside[np.flatnonzero(blue)[np.bincount(labels)[labels] >= 200]] = True
for _ in range(3):
    beside |= (share > 0) & np.array([any(beside[other] for other in near) for near in around])
blue |= beside
for _ in range(3):
    blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2

# Align the mannequin to the body standing on heels with its soles on the floor. The source faces +X and the body +Z.
body_doc, body_binary = read_glb(BODY)
skin = body_doc['meshes'][0]['primitives'][0]['attributes']
rest = accessor(body_doc, body_binary, skin['POSITION']).astype(float)
joints = accessor(body_doc, body_binary, skin['JOINTS_0']).astype(int)
weights = accessor(body_doc, body_binary, skin['WEIGHTS_0']).astype(float)
deformation = heel_pose(body_doc, body_binary, degrees=30)[0]
homogeneous = np.c_[rest, np.ones(len(rest))]
target = np.einsum('vj,vjab,vb->va', weights, deformation[joints], homogeneous)[:, :3]
lift = -target[:, 1].min()
target[:, 1] += lift
tree = cKDTree(target)
labels = source.components(faces[blue])
mannequin = np.unique(faces[blue][labels == np.bincount(labels).argmax()])
scale = target[:, 1].max() / (np.ptp(source.points[:, 1]) * .97)
offset = np.array([0, target[:, 1].max() - scale * source.points[mannequin][:, 1].max(), 0])
fit = source.align(mannequin, target, scale, offset)
fit = {**{key: fit[key] for key in ('scale', 'rotation', 'translation')}, 'heel_lift_m': lift,
       **{key: fit[key] for key in ('mannequin_vertices', 'mean_distance_m', 'p95_distance_m')}}

keep = faces[~blue]
labels = source.components(keep)
order = sorted(np.unique(labels), key=lambda label: -(labels == label).sum())
parts = {}
report_components = []
gold_texel = warmth > 40
for label in order:
    triangles = keep[labels == label]
    low, high = source.extent(triangles)
    centre = (low + high) / 2
    report_components.append({'triangles': len(triangles), 'min': low.round(3).tolist(), 'max': high.round(3).tolist()})
    if len(triangles) < 20:
        continue
    if low[1] < .1:
        name = 'legs'
    elif high[1] > 1.6:
        name = 'helmet'
    elif abs(centre[0]) > .3 and np.ptp([low[0], high[0]]) < .45:
        name = 'left_glove' if centre[0] > 0 else 'right_glove'
    elif len(triangles) < 1000 and low[1] < 1.15:
        # Scraps of the hips and thighs left under the skirt once the mannequin between them is gone.
        continue
    else:
        name = 'dress'
    parts.setdefault(name, []).append(triangles)
parts = {name: np.vstack(groups) for name, groups in parts.items()}
legs = parts['legs']
if np.abs(source.points[np.unique(legs)][:, 0]).max() > .45:
    # v2 fused the skirt to the stockings: what stands off the legs above the shoes, or lies above the legs, is skirt.
    # Loose triangles of the same lump up at the chest are not: they showed as shards inside the neckline.
    centres = source.points[legs].mean(1)
    skirt = ((centres[:, 1] > .9 + lift) | ((tree.query(centres)[0] > .03) & (centres[:, 1] > .4))) \
        & (centres[:, 1] < 1.3 + lift)
    parts['dress'] = np.vstack([parts['dress'], legs[skirt]])
    parts['legs'] = legs[~skirt]

dress = parts.pop('dress')
gold = source.face_share(gold_texel, dress) > .5
heights = source.points[dress].mean(1)[:, 1]
band = gold & (heights > 1.05 + lift) & (heights < 1.25 + lift)
belt_top = float(np.percentile(source.points[dress[band]][:, :, 1].max(1), 90)) + BELT_MARGIN
dress = source.clip(dress, belt_top)
above = source.points[dress].mean(1)[:, 1] > belt_top
parts['top'], parts['skirt'] = dress[above], dress[~above]

# Loose scraps: pieces of a few triangles joined to nothing else.
labels = source.components(parts['top'])
parts['top'] = parts['top'][np.bincount(labels)[labels] >= 20]

# Pale mannequin triangles stayed above the front neckline as teeth, with gaps between them where the skin under
# them was hidden: the rim is peeled back while its triangles are cool white, which stops at the gold trim.
cool_texel = warmth < 0
for _ in range(NECKLINE_PEEL):
    top = parts['top']
    used = {}
    welded = source.weld()[top]
    for index, triangle in enumerate(welded):
        for i in range(3):
            used.setdefault(tuple(sorted((triangle[i], triangle[(i + 1) % 3]))), []).append(index)
    open_edges = [edge for edge, group in used.items() if len(group) == 1]
    rim = np.zeros(len(top), bool)
    rim[[used[edge][0] for edge in open_edges]] = True
    # A strip one triangle wide also hung off the trim across a slit: every corner of it lies on an open edge.
    sliver = np.isin(welded, np.unique(open_edges)).all(1)
    centres = source.points[top].mean(1)
    # Short of the sleeve joins, whose thin bridges to the bodice would go too.
    front = (centres[:, 1] > 1.3 + lift) & (np.abs(centres[:, 0]) < .125) & (centres[:, 2] > 0)
    teeth = rim & front & (sliver | (source.face_share(cool_texel, top) >= .8))
    if not teeth.any():
        break
    parts['top'] = top[~teeth]

# Dropping the mannequin also tore slits in the bodice beside the trim, and the skin behind them showed or left a
# hole. Each short loop of open edges at the front neckline is closed with a fan from its centre.
top = parts['top']
welded = source.weld()
used = {}
for triangle in top:
    for i in range(3):
        a, b = triangle[i], triangle[(i + 1) % 3]
        used.setdefault(tuple(sorted((welded[a], welded[b]))), []).append((a, b))
open_edges = [group[0] for group in used.values() if len(group) == 1]
labels = connected_components(coo_matrix((np.ones(len(open_edges)), ([welded[a] for a, _ in open_edges],
                                                                   [welded[b] for _, b in open_edges])),
                                         shape=(welded.max() + 1,) * 2), directed=False)[1]
patches = []
for label in np.unique([labels[welded[a]] for a, _ in open_edges]):
    loop = [(a, b) for a, b in open_edges if labels[welded[a]] == label]
    corners = source.points[[a for a, _ in loop]]
    centre = corners.mean(0)
    length = sum(np.linalg.norm(source.points[a] - source.points[b]) for a, b in loop)
    if length < SLIT_LOOP and centre[1] > 1.3 + lift and centre[2] > 0:
        nearest = loop[int(np.linalg.norm(corners - centre, axis=1).argmin())][0]
        source.points = np.vstack([source.points, centre])
        source.normals = np.vstack([source.normals, source.normals[[a for a, _ in loop]].mean(0) / np.linalg.norm(source.normals[[a for a, _ in loop]].mean(0))])
        source.uv = np.vstack([source.uv, source.uv[nearest]])
        # Open edges run against their triangle's winding, so the fan keeps the cloth's facing.
        patches += [[b, a, len(source.points) - 1] for a, b in loop]
if patches:
    parts['top'] = np.vstack([top, patches])

# Pale mannequin triangles stayed on the arm above each glove: the glove ends at the plane of its gold band's far edge.
for name in ('left_glove', 'right_glove'):
    glove = parts[name]
    band = np.unique(glove[source.face_share(gold_texel, glove) > .5])
    centre = source.points[band].mean(0)
    across = np.linalg.svd(source.points[band] - centre)[2][2]
    across *= np.sign((centre - source.points[np.unique(glove)].mean(0)) @ across)
    edge = float(np.percentile(source.points[band] @ across, 98)) + CUFF_MARGIN
    glove = source.clip(glove, edge, across)
    parts[name] = glove[source.points[glove].mean(1) @ across < edge]

legs = source.clip(parts.pop('legs'), STOCKING_TOP + lift)
legs = legs[source.points[legs].mean(1)[:, 1] < STOCKING_TOP + lift]
left = source.points[legs].mean(1)[:, 0] > 0
parts['left_boots'], parts['right_boots'] = legs[left], legs[~left]

output = DIRECTORY / 'segmented.glb'
source.write(output, parts)
report = {'tool': 'tools/split-female-ceremony-outfit.py', 'source': str(SOURCE.relative_to(ROOT)),
          'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(), 'body': str(BODY.relative_to(ROOT)),
          'alignment': fit, 'source_triangles': len(faces), 'mannequin_triangles': int(blue.sum()),
          'belt_top_m': belt_top, 'stocking_top_m': STOCKING_TOP + lift, 'components': report_components[:12],
          'parts': source.part_report(parts),
          'output': str(output.relative_to(ROOT)), 'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest()}
(DIRECTORY / 'split.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=1))
