"""Fit the bald Tripo ranger head to the female base rig with one neck seam.

Reuses the seam, weight and neck colour steps of fit-female-custom-face.py.
The head only gets a rotation, a uniform scale and a translation.
"""
import copy
import io
import importlib.util
import json
from pathlib import Path

import numpy as np
import trimesh

from PIL import Image
from scipy.ndimage import binary_dilation, gaussian_filter1d
from scipy.spatial import cKDTree

from lib.glb import read_glb, view_bytes, write_glb
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
SMOOTH_BELOW, SMOOTH_ABOVE = 0.04, 0.02
SMOOTH_RADIUS = 0.064
SMOOTH_SIGMA = 0.008
NECK_AXIS_Z = -0.03
BLEND_HALF_WIDTH = 0.10


def material_image(doc, raw, texture):
    image = doc['images'][doc['textures'][texture['index']]['source']]
    return np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, image['bufferView']))).convert('RGB'), dtype=np.float32) / 255


def ring_angles(points, start=None):
    """Turn of each ring point around the neck axis, as a fraction of a full turn from the first point."""
    angle = np.arctan2(points[:, 0], points[:, 2] - NECK_AXIS_Z) / (2 * np.pi)
    step = np.diff(np.unwrap(angle * 2 * np.pi)) / (2 * np.pi)
    assert np.all(step > 0) or np.all(step < 0), 'Neck ring must wind once around the axis'
    turn = (angle - (angle[0] if start is None else start)) * (1 if step[0] > 0 else -1)
    return turn % 1, angle[0]


def angular_seam(body, body_faces, head, head_faces):
    """custom.direct_seam with ring points paired by angle around the neck instead of by arc length.

    Pairing by arc length slides ring vertices sideways and moves each body vertex by its nearest ring
    segment, which creases the neck. Paired by angle, every body vertex moves only along its own radius,
    by the ring's radius change at its angle.
    """
    lower, upper = custom.ring(body, body_faces, NECK_Y), custom.ring(head, head_faces, NECK_Y)
    low_turn, start = ring_angles(body['POSITION'][lower])
    high_turn, _ = ring_angles(head['POSITION'][upper], start)
    shift = int(np.argmin(high_turn))
    upper, high_turn = np.roll(upper, -shift), np.roll(high_turn, -shift)
    bottom, top = body['POSITION'][lower].copy(), head['POSITION'][upper].copy()
    a, b = np.r_[low_turn, 1], np.r_[high_turn, 1 + high_turn[0]]
    assert np.all(np.diff(a) > 0) and np.all(np.diff(b) > 0)
    targets = np.unique(np.round(np.r_[low_turn, high_turn], 12))
    wrapped = np.sort(np.where(targets < high_turn[0], targets + 1, targets))
    body, body_faces = custom.refine_ring(body, body_faces, lower, a, np.r_[targets, 1], NECK_Y)
    head, head_faces = custom.refine_ring(head, head_faces, upper, b, wrapped, NECK_Y)
    canonical = custom.sample(bottom, a, targets)
    destination = np.column_stack([np.interp(np.where(targets < high_turn[0], targets + 1, targets), b,
                                             np.r_[top[:, i], top[0, i]]) for i in range(3)])
    original = body['POSITION'].copy()
    offset = original - [0, 0, NECK_AXIS_Z]
    radius = np.hypot(offset[:, 0], offset[:, 2])
    turn = ((np.arctan2(offset[:, 0], offset[:, 2]) / (2 * np.pi) - start)
            * (1 if low_turn[1] < .5 else -1)) % 1
    grow = np.hypot(destination[:, 0], destination[:, 2] - NECK_AXIS_Z) - np.hypot(canonical[:, 0], canonical[:, 2] - NECK_AXIS_Z)
    change = np.interp(turn, np.r_[targets, 1], np.r_[grow, grow[0]])
    fade = custom.smoothstep((original[:, 1] - NECK_Y + .05) / .05) * (1 - custom.smoothstep((radius - .07) / .03))
    outward = np.stack([offset[:, 0], np.zeros(len(original)), offset[:, 2]], axis=1) / np.maximum(radius, 1e-9)[:, None]
    body['POSITION'] = original + outward * (change * fade)[:, None]
    selected = fade > 0
    for old, new in zip(np.round(canonical, 8), destination):
        body['POSITION'][np.all(np.round(original, 8) == old, axis=1)] = new
    body['NORMAL'][selected] = custom.smooth_normals(body['POSITION'], body_faces)[selected]
    head_positions = np.round(head['POSITION'], 8)
    for point in destination:
        lower_ids = np.flatnonzero(np.all(np.round(body['POSITION'], 8) == np.round(point, 8), axis=1))
        upper_ids = np.flatnonzero(np.all(np.abs(head['POSITION'] - point) < 2e-8, axis=1))
        assert len(lower_ids) and len(upper_ids), 'The seam must share every boundary vertex'
        head['POSITION'][upper_ids] = body['POSITION'][lower_ids[0]]
        head['dense'][upper_ids] = body['dense'][lower_ids[0]]
        body['NORMAL'][lower_ids] = custom.unit(head['NORMAL'][upper_ids[:1]])
    return body, body_faces, head, head_faces, len(destination), float(np.linalg.norm(body['POSITION'] - original, axis=1).max() * 1000)


