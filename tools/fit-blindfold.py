"""Cut the blindfold off the blue mannequin it was generated on and rig it to the head.

The mannequin is a render of our own body, so aligning its generated copy to the body (rotation, uniform scale,
translation) places the blindfold. Blue triangles are dropped; the rest moves with the Head bone only.

.venv/bin/python tools/fit-blindfold.py male|female
"""
import argparse
import copy
import hashlib
import importlib.util
import io as bytes_io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.mannequin import Source

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('plate', ROOT / 'tools/fit-modular-plate.py')
io = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(io)
PROFILES = {
    'male': dict(folder='tripo_male_v1', body='assets/modular_human_male_01/fitted/base.glb',
                 regions=('head', 'neck', 'torso', 'upper_arms'), rig_id='human_male_01_mixamo_candidate_v2',
                 # A first guess at the generated bust's size, and the height (m) of the base of the neck.
                 scale=.79, neck=1.5),
    # The female mannequin was rendered from the ranger-face body, which shares the rig body's frame.
    'female': dict(folder='tripo_female_v1', body='assets/modular_human_female_01/rig_v1/female-base.glb',
                   mannequin='assets/modular_human_female_01/faces/tripo_ranger_v1/base_ranger.glb',
                   rig_id='human_female_01_local_v1', scale=.85, neck=1.52),
}
PART = 'helmet_blindfold'
TEXTURE = 1024


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def body_surface(body, binary, regions=None):
    return np.vstack([accessor(body, binary, primitive['attributes']['POSITION'])
                      for node in body['nodes'] if 'mesh' in node
                      and (regions is None or node.get('extras', {}).get('region') in regions)
                      for primitive in body['meshes'][node['mesh']]['primitives']]).astype(float)


def place(profile, path, source, blue, body, binary):
    whole = body_surface(body, binary, profile.get('regions'))
    neck = profile['neck']
    head = whole[whole[:, 1] > neck]
    bust = np.unique(source.faces[blue])
    # Start with the generated crown on the body's.
    scale = profile['scale']
    offset = np.array([0, whole[:, 1].max() - scale * source.points[bust, 1].max(), 0])
    sized = Source(path).align(bust, whole, scale, offset, upright=True, rotation=np.eye(3))
    scale, offset = sized['scale'], np.array(sized['translation'])
    placed = scale * source.points[bust] @ np.array(sized['rotation']).T + offset
    face = bust[(placed[:, 1] > neck) & (np.abs(placed[:, 0]) < .09)]
    return source.align(face, head, scale, offset, upright=True, rescale=False,
                        rotation=np.array(sized['rotation']))


def clean_texture(source, raw):
    """Mannequin blue that bled into the cloth's texels takes the nearest cloth colour."""
    spread = ndimage.binary_dilation(source.warmth < -15, iterations=3)
    nearest = ndimage.distance_transform_edt(spread, return_distances=False, return_indices=True)
    cleaned = Image.fromarray(source.colour[nearest[0], nearest[1]]).resize((TEXTURE, TEXTURE), Image.LANCZOS)
    packed = bytes_io.BytesIO()
    cleaned.save(packed, 'PNG')
    while len(raw) % 4:
        raw.append(0)
    source.doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': packed.getbuffer().nbytes})
    raw += packed.getvalue()
    source.image.update(bufferView=len(source.doc['bufferViews']) - 1, mimeType='image/png')
    source.doc['buffers'][0]['byteLength'] = len(raw)
    return int(spread.sum())


