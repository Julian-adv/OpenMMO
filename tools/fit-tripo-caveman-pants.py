"""Fit the Tripo skirt as a rigid belt with four independently bending pelts."""
import importlib.util
import json
from pathlib import Path

import numpy as np
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from scipy.sparse.linalg import factorized

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/caveman_tripo_pants_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io
spec = importlib.util.spec_from_file_location('barbarian', ROOT / 'tools/fit-modular-barbarian.py')
barbarian = importlib.util.module_from_spec(spec)
spec.loader.exec_module(barbarian)


def clip_plane(points, uv, faces, normal):
    normal = io.unit(np.asarray(normal, dtype=float))
    tangent = np.cross([0, 1, 0], normal)
    basis = np.array([normal, [0, 1, 0], tangent])
    local, texcoords, triangles = barbarian.clip_height(points @ basis.T, uv, faces, 0, True, axis=0)
    return local @ basis, texcoords, triangles


def add_mesh(doc, binary, name, points, uv, faces, material):
    used, remapped = np.unique(faces, return_inverse=True)
    points, uv, faces = points[used], uv[used], remapped.reshape(-1, 3)
    joints = np.zeros((len(points), 4), dtype=np.uint16)
    joints[:, 0] = fit.NAMES.index('Hips')
    weights = np.zeros((len(points), 4))
    weights[:, 0] = 1
    attributes = io.add_skin_attributes(doc, binary, points, io.smooth_normals(points, faces), joints, weights)
    attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'].append(dict(name=name, primitives=[dict(attributes=attributes,
        material=material, indices=io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125))]))


