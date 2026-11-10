"""Fit the bald Tripo ranger head to the female base rig with one neck seam.

Reuses the seam, weight and neck colour steps of fit-female-custom-face.py.
The head only gets a rotation, a uniform scale and a translation.
"""
import copy
import importlib.util
import json
from pathlib import Path

import numpy as np
import trimesh

from PIL import Image
from scipy.ndimage import binary_dilation
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('custom_face', ROOT / 'tools/fit-female-custom-face.py')
custom = importlib.util.module_from_spec(spec)
spec.loader.exec_module(custom)

BASE = custom.BASE
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_ranger_v1'
SOURCE = OUT / 'model_url.glb'
custom.OUT = OUT

SCALE = 0.293
# Chin at the custom face's chin height (1.57 m); the crown ends 15 mm below the base head's.
OFFSET = np.array([-0.002, 1.6386, -0.008])
NECK_Y = 1.552
LASH_SOURCE = ROOT / 'assets/items/female_ranger_outfit/tripo_head_v1/model_url.glb'
BLEND_BAND = 0.05
BLEND_HALF_WIDTH = 0.10


def bake_body_neck(body, faces, source, source_faces, body_image, head_image):
    """custom.bake_body_neck with a wider band: the bald source reaches the collarbones."""
    pixels = body_image.copy()
    image_height, image_width = pixels.shape[:2]
    low_y, reach = NECK_Y - BLEND_BAND, BLEND_HALF_WIDTH + .03
    coordinates, positions = [], []
    for triangle in faces:
        points = body['POSITION'][triangle]
        if points[:, 1].max() < low_y or np.abs(points[:, 0]).min() > reach:
            continue
        uv = body['TEXCOORD_0'][triangle] * [image_width - 1, image_height - 1]
        low = np.maximum(np.floor(uv.min(0)).astype(int), 0)
        high = np.minimum(np.ceil(uv.max(0)).astype(int), [image_width - 1, image_height - 1])
        x, y = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
        samples = np.column_stack([x.ravel(), y.ravel()])
        matrix = np.column_stack([uv[1] - uv[0], uv[2] - uv[0]])
        if abs(np.linalg.det(matrix)) < 1e-10:
            continue
        weights = (samples - uv[0]) @ np.linalg.inv(matrix).T
        bary = np.column_stack([1 - weights.sum(1), weights])
        inside = np.all(bary >= -1e-6, axis=1)
        world = bary[inside] @ points
        selected = (world[:, 1] >= low_y) & (np.abs(world[:, 0]) < reach)
        coordinates.append(samples[inside][selected])
        positions.append(world[selected])
    coordinates, positions = np.concatenate(coordinates), np.concatenate(positions)
    uv = custom.surface_sample(positions, source, source_faces, 'TEXCOORD_0')
    color = custom.texture_sample(head_image, uv)
    fade = custom.smoothstep((positions[:, 1] - low_y) / BLEND_BAND)
    fade *= 1 - custom.smoothstep((np.abs(positions[:, 0]) - BLEND_HALF_WIDTH) / .03)
    x, y = coordinates.T
    pixels[y, x] = pixels[y, x] * (1 - fade[:, None]) + color * fade[:, None]
    painted = np.zeros(pixels.shape[:2], dtype=bool)
    painted[y, x] = True
    padding = np.column_stack(np.nonzero(binary_dilation(painted, iterations=3) & ~painted))
    filled = np.column_stack(np.nonzero(painted))
    nearest = cKDTree(filled).query(padding)[1]
    pixels[padding[:, 0], padding[:, 1]] = pixels[filled[nearest, 0], filled[nearest, 1]]
    Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / 'body-neck-albedo.png')
    return int(len(np.unique(coordinates, axis=0)))


def components(points, faces):
    mesh = trimesh.Trimesh(points, faces, process=False)
    mesh.merge_vertices()
    groups = list(trimesh.graph.connected_components(mesh.face_adjacency, nodes=np.arange(len(faces))))
    return [(group, points[faces[group]].reshape(-1, 3)) for group in groups]


