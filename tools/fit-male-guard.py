"""Place and skin the split guard captain's outfit on the modular male.

Parts move only as wholes; the sleeves also turn at the shoulder and elbow to the body's arm pose.

.venv/bin/python tools/fit-male-guard.py
"""
import importlib.util
import json
from io import BytesIO
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.spatial import cKDTree

from lib.glb import read_glb

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/items/male_guard_outfit/tripo_body_v1/segmented.glb'
HELMET = ROOT / 'assets/items/male_guard_outfit/tripo_helmet_v1/segmented.glb'
TABARD = ROOT / 'assets/items/male_guard_outfit/tripo_tabard_v1/segmented.glb'
# Heights (m) between which the tabard's belt lies, and its texels' brightness below which they are leather.
BELT = (1.135, 1.235)
LEATHER = 140
# The chest picture: top edge under the neckline, width, and a cape print's shape.
PRINT_TOP, PRINT_WIDTH, PRINT_ASPECT = 1.49, .22, 320 / 512
# Girth of the tabard against the generated one.
TABARD_GIRTH = 1.16
# Second UV set u for cloth that only takes dye, and for the belt, which takes nothing.
DYED, PLAIN = -10, -20
OUTPUT = ROOT / 'assets/modular_human_male_01/guard/tripo_v1'
REPORT = ROOT / 'doc/assets/modular-male-guard-fitting-v1.json'
spec = importlib.util.spec_from_file_location('tuxedo', ROOT / 'tools/fit-male-tuxedo.py')
tuxedo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tuxedo)
fit, io, pants = tuxedo.fit, tuxedo.io, tuxedo.pants
JOINT = io.BONES


def settle(points, regions, sign=0):
    """Slide a part level onto the body."""
    chosen = points[:, 0] * sign >= 0
    _, shift, report = tuxedo.icp(points[chosen], fit.body_surface(regions, sign), False)
    points[chosen] += shift * [1, 0, 1]
    return report


def top(points, graph):
    # The hauberk stays where the mannequin's neck put it. Laying it on the torso slid it 8 cm back off the chest:
    # the body leans back above the waist and the loose mail gives no hold.
    points, sleeves = tuxedo.align_sleeves(points, graph)
    bones = ('Hips', 'Spine', 'Spine1', 'Spine2', 'Neck')
    # The mail skirt hangs to mid-thigh and follows the thighs.
    limb = ('Shoulder', 'Arm', 'ForeArm', 'UpLeg')
    return points, tuxedo.weights_from(points, graph, ('torso', 'upper_arms', 'forearms', 'neck', 'legs'),
                                       lambda name: name in bones or name.endswith(limb)), dict(sleeves=sleeves)


def stand(points, scale):
    """The generated legs are short of the floor: grow the lower body about the hips."""
    return (points - JOINT['Hips']) * scale + JOINT['Hips']


def trousers(points, graph, scale):
    points = stand(points, scale)
    placed = settle(points, ('legs',))
    return points, pants.skin_weights(points, graph, np.ones(len(points), bool)), dict(scale=round(float(scale), 4), **placed)


def boots(points, graph, scale):
    points = stand(points, scale)
    placed = {key: settle(points, ('feet', 'ankles', 'legs'), sign) for _, key, sign in tuxedo.SIDES}
    return points, tuxedo.weights_from(points, graph, ('feet', 'ankles', 'legs'),
                                       lambda name: name.endswith(('Leg', 'Foot', 'ToeBase'))), \
        placed


