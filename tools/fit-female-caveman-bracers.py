"""Fit mirrored bracers to both female forearms."""
import copy
import hashlib
import json
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb, view_bytes
from lib.glb_accessors import accessor, add_accessor
from outfits.rogue_layers import wrist_section

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_caveman_bracers/tripo_v1'
SOURCE = DIRECTORY / 'source/model_url.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUTPUT = DIRECTORY / 'fitted_v1'
REPORT = ROOT / 'doc/assets/modular-female-caveman-bracers-fitting-v1.json'
def add_skin_attributes(doc, binary, positions, normals, joints, weights):
    joints[weights == 0] = 0
    return {
        'POSITION': add_accessor(doc, binary, positions, 'VEC3'),
        'NORMAL': add_accessor(doc, binary, normals, 'VEC3'),
        'JOINTS_0': add_accessor(doc, binary, joints, 'VEC4', 5123),
        'WEIGHTS_0': add_accessor(doc, binary, weights, 'VEC4'),
    }


def unit(v):
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def smooth_normals(positions, indices):
    unique, inverse = np.unique(np.round(positions, 6), axis=0, return_inverse=True)
    normals = np.zeros_like(unique)
    face = positions[indices]
    cross = np.cross(face[:, 1] - face[:, 0], face[:, 2] - face[:, 0])
    for corner in range(3):
        np.add.at(normals, inverse[indices[:, corner]], cross)
    lengths = np.linalg.norm(normals, axis=1, keepdims=True)
    return (normals / np.maximum(lengths, 1e-10))[inverse]


def compact(doc, binary):
    used = {skin['inverseBindMatrices'] for skin in doc['skins']}
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            used.update(primitive['attributes'].values())
            used.add(primitive['indices'])
            for index in primitive['attributes'].values():
                doc['bufferViews'][doc['accessors'][index]['bufferView']]['target'] = 34962
            doc['bufferViews'][doc['accessors'][primitive['indices']]['bufferView']]['target'] = 34963
    remap = {old: new for new, old in enumerate(sorted(used))}
    doc['accessors'] = [doc['accessors'][i] for i in sorted(used)]
    for skin in doc['skins']:
        skin['inverseBindMatrices'] = remap[skin['inverseBindMatrices']]
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            primitive['attributes'] = {k: remap[v] for k, v in primitive['attributes'].items()}
            primitive['indices'] = remap[primitive['indices']]
    refs = doc['accessors'] + doc['images']
    used_views = sorted({ref['bufferView'] for ref in refs})
    remap = {old: new for new, old in enumerate(used_views)}
    packed = bytearray()
    views = []
    for index in used_views:
        view = copy.deepcopy(doc['bufferViews'][index])
        start = view.get('byteOffset', 0)
        packed.extend(b'\0' * (-len(packed) % 4))
        view['byteOffset'] = len(packed)
        packed.extend(binary[start:start+view['byteLength']])
        views.append(view)
    for ref in refs:
        ref['bufferView'] = remap[ref['bufferView']]
    doc['bufferViews'] = views
    doc['buffers'] = [{'byteLength': len(packed)}]
    return bytes(packed)


WRIST_OFFSET = .028
LENGTH = .153
CLEARANCE = .00396
SOFTNESS = .00144
HIDE_MARGIN = .0072
SOURCE_BASIS = np.array([[0, 0, -1], [1, 0, 0]], dtype=float)