def main():
    source = OUTPUT / 'source.glb'
    base = ROOT / 'assets/modular_human_male_01/parts/fitted/base.glb'
    assert fit.digest(source) == 'a7b5cb922f7c117352a45b013054103905aad9eb0b0e3ec473b17b82f575bba3'
    assert fit.digest(base) == 'ae72eb53953dd86b716859a402700eab536863e245c5261acb2592e5ef87ea5b'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(doc, raw, attrs['POSITION'])
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    _, first, inverse = np.unique(np.round(original, 5), axis=0, return_index=True, return_inverse=True)
    source_points = original[first].astype(float)
    welded_faces = inverse[faces]
    edges = np.unique(np.sort(np.concatenate([welded_faces[:, [0, 1]], welded_faces[:, [1, 2]], welded_faces[:, [2, 0]]]), axis=1), axis=0)
    rows, cols = np.r_[edges[:, 0], edges[:, 1]], np.r_[edges[:, 1], edges[:, 0]]
    adjacency = sparse.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(first), len(first)))
    count, labels = connected_components(adjacency)
    main_component = labels == np.bincount(labels).argmax()
    assert count == 19 and main_component.sum() == 925
    points = source_points * [.55, .495, .60] + [0, .65, -.020]
    scaled = points.copy()
    garment_triangles = scaled[welded_faces[np.all(main_component[welded_faces], axis=1)]]
    body = fit.body_surface(('torso', 'legs'))
    body_triangles = body[0][body[1]]
    for i, point in enumerate(scaled):
        radial = point[[0, 2]] - [0, -.020]
        radius = np.linalg.norm(radial)
        direction = np.array([radial[0] / radius, 0, radial[1] / radius])
        height = np.clip(point[1], 1.085, 1.14)
        origin = np.array([0, height, -.020])
        skin = barbarian.ray_surface(body_triangles, origin, direction)
        outer = barbarian.ray_surface(garment_triangles, origin, direction)
        assert skin is not None and outer is not None
        amount = io.smoothstep((point[1] - 1.015) / .070)
        target = skin + .018 + np.clip(radius - outer, -.006, .065)
        points[i, [0, 2]] += radial / radius * amount * (target - radius)
    for component in range(count):
        selected = labels == component
        if not np.any(selected & main_component):
            delta = np.mean(points[selected] - scaled[selected], axis=0)
            points[selected] = scaled[selected] + delta
    laplacian = sparse.diags(np.asarray(adjacency.sum(1)).ravel()) - adjacency
    smooth = factorized((sparse.eye(len(first)) + .5 * laplacian).tocsc())
    iterations = []
    movable = main_component & (points[:, 1] < 1.087)
    for step in range(20):
        nearest, normals, _ = fit.nearest_surface(points, body, candidates=96)
        signed = np.sum((points - nearest) * normals, axis=1)
        correction = normals * np.clip(.008 - signed, 0, .018)[:, None]
        correction[~movable] = 0
        delta = smooth(correction)
        delta[~movable] = 0
        points += delta
        iterations.append(dict(step=step, minimum_signed_distance_m=float(signed[main_component].min()), maximum_correction_m=float(np.linalg.norm(delta, axis=1).max())))
    positions = points[inverse]
    binary = bytearray(raw)
    material = primitive['material']
    doc['meshes'] = []
    cloth_faces = faces[np.all(main_component[inverse][faces], axis=1)]
    accessory_faces = faces[~np.all(main_component[inverse][faces], axis=1)]
    core = barbarian.clip_height(positions, uv, cloth_faces, 1.095, True)
    add_mesh(doc, binary, 'caveman_belt', *core, material)
    add_mesh(doc, binary, 'caveman_bone_ornaments', positions, uv, accessory_faces, material)
    loose = barbarian.clip_height(positions, uv, cloth_faces, 1.112, False)
    physics_by_name = {}
    for name, outward, pivot in [
        ('front', [0, 0, 1], [0, 1.106, .107]),
        ('back', [0, 0, -1], [0, 1.106, -.137]),
        ('left', [1, 0, 0], [.160, 1.106, -.020]),
        ('right', [-1, 0, 0], [-.160, 1.106, -.020]),
    ]:
        x, _, z = outward
        panel = clip_plane(*loose, [x + z, 0, z - x])
        panel = clip_plane(*panel, [x - z, 0, z + x])
        region = 'caveman_pelt_' + name
        add_mesh(doc, binary, region, *panel, material)
        physics = barbarian.panel_physics('plate', 'Hips', pivot, outward,
            float(pivot[1] - panel[0][:, 1].min()))
        physics['colliders'][0].update(center=[0, .98, -.010], radii=[.192, .15, .14])
        physics['bend'] = dict(droop=.60)
        if name in ('left', 'right'):
            side = 'Left' if name == 'left' else 'Right'
            sign = 1 if name == 'left' else -1
            physics['motion'] = dict(damping=19, stiffness=52, gravity=10, inertia=.60, max_angle=1.05,
                min_angle=.24, flex_clearance=.45)
            physics['bend']['contact'] = True
            physics['bend']['joint'] = dict(at=.50, width=.14, damping=12, stiffness=28, inertia=.65)
            physics['bend']['crease'] = dict(width=.025, angle=.16, damping=14, stiffness=36)
            physics['colliders'] = [
                dict(bone='Hips', center=[0, .98, -.010], radii=[.185, .155, .145]),
                dict(bone=side + 'UpLeg', center=[sign * .115, .81, -.012], radii=[.115, .215, .132]),
                dict(bone=side + 'Leg', center=[sign * .165, .33, -.04], radii=[.080, .25, .10]),
            ]
        elif name == 'back':
            physics['bend'] = dict(droop=.15, contact=True)
            physics['colliders'][0].update(radii=[.174, .15, .13])
        physics_by_name[region] = physics
    fit.with_rig(doc, binary, 'pants_caveman', 'tripo_caveman_pants_v1')
    for node in doc['nodes']:
        if node.get('name') in physics_by_name:
            node['extras']['pelt_physics'] = physics_by_name[node['name']]
    for mesh in doc['meshes']:
        if mesh['name'] in physics_by_name:
            mesh['extras']['pelt_physics'] = physics_by_name[mesh['name']]
    binary = io.compact(doc, binary)
    target = OUTPUT / 'pants_caveman.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    report = dict(date='2026-10-04', status='Fitted belt and four independently bending pelts; runtime and visual validation required',
        source={'path': str(source.relative_to(ROOT)), 'sha256': fit.digest(source)},
        base={'path': str(base.relative_to(ROOT)), 'sha256': fit.digest(base)},
        interfaces={'path': 'assets/modular_human_male_01/parts/interfaces/v1/interfaces.json', 'sha256': fit.digest(ROOT / 'assets/modular_human_male_01/parts/interfaces/v1/interfaces.json')},
        rig_id='human_male_01_mixamo_candidate_v2', preserved_embedded_texture=True,
        topology=dict(source_triangles=len(faces), rigid_core_meshes=2, separate_hinged_panels=4,
            method='Clip source cloth into front/back/left/right quadrants with interpolated original UV; 17mm overlap beneath the rigid waistband',
            waist_overlap_y_m=[1.095, 1.112]),
        transform=dict(scale=[.55, .495, .60], translation=[0, .65, -.020], waist='Actual body radial sections with preserved shell and accessory offsets'),
        source_components=count,
        physics=dict(anchor_bone='Hips', rigid_accessory_components=count - 1,
            method='Independent hip hinges; side pelts add a damped midpoint hinge, a longitudinal center crease and local contact. Welded edge projection caps extension at 2% plus 1 micrometer tolerance; no thigh skinning',
            panels=physics_by_name),
        clearance_iterations=iterations, bounds_m=dict(minimum=positions.min(0).tolist(), maximum=positions.max(0).tolist()), validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-caveman-tripo-pants-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation']))


if __name__ == '__main__':
    main()