def smooth_seam(body, body_faces, head, head_faces):
    """Smooth the neck radius along its height in a band around the seam, so the outline has no ridge there.

    Vertices move only towards or away from the neck axis. The weight is 1 on the seam and 0 at the band ends.
    """
    points = np.concatenate([body['POSITION'], head['POSITION']])
    faces = np.concatenate([body_faces, head_faces + len(body['POSITION'])])
    axis = np.array([0, 0, NECK_AXIS_Z])
    mesh = trimesh.Trimesh(points, faces, process=False)
    heights = np.arange(NECK_Y - SMOOTH_BELOW - .03, NECK_Y + SMOOTH_ABOVE + .03, .002)
    angles = np.radians(np.arange(0, 360, 3))
    directions = np.stack([np.sin(angles), np.zeros_like(angles), np.cos(angles)], axis=1)
    origins = np.repeat(axis[None] + np.outer(heights, [0, 1, 0]), len(angles), axis=0)
    rays = np.tile(directions, (len(heights), 1))
    hits, ray_ids, _ = mesh.ray.intersects_location(origins, rays, multiple_hits=True)
    field = np.zeros(len(origins))
    np.maximum.at(field, ray_ids, np.einsum('ij,ij->i', hits - origins[ray_ids], rays[ray_ids]))
    field = field.reshape(len(heights), len(angles))
    column = ((field > 0) & (field < SMOOTH_RADIUS + .008)).astype(np.float64)
    sigma = SMOOTH_SIGMA / .002
    blurred = gaussian_filter1d(field * column, sigma, axis=0, mode='nearest')
    blurred /= np.maximum(gaussian_filter1d(column, sigma, axis=0, mode='nearest'), 1e-6)
    change = np.where(column > 0, blurred - field, 0)
    change = gaussian_filter1d(change, 1.5, axis=1, mode='wrap')

    offset = points - axis
    radius = np.hypot(offset[:, 0], offset[:, 2])
    y = points[:, 1]
    weight = np.where(y < NECK_Y, custom.smoothstep((y - NECK_Y + SMOOTH_BELOW) / SMOOTH_BELOW),
                      custom.smoothstep((NECK_Y + SMOOTH_ABOVE - y) / SMOOTH_ABOVE))
    # Only the neck column: the shoulders and the underside of the jaw stay as they are.
    weight *= 1 - custom.smoothstep((radius - SMOOTH_RADIUS) / .008)
    row = np.clip((y - heights[0]) / .002, 0, len(heights) - 1.001)
    turn = (np.degrees(np.arctan2(offset[:, 0], offset[:, 2])) % 360) / 3
    r0, c0 = np.floor(row).astype(int), np.floor(turn).astype(int) % len(angles)
    fr, fc, c1 = row - r0, turn - np.floor(turn), (c0 + 1) % len(angles)
    delta = ((change[r0, c0] * (1 - fc) + change[r0, c1] * fc) * (1 - fr) +
             (change[r0 + 1, c0] * (1 - fc) + change[r0 + 1, c1] * fc) * fr) * weight
    outward = np.stack([offset[:, 0], np.zeros(len(points)), offset[:, 2]], axis=1) / np.maximum(radius, 1e-9)[:, None]
    moved = points + outward * delta[:, None]

    # Normals stay as they are. Rebuilding them from the faces gave every vertex added on a straight ring
    # edge that edge's flat normal, which showed as a row of teeth along the seam.
    split = len(body['POSITION'])
    body['POSITION'], head['POSITION'] = moved[:split], moved[split:]
    return {'band_m': [NECK_Y - SMOOTH_BELOW, NECK_Y + SMOOTH_ABOVE], 'height_sigma_m': SMOOTH_SIGMA,
            'vertices': int(np.count_nonzero(weight > 0)),
            'body_maximum_mm': float(np.abs(delta[:split]).max() * 1000),
            'head_maximum_mm': float(np.abs(delta[split:]).max() * 1000)}