def eye_parts(points, faces, largest):
    """Loose pieces around the eyes in Tripo's frame (+X front, Z sideways): eyeballs and lashes per side."""
    balls, lashes = {}, {1: [], -1: []}
    for group, corners in components(points, faces):
        low, high = corners.min(0), corners.max(0)
        if low[0] < .1 or low[1] < .04 or high[1] > .22 or np.abs(corners[:, 2]).max() > .22:
            continue
        side = 1 if corners[:, 2].mean() > 0 else -1
        if len(group) <= largest:
            lashes[side].append(group)
        elif len(group) < 1000:
            balls[side] = corners
    return balls, {side: np.concatenate(groups) for side, groups in lashes.items()}


def transplant_lashes(head_points, head_faces):
    """Strand lashes of the first head, moved per eye by one uniform scale and a translation.

    Returns the bald head's own lash faces to drop, and the strand geometry in the bald head's frame.
    """
    donor, donor_raw = read_glb(LASH_SOURCE)
    attrs, faces = custom.geometry(donor, donor_raw, donor['meshes'][0]['primitives'][0])
    points = attrs['POSITION'].astype(np.float64)
    donor_balls, donor_lashes = eye_parts(points, faces, 16)
    own_balls, own_lashes = eye_parts(head_points, head_faces, 200)
    scale = float(np.mean([np.ptp(own_balls[side][:, 2]) / np.ptp(donor_balls[side][:, 2]) for side in (1, -1)]))
    surface = np.ones(len(head_faces), dtype=bool)
    surface[np.concatenate(list(own_lashes.values()))] = False
    skin = trimesh.Trimesh(head_points, head_faces[surface], process=False)
    moved, picked, record = points.copy(), [], {'uniform_scale': scale}
    for side in (1, -1):
        anchor = lambda ball: np.array([ball[:, 0].max(), ball[:, 1].min() + np.ptp(ball[:, 1]) / 2,
                                        ball[:, 2].min() + np.ptp(ball[:, 2]) / 2])
        source, target = anchor(donor_balls[side]), anchor(own_balls[side])
        ids = np.unique(faces[donor_lashes[side]])
        moved[ids] = (points[ids] - source) * scale + target
        picked.append(donor_lashes[side])
        distance = trimesh.proximity.closest_point(skin, moved[ids])[1]
        record['left' if side > 0 else 'right'] = {
            'strand_triangles': int(len(donor_lashes[side])), 'replaced_own_triangles': int(len(own_lashes[side])),
            'translation_source_units': (target - source * scale).tolist(),
            'nearest_skin_distance_mm': {'min': float(distance.min() * SCALE * 1000),
                                         'median': float(np.median(distance) * SCALE * 1000),
                                         'max': float(distance.max() * SCALE * 1000)}}
    used, lash_faces = np.unique(faces[np.concatenate(picked)], return_inverse=True)
    lash = {'POSITION': moved[used], 'NORMAL': attrs['NORMAL'][used].astype(np.float64),
            'TEXCOORD_0': attrs['TEXCOORD_0'][used].astype(np.float64)}
    return surface, lash, lash_faces.reshape(-1, 3), (donor, donor_raw), record