def texels(source, raw, uv):
    image = source['images'][source['textures'][source['materials'][0]['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]
    view = source['bufferViews'][image['bufferView']]
    start = view.get('byteOffset', 0)
    colour = np.asarray(Image.open(BytesIO(bytes(raw[start:start + view['byteLength']]))).convert('RGB')).astype(int)
    pixels = np.clip((uv % 1) * colour.shape[0], 0, colour.shape[0] - 1).astype(int)
    return colour[pixels[:, 1], pixels[:, 0]]


def helmet(points, graph):
    # The hat and hood were generated on the head and stay where its face put them.
    return points, tuxedo.weights_from(points, graph, ('head', 'neck'), lambda name: name in ('Head', 'Neck', 'Spine2')), {}


def tabard(points, graph):
    # Generated over the fitted hauberk, so it stays where the mannequin put it and moves as the mail under it does:
    # with weights of its own the mail came through the chest in every pose but the bind pose.
    mail, raw = read_glb(OUTPUT / 'top_guard.glb')
    attributes = mail['meshes'][0]['primitives'][0]['attributes']
    under = cKDTree(io.accessor(mail, raw, attributes['POSITION']).astype(float))
    distance, nearest = under.query(points)
    # Wider and deeper about the spine so it also clears other tops (user request 2026-11-18); its height stays.
    middle = np.array([0, 0, points[:, 2].mean()])
    points = (points - middle) * [TABARD_GIRTH, 1, TABARD_GIRTH] + middle
    return points, (io.accessor(mail, raw, attributes['JOINTS_0'])[nearest].astype(np.uint16),
                    io.accessor(mail, raw, attributes['WEIGHTS_0'])[nearest].astype(float)), \
        dict(median_gap_m=round(float(np.median(distance)), 4))


def picture_uv(positions, normals, faces, colour, belt=BELT, top=PRINT_TOP, width=PRINT_WIDTH):
    """Per corner: where the chest picture falls on the front panel, else DYED cloth or the PLAIN belt."""
    middle = positions[faces].mean(1)
    leather = (middle[:, 1] > belt[0]) & (middle[:, 1] < belt[1]) & (colour.mean(1) < LEATHER)
    front = ~leather & (middle[:, 2] > .02) & (normals[faces].mean(1)[:, 2] > .2) & (middle[:, 1] > belt[0])
    corners = positions[faces]
    height = width / PRINT_ASPECT
    spot = np.stack([corners[..., 0] / width + .5, (top - corners[..., 1]) / height], -1)
    spot[~front] = [DYED, 0]
    spot[leather] = [PLAIN, 0]
    return spot.reshape(-1, 2), dict(belt_triangles=int(leather.sum()), picture_triangles=int(front.sum()),
                                     picture_m=[width, round(height, 3)], picture_top_m=top)


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    files = {path: read_glb(path) for path in (SOURCE, HELMET, TABARD)}

    def load(names, path=SOURCE):
        source, raw = files[path]
        nodes = {node['name']: source['meshes'][node['mesh']]['primitives'][0] for node in source['nodes']}
        position, uv, faces = [], [], []
        count = 0
        for name in names:
            attributes = nodes[name]['attributes']
            p = io.accessor(source, raw, attributes['POSITION']).astype(float)
            faces.append(io.accessor(source, raw, nodes[name]['indices']).reshape(-1, 3).astype(int) + count)
            position.append(p)
            uv.append(io.accessor(source, raw, attributes['TEXCOORD_0']))
            count += len(p)
        return np.vstack(position), np.vstack(uv), np.vstack(faces), path

    shoes = load(['left_boots', 'right_boots'])
    hips = JOINT['Hips'][1]
    scale = hips / (hips - shoes[0][:, 1].min())
    jobs = {'top': (load(['top']), top), 'pants': (load(['pants']), lambda p, g: trousers(p, g, scale)),
            'gloves': (load(['left_glove', 'right_glove']), tuxedo.gloves),
            'boots': (shoes, lambda p, g: boots(p, g, scale)), 'helmet': (load(['helmet'], HELMET), helmet),
            'tabard': (load(['tabard'], TABARD), tabard)}
    report = dict(date='2026-11-18', source=tuxedo.record(SOURCE), helmet_source=tuxedo.record(HELMET), tabard_source=tuxedo.record(TABARD),
                  base=tuxedo.record(ROOT / 'assets/modular_human_male_01/fitted/base.glb'),
                  rig_id='human_male_01_mixamo_candidate_v2', parts={})
    for name, ((original, uv, faces, path), place) in jobs.items():
        source, raw = files[path]
        welded, inverse = np.unique(np.round(original, 5), axis=0, return_inverse=True)
        inverse = inverse.ravel()
        graph = tuxedo.adjacency(welded, inverse[faces])
        welded, (joints, weights), placement = place(welded.copy(), graph)
        positions = welded[inverse]
        doc = dict(asset=source['asset'], meshes=[dict(name=f'{name}_guard', primitives=[dict(material=0)])],
                   materials=source['materials'], textures=source['textures'], images=source['images'],
                   samplers=source.get('samplers', []), bufferViews=source['bufferViews'],
                   accessors=[], buffers=[{}])
        doc = json.loads(json.dumps(doc))
        binary = bytearray(raw)
        primitive = doc['meshes'][0]['primitives'][0]
        normals = io.smooth_normals(positions, faces)
        joints, weights = joints[inverse], weights[inverse]
        spot = None
        if name == 'tabard':
            spot, picture = picture_uv(positions, normals, faces, texels(source, raw, uv[faces].mean(1)))
            # Corners stop sharing vertices so the picture's coordinates do not run into the cloth beside it.
            corners = faces.reshape(-1)
            positions, normals, uv, joints, weights = (x[corners] for x in (positions, normals, uv, joints, weights))
            faces = np.arange(len(corners)).reshape(-1, 3)
        primitive['attributes'] = io.add_skin_attributes(doc, binary, positions, normals, joints, weights)
        primitive['attributes']['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
        if spot is not None:
            primitive['attributes']['TEXCOORD_1'] = io.add_accessor(doc, binary, spot.astype(np.float32), 'VEC2')
        primitive['indices'] = io.add_accessor(doc, binary, faces.reshape(-1), 'SCALAR', 5125)
        fit.with_rig(doc, binary, f'{name}_guard', 'tripo_guard_v1')
        binary = io.compact(doc, binary)
        target = OUTPUT / f'{name}_guard.glb'
        fit.write_glb(target, doc, binary)
        if spot is not None:
            placement = dict(placement, picture=picture)
        report['parts'][name] = dict(placement=placement, validation=fit.validate(target))
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({name: (part['placement'], part['validation']['triangles']) for name, part in report['parts'].items()}, indent=1))


if __name__ == '__main__':
    main()