def bake_body_neck(body, faces, source, source_faces, body_image, head_image, name):
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
    color = custom.texture_sample(head_image, custom.surface_sample(positions, source, source_faces, 'TEXCOORD_0'))
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
    Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / name)
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
    body, kept_faces, head, head_faces, seam_count, body_displacement = angular_seam(body, kept_faces, head, head_faces)
    assert float(np.linalg.norm(head['POSITION'] - head['reference'], axis=1).max()) < 1e-7
    smoothing = smooth_seam(body, kept_faces, head, head_faces)
    reference = head.pop('reference')
    untouched = reference[:, 1] > NECK_Y + SMOOTH_ABOVE
    affine_error = float(np.linalg.norm(head['POSITION'] - reference, axis=1)[untouched].max())
    assert affine_error < 1e-7
    body_material = base['meshes'][0]['primitives'][0]['material']
    body_maps, head_maps = base['materials'][body_material], source['materials'][0]
    recolored = bake_body_neck(
        body, kept_faces, source_attrs, source_faces,
        material_image(base, base_raw, body_maps['pbrMetallicRoughness']['baseColorTexture']),
        material_image(source, source_raw, head_maps['pbrMetallicRoughness']['baseColorTexture']), 'body-neck-albedo.png')
    bake_body_neck(
        body, kept_faces, source_attrs, source_faces,
        material_image(base, base_raw, body_maps['pbrMetallicRoughness']['metallicRoughnessTexture']),
        material_image(source, source_raw, head_maps['pbrMetallicRoughness']['metallicRoughnessTexture']), 'body-neck-orm.png')

    doc, raw = copy.deepcopy(base), bytearray(base_raw)
    face_material = custom.add_material(doc, raw, source, source_raw)
    doc['materials'][face_material]['name'] = 'Female ranger face'
    fitted = doc['materials'][body_material]
    for name, slot in [('body-neck-albedo.png', fitted['pbrMetallicRoughness']['baseColorTexture']),
                       ('body-neck-orm.png', fitted['pbrMetallicRoughness']['metallicRoughnessTexture'])]:
        image_data = (OUT / name).read_bytes()
        raw.extend(b'\0' * (-len(raw) % 4))
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
        raw.extend(image_data)
        doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': name})
        doc['textures'].append({'source': len(doc['images']) - 1})
        slot['index'] = len(doc['textures']) - 1
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
              'head_affine_scope': 'Rotation, uniform scale and translation above the seam smoothing band',
              'maximum_head_affine_error_m': affine_error, 'seam_smoothing': smoothing,
              'source_triangles': len(source_faces), 'head_triangles': len(head_faces),
              'retained_body_triangles': len(kept_faces), 'neck_boundary_vertices': seam_count,
              'body_neck_fit_maximum_mm': body_displacement, 'body_neck_recolored_pixels': recolored,
              'rig_id': 'human_female_01_local_v1',
              'eyelashes': {'source_path': str(LASH_SOURCE.relative_to(ROOT)), 'source_sha256': custom.digest(LASH_SOURCE),
                            'method': 'Strand lashes of the first head replace the bald head\'s lash plates', **lash_record},
              'files': {name: custom.digest(OUT / name) for name in ['base_ranger.glb', 'body-neck-albedo.png', 'body-neck-orm.png']}}
    (OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