def source_section(triangles, y, radial):
    axis = np.array([0., 1., 0.])
    origin = np.array([0., y, 0.])
    distances = (triangles-origin) @ axis
    crossing = (distances.min(axis=1) < 0) & (distances.max(axis=1) > 0)
    segments = []
    for tri, values in zip(triangles[crossing], distances[crossing]):
        hits = []
        for a, b in [(0, 1), (1, 2), (2, 0)]:
            if values[a] * values[b] < 0:
                t = values[a] / (values[a]-values[b])
                hits.append((tri[a]+t*(tri[b]-tri[a])-origin) @ SOURCE_BASIS.T)
        if len(hits) == 2:
            segments.append(hits)
    segments = np.array(segments)
    assert len(segments) >= 3
    mid = (segments.reshape(-1, 2).min(0)+segments.reshape(-1, 2).max(0))/2
    start, edge = segments[:, 0]-mid, segments[:, 1]-segments[:, 0]
    cross = lambda a, b: a[..., 0]*b[..., 1]-a[..., 1]*b[..., 0]
    denominator = cross(radial[:, None], edge)
    along = np.divide(cross(start, edge)[None, :], denominator, out=np.full_like(denominator, np.inf), where=abs(denominator)>1e-10)
    between = np.divide(cross(start, radial[:, None]), denominator, out=np.full_like(denominator, np.inf), where=abs(denominator)>1e-10)
    valid = (between >= 0) & (between <= 1) & (along > 0)
    radius = np.where(valid, along, np.inf).min(axis=1)
    good = np.isfinite(radius)
    assert good.sum() >= len(radial)//2, f'Insufficient source section at {y}'
    angles = np.arange(len(radial))*2*np.pi/len(radial)
    radius[~good] = np.interp(angles[~good], angles[good], radius[good], period=2*np.pi)
    return mid, radius


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def setup_body():
    doc, raw = read_glb(BODY)
    skin = doc['skins'][0]
    names = [doc['nodes'][i]['name'] for i in skin['joints']]
    bind = accessor(doc, raw, skin['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    bones = dict(zip(names, np.linalg.inv(bind)[:, :3, 3]))
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = accessor(doc, raw, attrs['POSITION']).astype(float)
    faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    dense = np.zeros((len(points), len(names)))
    np.add.at(dense, (np.arange(len(points))[:, None], accessor(doc, raw, attrs['JOINTS_0'])), accessor(doc, raw, attrs['WEIGHTS_0']))
    return doc, raw, names, bones, points, faces, dense


def frame(bones, side):
    wrist = bones[side + 'Hand']
    axis = unit(bones[side + 'ForeArm'] - wrist)
    width = bones[side + 'HandIndex1'] - bones[side + 'HandPinky1']
    dorsal = np.cross(bones[side + 'HandMiddle1'] - wrist, width)
    dorsal = unit(dorsal - axis * np.dot(dorsal, axis))
    if side == 'Left':
        dorsal = -dorsal
    across = np.cross(dorsal, -axis)
    if side == 'Left':
        across = -across
    return wrist, axis, np.array([across, dorsal])


def fit_side(points, faces, body_points, body_faces, arm_faces, bones, side):
    wrist, axis, basis = frame(bones, side)
    theta = np.arange(128) * 2 * np.pi / 128
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    low, high = .13, .99951172
    source_rows = np.linspace(.25, .90, 40)
    source_sections = [source_section(points[faces], y, radial) for y in source_rows]
    source_centers = np.array([center for center, _ in source_sections])
    source_radii = np.array([radius for _, radius in source_sections])
    scale = LENGTH / (high - low)
    heights = WRIST_OFFSET + (points[:, 1] - low) * scale
    rows = np.linspace(WRIST_OFFSET, WRIST_OFFSET + LENGTH, 48)
    triangles = body_points[body_faces[arm_faces]]
    sections = [wrist_section(triangles, wrist + axis * h, axis, basis, radial) for h in rows]
    centers = np.array([center for center, _ in sections])
    radii = np.array([radius for _, radius in sections])
    source_center = np.column_stack([np.interp(points[:, 1], source_rows, source_centers[:, j]) for j in range(2)])
    cross = (points @ SOURCE_BASIS.T) - source_center
    angles = np.arctan2(cross[:, 1], cross[:, 0])

    def sample(table, levels, coordinates):
        values = np.array([np.interp(angles, theta, row, period=2 * np.pi) for row in table])
        return np.array([np.interp(c, levels, values[:, i]) for i, c in enumerate(coordinates)])

    depth = (np.linalg.norm(cross, axis=1) - sample(source_radii, source_rows, points[:, 1])) * scale
    offset = CLEARANCE + SOFTNESS * np.logaddexp(0, depth / SOFTNESS)
    radius = sample(radii, rows, heights) + offset
    center = np.column_stack([np.interp(heights, rows, centers[:, j]) for j in range(2)])
    fitted = wrist + heights[:, None] * axis
    fitted += (center + radius[:, None] * np.column_stack([np.cos(angles), np.sin(angles)])) @ basis
    skin_radius = sample(radii, rows, heights)
    return fitted, dict(wrist=wrist.tolist(), axis=axis.tolist(), basis=basis.tolist(), scale=float(scale),
        axial_interval_m=[float(WRIST_OFFSET), float(WRIST_OFFSET + LENGTH)],
        forearm_length_m=float(np.linalg.norm(bones[side + 'ForeArm'] - wrist)),
        minimum_offset_m=float(offset.min()), maximum_offset_m=float(offset.max()),
        rest_skin_gap_min_m=float((radius - skin_radius).min()))


def hidden_skin(body_points, body_faces, arm_vertices, bones, side, interval):
    wrist, axis, _ = frame(bones, side)
    along = (body_points - wrist) @ axis
    inside = arm_vertices & (along > interval[0] + HIDE_MARGIN) & (along < interval[1] - HIDE_MARGIN)
    return inside[body_faces].all(1)


def attach_rig(doc, raw, base, base_raw, sides):
    ids = base['skins'][0]['joints']
    remap = {old: i for i, old in enumerate(ids)}
    doc['nodes'] = [copy.deepcopy(base['nodes'][i]) for i in ids]
    for bone in doc['nodes']:
        if 'children' in bone:
            bone['children'] = [remap[i] for i in bone['children'] if i in remap]
    children = {child for bone in doc['nodes'] for child in bone.get('children', [])}
    armature = len(doc['nodes'])
    doc['nodes'].append({'name': 'Armature', 'children': [i for i in range(len(ids)) if i not in children], 'extras': {'rig_id': 'human_female_01_local_v1'}})
    doc['scenes'] = [{'nodes': [armature], 'extras': {'rig_id': 'human_female_01_local_v1'}}]
    doc['scene'] = 0
    skin = copy.deepcopy(base['skins'][0])
    skin['joints'] = [remap[i] for i in ids]
    skin['skeleton'] = remap[skin['skeleton']]
    skin['inverseBindMatrices'] = add_accessor(doc, raw, accessor(base, base_raw, skin['inverseBindMatrices']), 'MAT4')
    doc['skins'] = [skin]
    for i, side in enumerate(sides):
        doc['scenes'][0]['nodes'].append(len(doc['nodes']))
        doc['nodes'].append({'name': f'gloves_female_caveman_{side.lower()}', 'mesh': i, 'skin': 0,
                             'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'gloves_female_caveman'}})
    doc.pop('animations', None)


def main():
    OUTPUT.mkdir(exist_ok=True)
    assert digest(SOURCE) == '4272c6f0eedf3057f69de43e5dd80fca8718b105333f9b2181958adf0a4a6351'
    assert digest(BODY) == '3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5'
    base, base_raw, names, bones, body_points, body_faces, dense = setup_body()
    doc, source_raw = read_glb(SOURCE)
    images = [view_bytes(doc, source_raw, image['bufferView']) for image in doc['images']]
    primitive = doc['meshes'][0]['primitives'][0]
    points = accessor(doc, source_raw, primitive['attributes']['POSITION']).astype(float)
    uv = accessor(doc, source_raw, primitive['attributes']['TEXCOORD_0'])
    faces = accessor(doc, source_raw, primitive['indices']).reshape(-1, 3)
    radial = np.linalg.norm(points[:, [0, 2]], axis=1)
    caps = np.any((radial[faces] < .11) & (points[faces, 1] > .12), axis=1)
    removed_caps = int(caps.sum())
    faces = faces[~caps]
    raw = bytearray(source_raw)
    doc['meshes'] = []
    sides, records, hide = ['Right', 'Left'], {}, np.zeros(len(body_faces), dtype=bool)
    for side in sides:
        arm = dense[:, [names.index(side + bone) for bone in ['Arm', 'ForeArm', 'Hand']]].sum(1)
        arm_faces = arm[body_faces].mean(1) > .5
        fitted, record = fit_side(points, faces, body_points, body_faces, arm_faces, bones, side)
        triangles = faces[:, ::-1] if side == 'Left' else faces
        normals = smooth_normals(fitted, triangles)
        joints = np.tile([names.index(side + 'ForeArm'), 0, 0, 0], (len(fitted), 1)).astype(np.uint16)
        weights = np.tile([1., 0, 0, 0], (len(fitted), 1))
        attrs = add_skin_attributes(doc, raw, fitted, normals, joints, weights)
        attrs['TEXCOORD_0'] = add_accessor(doc, raw, uv, 'VEC2')
        doc['meshes'].append({'name': f'gloves_female_caveman_{side.lower()}', 'primitives': [{
            'attributes': attrs, 'material': primitive['material'],
            'indices': add_accessor(doc, raw, triangles.reshape(-1, 1).astype(np.uint32), 'SCALAR', 5125)}]})
        interval = np.array(record['axial_interval_m'])
        side_hide = hidden_skin(body_points, body_faces, arm > .99, bones, side, interval)
        hide |= side_hide
        records[side] = {**record, 'triangles': len(triangles), 'vertices': len(fitted), 'hidden_body_triangles': int(side_hide.sum())}
    attach_rig(doc, raw, base, base_raw, sides)
    raw = compact(doc, raw)
    assert images == [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    target = OUTPUT / 'gloves_female_caveman.glb'
    write_glb(target, doc, raw)
    mask = {'body': str(BODY.relative_to(ROOT)), 'body_sha256': digest(BODY), 'mesh_index': 0, 'primitive_index': 0,
            'primitive_triangles': len(body_faces), 'hide_triangle_indices': np.flatnonzero(hide).tolist(),
            'scope': f'Forearm skin fully inside each bracer, {HIDE_MARGIN * 1000:.1f} mm in from both openings; hands stay visible.'}
    (OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')
    report = {'date': '2026-11-01', 'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': digest(SOURCE)},
              'body': {'path': str(BODY.relative_to(ROOT)), 'sha256': digest(BODY)},
              'output': {'path': str(target.relative_to(ROOT)), 'sha256': digest(target)},
              'rig_id': 'human_female_01_local_v1',
              'wrist_offset_m': WRIST_OFFSET, 'length_m': LENGTH,
              'clearance_m': CLEARANCE, 'sides': records, 'hidden_body_triangles': int(hide.sum()),
              'method': 'Map the source inner wall per height and angle onto 48 actual forearm sections plus clearance, keeping '
                        'relief above the inner wall at the uniform length scale. The source flare toward the elbow follows the forearm. '
                        'Left uses the mirrored frame on its own bones with reversed winding. Each bracer follows its ForeArm rigidly.',
              'removed_interior_cap_faces_per_side': removed_caps, 'original_uv_and_embedded_textures_preserved': True, 'status': 'Candidate; see animation-validation.json for sampled diagnostics'}
    REPORT.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({side: {k: records[side][k] for k in ['axial_interval_m', 'forearm_length_m', 'minimum_offset_m', 'maximum_offset_m', 'rest_skin_gap_min_m', 'hidden_body_triangles']} for side in sides}, indent=1))


if __name__ == '__main__':
    main()