def main():
    base, base_raw = read_glb(BASE)
    source, source_raw = read_glb(SOURCE)
    original_body, body_faces = custom.geometry(base, base_raw, base['meshes'][0]['primitives'][0])
    source_attrs, source_faces = custom.geometry(source, source_raw, source['meshes'][0]['primitives'][0])
    source_attrs = {key: source_attrs[key].astype(np.float64) for key in ['POSITION', 'NORMAL', 'TEXCOORD_0']}
    surface, lash, lash_faces, (donor, donor_raw), lash_record = transplant_lashes(source_attrs['POSITION'], source_faces)
    source_faces = source_faces[surface]
    # Tripo's face looks along +X; the rig looks along +Z.
    turn = lambda v: np.stack([-v[:, 2], v[:, 1], v[:, 0]], axis=1)
    for part in (source_attrs, lash):
        part['POSITION'] = turn(part['POSITION']) * SCALE + OFFSET
        part['NORMAL'] = turn(part['NORMAL'])
    source_attrs['reference'] = source_attrs['POSITION'].copy()
    head, head_faces = custom.clip(source_attrs, source_faces, NECK_Y, True)
    head['dense'] = custom.surface_sample(head['POSITION'], original_body, body_faces, 'dense')
    head_joint = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']].index('Head')
    blend = custom.smoothstep((head['POSITION'][:, 1] - 1.59) / .02)
    head['dense'] *= 1 - blend[:, None]
    head['dense'][:, head_joint] += blend
    body, kept_faces = custom.clip(original_body, body_faces, NECK_Y, False)
    body, kept_faces, head, head_faces, seam_count, body_displacement = custom.direct_seam(
        body, kept_faces, head, head_faces, NECK_Y)
    affine_error = float(np.linalg.norm(head['POSITION'] - head.pop('reference'), axis=1).max())
    assert affine_error < 1e-12
    body_material = base['meshes'][0]['primitives'][0]['material']
    recolored = bake_body_neck(
        body, kept_faces, source_attrs, source_faces,
        custom.texture_image(base, base_raw, base['materials'][body_material]),
        custom.texture_image(source, source_raw, source['materials'][0]))
    doc, raw = copy.deepcopy(base), bytearray(base_raw)
    face_material = custom.add_material(doc, raw, source, source_raw)
    doc['materials'][face_material]['name'] = 'Female ranger face'
    image_data = (OUT / 'body-neck-albedo.png').read_bytes()
    raw.extend(b'\0' * (-len(raw) % 4))
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
    raw.extend(image_data)
    doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Body neck color match'})
    doc['textures'].append({'source': len(doc['images']) - 1})
    doc['materials'][body_material]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    lash_material = custom.add_material(doc, raw, donor, donor_raw)
    color = doc['materials'][lash_material]['pbrMetallicRoughness']['baseColorTexture']
    doc['materials'][lash_material] = {'name': 'Female ranger eyelashes', 'doubleSided': True,
                                       'pbrMetallicRoughness': {'baseColorTexture': color, 'metallicFactor': 0, 'roughnessFactor': .9}}
    lash['dense'] = np.zeros((len(lash['POSITION']), head['dense'].shape[1]))
    lash['dense'][:, head_joint] = 1
    primitives = [custom.add_geometry(doc, raw, body, kept_faces, body_material),
                  custom.add_geometry(doc, raw, head, head_faces, face_material),
                  custom.add_geometry(doc, raw, lash, lash_faces, lash_material)]
    doc['meshes'] = [{'name': 'female_body', 'primitives': [primitives[0]]},
                     {'name': 'face_female_ranger', 'primitives': [primitives[1]]},
                     {'name': 'eyelashes_female_ranger', 'primitives': [primitives[2]]}]
    doc['nodes'].append({'name': 'eyelashes_female_ranger', 'mesh': 2, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'eyelashes_female_ranger'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    doc['nodes'][0]['mesh'] = 0
    doc['nodes'].append({'name': 'face_female_ranger', 'mesh': 1, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'face_female_ranger'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    assembled = OUT / 'base_ranger.glb'
    write_glb(assembled, doc, custom.compact(doc, raw))
    check, check_raw = read_glb(assembled)
    assert check['nodes'][:len(base['nodes'])] == base['nodes']
    assert np.array_equal(accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']),
                          accessor(check, check_raw, check['skins'][0]['inverseBindMatrices']))
    report = {'date': '2026-11-10', 'source_path': str(SOURCE.relative_to(ROOT)), 'source_sha256': custom.digest(SOURCE),
              'base_sha256': custom.digest(BASE), 'rotation': 'Tripo +X front to rig +Z front',
              'scale': SCALE, 'offset_m': OFFSET.tolist(), 'neck_cut_y_m': NECK_Y,
              'new_head_rotation_scale_translation_only': True, 'maximum_head_affine_error_m': affine_error,
              'source_triangles': len(source_faces), 'head_triangles': len(head_faces),
              'retained_body_triangles': len(kept_faces), 'neck_boundary_vertices': seam_count,
              'body_neck_fit_maximum_mm': body_displacement, 'body_neck_recolored_pixels': recolored,
              'rig_id': 'human_female_01_local_v1',
              'eyelashes': {'source_path': str(LASH_SOURCE.relative_to(ROOT)), 'source_sha256': custom.digest(LASH_SOURCE),
                            'method': 'Strand lashes of the first head replace the bald head\'s lash plates', **lash_record},
              'files': {name: custom.digest(OUT / name) for name in ['base_ranger.glb', 'body-neck-albedo.png']}}
    (OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