def rigged(source, faces, body, body_raw, rig_id):
    """A Head-only skinned part on the body's own skeleton."""
    used = np.unique(faces)
    index = np.full(len(source.points), -1)
    index[used] = np.arange(len(used))
    faces = index[faces]
    points, uv = source.points[used], source.uv[used]
    normal = source.normals[used]
    normal /= np.linalg.norm(normal, axis=1, keepdims=True)
    ids = body['skins'][0]['joints']
    names = [body['nodes'][i]['name'] for i in ids]
    joints = np.zeros((len(points), 4), np.uint16)
    joints[:, 0] = names.index('Head')
    weights = np.zeros((len(points), 4))
    weights[:, 0] = 1
    doc = source.doc
    raw = bytearray(source.binary)
    filled = clean_texture(source, raw)
    material = source.primitive['material']
    attrs = io.add_skin_attributes(doc, raw, points, normal, joints, weights)
    attrs['TEXCOORD_0'] = add_accessor(doc, raw, uv, 'VEC2')
    attrs['TANGENT'] = add_accessor(doc, raw, io.tangents(points, normal, uv, faces), 'VEC4')
    doc['meshes'] = [{'name': PART, 'primitives': [{'attributes': attrs, 'material': material,
                      'indices': add_accessor(doc, raw, faces.reshape(-1, 1), 'SCALAR', 5125)}]}]
    kept = [i for i, node in enumerate(body['nodes']) if 'mesh' not in node]
    remap = {old: new for new, old in enumerate(kept)}
    doc['nodes'] = [copy.deepcopy(body['nodes'][i]) for i in kept]
    for node in doc['nodes']:
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children'] if i in remap]
    doc['scenes'] = copy.deepcopy(body['scenes'])
    for scene in doc['scenes']:
        scene['nodes'] = [remap[i] for i in scene['nodes'] if i in remap]
    doc['scenes'][body.get('scene', 0)]['nodes'].append(len(doc['nodes']))
    doc['scene'] = body.get('scene', 0)
    doc['nodes'].append({'name': PART, 'mesh': 0, 'skin': 0, 'extras': {'rig_id': rig_id, 'part_id': PART}})
    skin = copy.deepcopy(body['skins'][0])
    skin['joints'] = [remap[i] for i in ids]
    if 'skeleton' in skin:
        skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(body, body_raw, skin['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    doc.pop('animations', None)
    return doc, io.compact(doc, raw), len(faces), filled


def main(sex):
    profile = PROFILES[sex]
    directory = ROOT / 'assets/items/blindfold' / profile['folder']
    path = directory / 'source/model.glb'
    source = Source(path)
    blue = source.face_share(source.warmth < -15, source.faces) > .5
    around = source.neighbours()
    for _ in range(3):
        blue = np.array([sum(not blue[other] for other in near) for near in around]) < 2
    body_path = ROOT / profile['body']
    body, body_raw = read_glb(body_path)
    fit = place(profile, path, source, blue, *read_glb(ROOT / profile.get('mannequin', profile['body'])))
    keep = source.faces[~blue]
    labels = source.components(keep)
    keep = np.vstack([keep[labels == label] for label in np.unique(labels) if (labels == label).sum() >= 20])
    doc, raw, triangles, filled = rigged(source, keep, body, body_raw, profile['rig_id'])
    out = directory / 'fitted'
    out.mkdir(exist_ok=True)
    target = out / f'{PART}.glb'
    write_glb(target, doc, raw)
    points = source.points[np.unique(keep)]
    report = {'tool': 'tools/fit-blindfold.py', 'sex': sex,
              'source': {'path': str(path.relative_to(ROOT)), 'sha256': digest(path)},
              'body': {'path': profile['body'], 'sha256': digest(body_path)}, 'rig_id': profile['rig_id'],
              'alignment': fit, 'mannequin_triangles': int(blue.sum()), 'triangles': triangles,
              'blue_texels_filled': filled, 'texture': TEXTURE,
              'bounds_m': [points.min(0).round(4).tolist(), points.max(0).round(4).tolist()],
              'weights': 'Head 100%',
              'output': {'path': str(target.relative_to(ROOT)), 'sha256': digest(target)}}
    (out / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    if sex == 'female':
        # The female packer wants one per part; the cloth hides and moves no skin.
        triangles = len(accessor(body, body_raw, body['meshes'][0]['primitives'][0]['indices'])) // 3
        (out / 'body-occlusion.json').write_text(json.dumps(
            {'mesh_index': 0, 'primitive_index': 0, 'primitive_triangles': triangles,
             'hide_triangle_indices': [], 'vertex_adjustments': []}, indent=2) + '\n')
    print(json.dumps(report, indent=1))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('sex', choices=PROFILES)
    main(parser.parse_args().sex)
