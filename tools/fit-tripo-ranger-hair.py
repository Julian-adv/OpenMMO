import importlib.util
import json
from pathlib import Path

import numpy as np

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/hair/tripo_ranger_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io


def clearance(points, faces):
    result = points.copy()
    surface = fit.body_surface(('head',))
    for _ in range(8):
        nearest, normal, _ = fit.nearest_surface(result, surface)
        signed = np.einsum('ij,ij->i', result - nearest, normal)
        active = (result[:, 1] > 1.72) & (np.linalg.norm(result - nearest, axis=1) < .045) & (signed < .005)
        result[active] = nearest[active] + normal[active] * .005
        samples = result[faces].mean(1)
        nearest, normal, _ = fit.nearest_surface(samples, surface)
        signed = np.einsum('ij,ij->i', samples - nearest, normal)
        active = (samples[:, 1] > 1.72) & (np.linalg.norm(samples - nearest, axis=1) < .04) & (signed < .003)
        updates, counts = np.zeros_like(result), np.zeros(len(result))
        for corner in range(3):
            np.add.at(updates, faces[active, corner], (nearest + normal * .005 - samples)[active])
            np.add.at(counts, faces[active, corner], 1)
        moved = counts > 0
        result[moved] += updates[moved] / counts[moved, None]
    return result


def collar_clearance(points, faces):
    path = io.PARTS / 'ranger/tripo_top_v4/top_ranger.glb'
    doc, binary = fit.read_glb(path)
    triangles = []
    for mesh in doc['meshes']:
        for primitive in mesh['primitives']:
            vertices = io.accessor(doc, binary, primitive['attributes']['POSITION'])
            indices = io.accessor(doc, binary, primitive['indices']).reshape(-1, 3)
            triangles.extend(vertices[indices])
    triangles = np.asarray(triangles)

    def offsets(samples):
        moves = np.zeros_like(samples)
        for index in np.where((samples[:, 1] < 1.67) & (samples[:, 2] < -.035))[0]:
            point = samples[index]
            center = np.array([0., point[1], -.02])
            direction = point - center
            radius = np.linalg.norm(direction)
            direction /= radius
            _, outer = fit.ray_bounds(center[None], direction, triangles, .3)
            if np.isfinite(outer[0]) and radius < outer[0] + .004:
                moves[index] = direction * (outer[0] + .004 - radius)
        return moves

    result = points.copy()
    for _ in range(4):
        result += offsets(result)
        moves = offsets(result[faces].mean(1))
        active = np.linalg.norm(moves, axis=1) > 1e-6
        updates, counts = np.zeros_like(result), np.zeros(len(result))
        for corner in range(3):
            np.add.at(updates, faces[active, corner], moves[active])
            np.add.at(counts, faces[active, corner], 1)
        mask = counts > 0
        result[mask] += updates[mask] / counts[mask, None]
    displacement = np.linalg.norm(result - points, axis=1)
    return result, dict(reference=str(path.relative_to(ROOT)), sha256=fit.digest(path),
        adjusted_vertices=int((displacement > 1e-6).sum()), maximum_displacement_mm=float(displacement.max() * 1000),
        method='Radial nape projection outside actual ranger collar at vertices and triangle centers, targeting 4mm rest clearance',
        remaining_sample_correction_mm=float(np.linalg.norm(offsets(result[faces].mean(1)), axis=1).max() * 1000))


def main():
    source = OUTPUT / 'source.glb'
    assert fit.digest(source) == '399e2fee54b1fb3c3f002d95c4faae20f7be56795a309025bb30c40e02f4d9de'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, raw, attrs['POSITION'])
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    scale, offset = [.37, .40, .38], [0, 1.535, -.028]
    initial = points * scale + offset
    fitted = clearance(initial, faces)
    scalp_displacement = np.linalg.norm(fitted - initial, axis=1)
    fitted, collar = collar_clearance(fitted, faces)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    joints = np.tile([fit.NAMES.index('Head'), 0, 0, 0], (len(fitted), 1)).astype(np.uint16)
    weights = np.tile([1., 0, 0, 0], (len(fitted), 1))
    binary = bytearray(raw)
    skin = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, faces), joints, weights)
    skin['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'] = [dict(name='hair_ranger', primitives=[dict(attributes=skin,
        material=primitive['material'], indices=io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125))])]
    fit.with_rig(doc, binary, 'hair_ranger', 'tripo_ranger_hair_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'hair_ranger.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    output_attrs = doc['meshes'][0]['primitives'][0]['attributes']
    assert np.array_equal(uv, io.accessor(doc, binary, output_attrs['TEXCOORD_0']))
    report = dict(date='2026-10-08', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source),
        original_path=r'Y:\public\web_downloads\hair+wig+3d+model.glb', mounted_path='/mnt/y/web_downloads/hair+wig+3d+model.glb',
        provider='User-supplied Tripo output', received_date='2026-10-08', generation_date=None,
        subscription_tier='Unconfirmed for this file', license='Original Tripo output terms; see doc/assets/characters.md'),
        base=dict(path=str((io.PARTS / 'fitted/base.glb').relative_to(ROOT)), sha256=fit.digest(io.PARTS / 'fitted/base.glb')),
        output=dict(path=str(target.relative_to(ROOT)), sha256=fit.digest(target)),
        initial_transform=dict(scale=scale, translation_m=offset), source_triangles=len(faces), fitted_triangles=len(faces),
        clearance=dict(adjusted_vertices=int((scalp_displacement > 1e-6).sum()), maximum_displacement_mm=float(scalp_displacement.max() * 1000),
            method='Actual canonical scalp projection at vertices and triangle centers above Y=1.72m, targeting 5mm clearance'),
        collar_clearance=collar,
        texture_bytes_and_source_uv_preserved=True, topology_preserved=True, material_preserved=True,
        validation=fit.validate(target),
        scope='Canonical male head fitting candidate with rigid Head weights; no secondary hair physics. Pose and equipment intersections require separate review.')
    (ROOT / 'doc/assets/modular-ranger-tripo-hair-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
