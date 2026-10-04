import importlib.util
import io as byte_io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import binary_erosion
from scipy.spatial import cKDTree

from lib.glb import view_bytes

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/parts/hair_tripo_wavy_v1'
spec = importlib.util.spec_from_file_location('fit', ROOT / 'tools/fit-modular-rogue.py')
fit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fit)
io = fit.io


def components(points, faces):
    parents = np.arange(len(points))
    def root(i):
        while parents[i] != i:
            parents[i] = parents[parents[i]]
            i = parents[i]
        return i
    for a, b in cKDTree(points).query_pairs(.00001):
        parents[root(a)] = root(b)
    for a, b, c in faces:
        parents[root(a)] = root(b)
        parents[root(c)] = root(b)
    groups = {}
    for i in range(len(points)):
        groups.setdefault(root(i), []).append(i)
    return [np.array(group) for group in groups.values()]


def fit_points(points, groups):
    result = points * [.42, .43, .55] + [0, 1.483, -.028]
    surface = fit.body_surface(('head',))
    nearest, normal, _ = fit.nearest_surface(result, surface)
    signed = np.einsum('ij,ij->i', result - nearest, normal)
    near = np.linalg.norm(result - nearest, axis=1)
    mask = (result[:, 1] > 1.70) & (near < .07) & (signed < .006)
    for group in groups:
        if np.ptp(points[group], axis=0).max() < .12:
            center = points[group].mean(0)
            fitted_center = center * [.42, .43, .55] + [0, 1.483, -.028]
            result[group] = (points[group] - center) * .43 + fitted_center
            active = group[mask[group]]
            if len(active):
                result[group] += np.mean(nearest[active] + normal[active] * .006 - result[active], axis=0)
        else:
            active = group[mask[group]]
            result[active] = nearest[active] + normal[active] * .006
    return result


def scalp_clearance(points, faces):
    result = points.copy()
    surface = fit.body_surface(('head',))
    for _ in range(5):
        samples = result[faces].mean(1)
        nearest, normal, _ = fit.nearest_surface(samples, surface)
        signed = np.einsum('ij,ij->i', samples - nearest, normal)
        mask = (samples[:, 1] > 1.71) & (np.linalg.norm(samples - nearest, axis=1) < .04) & (signed < .005)
        correction = (nearest + normal * .007 - samples)[mask]
        updates = np.zeros_like(result)
        counts = np.zeros(len(result))
        for corner in range(3):
            np.add.at(updates, faces[mask, corner], correction)
            np.add.at(counts, faces[mask, corner], 1)
        active = counts > 0
        result[active] += updates[active] / counts[active, None]
    return result


def rear_locks(source, fitted, uv, faces, groups):
    group = max(groups, key=len)
    centers = source[faces].mean(1)
    vertices, textures, triangles = [], [], []
    for side in [-1, 1]:
        mask = np.isin(faces[:, 0], group) & (centers[:, 0] * side > .06) & (centers[:, 1] < .87)
        selected = faces[mask]
        indices = np.unique(selected)
        remap = np.full(len(source), -1)
        remap[indices] = np.arange(len(indices))
        donor_faces = remap[selected]
        copy = fitted[indices].copy()
        copy[:, 0] = copy[:, 0] * .65 - side * .055
        copy[:, 2] = np.interp(copy[:, 1], [1.48, 1.58, 1.70, 1.80, 1.88], [-.190, -.170, -.135, -.115, -.065]) - .15 * copy[:, 2]
        copy = scalp_clearance(copy, donor_faces)
        triangles.extend(donor_faces + len(vertices))
        vertices.extend(copy)
        textures.extend(uv[indices])
    return np.array(vertices), np.array(textures), np.array(triangles)


def tuck_front(points):
    def smoothstep(low, high, values):
        t = np.clip((values - low) / (high - low), 0, 1)
        return t * t * (3 - 2 * t)

    nearest, normals, _ = fit.nearest_surface(points, fit.body_surface(('head',)))
    toward = nearest + normals * .009 - points
    distance = np.linalg.norm(toward, axis=1)
    influence = smoothstep(1.755, 1.805, points[:, 1])
    influence *= 1 - smoothstep(1.87, 1.925, points[:, 1])
    influence *= 1 - smoothstep(.065, .12, np.abs(points[:, 0]))
    influence *= smoothstep(.035, .075, points[:, 2])
    influence *= np.maximum(np.einsum('ij,ij->i', points - nearest, normals), 0) > .009
    amount = np.minimum(distance * .65, .018) * influence
    result = points + toward * (amount / np.maximum(distance, 1e-12))[:, None]
    active = amount > .0001
    after, _, _ = fit.nearest_surface(result[active], fit.body_surface(('head',)))
    return result, dict(adjusted_vertices=int(active.sum()),
        maximum_inward_shift_mm=float(amount.max() * 1000),
        median_clearance_before_mm=float(np.median(np.linalg.norm(points[active] - nearest[active], axis=1)) * 1000),
        median_clearance_after_mm=float(np.median(np.linalg.norm(result[active] - after, axis=1)) * 1000),
        method='Smooth local forehead taper toward the canonical head, retaining a 9mm target volume and limiting movement to 18mm')


def scalp_underlay(image):
    node = next(n for n in io.BASE['nodes'] if n.get('extras', {}).get('region') == 'head')
    primitive = io.BASE['meshes'][node['mesh']]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(io.BASE, io.BASE_BIN, attrs['POSITION'])
    normals = io.accessor(io.BASE, io.BASE_BIN, attrs['NORMAL'])
    uv = io.accessor(io.BASE, io.BASE_BIN, attrs['TEXCOORD_0'])
    faces = io.accessor(io.BASE, io.BASE_BIN, primitive['indices']).reshape(-1, 3)
    center = points[faces].mean(1)
    selected = faces[((center[:, 1] > 1.862) & (center[:, 2] < .03)) | ((center[:, 1] > 1.74) & (center[:, 2] < -.015))]
    indices = np.unique(selected)
    remap = np.full(len(points), -1)
    remap[indices] = np.arange(len(indices))
    rgb = np.asarray(Image.open(byte_io.BytesIO(image)).convert('RGB'), dtype=float)
    brown = (rgb[..., 0] > rgb[..., 1] * 1.25) & (rgb[..., 1] > rgb[..., 2] * 1.2) & (rgb[..., 0] < 110) & (rgb[..., 0] > 45)
    color = np.median(rgb[brown], axis=0)
    distances = np.linalg.norm(rgb[20:-20, 20:-20] - color, axis=2)
    distances[~binary_erosion(brown, iterations=5)[20:-20, 20:-20]] = np.inf
    y, x = np.unravel_index(distances.argmin(), distances.shape)
    x, y = x + 20, y + 20
    patch_uv = uv[indices] * [.002, .002] + [x / rgb.shape[1], y / rgb.shape[0]]
    return points[indices] + normals[indices] * .003, patch_uv, remap[selected]


def rear_underlay():
    rows, columns = 19, 13
    vertices, textures, triangles = [], [], []
    heights = [1.535, 1.57, 1.62, 1.68, 1.74, 1.80, 1.845]
    widths = [.022, .050, .079, .096, .093, .078, .057]
    depths = [-.164, -.167, -.156, -.140, -.124, -.110, -.085]
    for row, t in enumerate(np.linspace(0, 1, rows)):
        height = heights[0] + t * (heights[-1] - heights[0])
        width = np.interp(height, heights, widths)
        depth = np.interp(height, heights, depths)
        for column, u in enumerate(np.linspace(-1, 1, columns)):
            wave = np.sin(u * np.pi * 3 + t * np.pi * 2)
            vertices.append([u * width + .004 * np.sin(t * np.pi * 2), height,
                depth + .032 * u * u + .003 * wave])
            textures.append([.44 + (u + 1) * .037, .88 - t * .51])
            if row and column:
                corner = row * columns + column
                triangles.extend([[corner - columns - 1, corner - columns, corner],
                    [corner - columns - 1, corner, corner - 1]])
    return np.array(vertices), np.array(textures), np.array(triangles)


def main():
    source = OUTPUT / 'source.glb'
    assert fit.digest(source) == '0f9fe4473895d187ee08b659585d6c3b8c58bf4c72b97444bb4ebc9939af2a9f'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    points = io.accessor(doc, raw, attrs['POSITION'])
    uv = io.accessor(doc, raw, attrs['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    groups = components(points, faces)
    fitted = fit_points(points, groups)
    original_uv = uv.copy()
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    fitted = scalp_clearance(fitted, faces)
    rear, rear_uv, rear_faces = rear_locks(points, fitted, uv, faces, groups)
    fitted, front_fit = tuck_front(fitted)
    rear_faces += len(fitted)
    fitted = np.concatenate([fitted, rear])
    uv = np.concatenate([uv, rear_uv])
    scalp, scalp_uv, scalp_faces = scalp_underlay(images[0])
    scalp_faces += len(fitted)
    fitted = np.concatenate([fitted, scalp])
    uv = np.concatenate([uv, scalp_uv])
    backing, backing_uv, backing_faces = rear_underlay()
    backing_faces += len(fitted)
    fitted = np.concatenate([fitted, backing])
    uv = np.concatenate([uv, backing_uv])
    all_faces = np.concatenate([faces, rear_faces, scalp_faces, backing_faces])
    joints = np.tile([fit.NAMES.index('Head'), 0, 0, 0], (len(fitted), 1)).astype(np.uint16)
    weights = np.tile([1., 0, 0, 0], (len(fitted), 1))
    binary = bytearray(raw)
    skin_attrs = io.add_skin_attributes(doc, binary, fitted, io.smooth_normals(fitted, all_faces), joints, weights)
    skin_attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, uv, 'VEC2')
    doc['meshes'] = [dict(name='hair_wavy_bone', primitives=[dict(attributes=skin_attrs,
        material=primitive['material'], indices=io.add_accessor(doc, binary, all_faces.reshape(-1, 1), 'SCALAR', 5125))])]
    material = doc['materials'][primitive['material']]
    material['doubleSided'] = True
    material['pbrMetallicRoughness']['roughnessFactor'] = .85
    material.get('extensions', {}).pop('KHR_materials_specular', None)
    fit.with_rig(doc, binary, 'hair_wavy_bone', 'tripo_wavy_hair_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'hair_wavy_bone.glb'
    fit.write_glb(target, doc, binary)
    assert [view_bytes(doc, binary, i['bufferView']) for i in doc['images']] == images
    assert np.array_equal(uv[:len(points)], original_uv)
    report = dict(date='2026-10-04', revision='front_fit_v3', source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source),
        original_path=r'Y:\public\web_downloads\brown wavy hair 3d model.glb', provider='User-supplied Tripo Studio output',
        license='Subject to original Tripo Studio output terms; original source retained.', generation_date=None,
        subscription_tier='Current file tier unconfirmed; prior record approximately USD 20/month', received_date='2026-10-04'),
        base=dict(path=str((io.PARTS / 'fitted/base.glb').relative_to(ROOT)), sha256=fit.digest(io.PARTS / 'fitted/base.glb')),
        output=dict(path=str(target.relative_to(ROOT)), sha256=fit.digest(target)),
        method='Fit crown/side locks onto actual canonical head surface with 6mm vertex clearance. Canonical Head weighting and unchanged 65-bone bind matrices.',
        validation=fit.validate(target), source_triangles=len(faces), fitted_triangles=len(all_faces),
        rear_lock_triangles=len(rear_faces), rear_lock_method='Reuse two original side lock surfaces on a curved rear profile, retaining their original UV and texture',
        scalp_underlay_triangles=len(scalp_faces), scalp_underlay_method='Canonical scalp surface offset 3mm, using a small brown patch in the unchanged source texture; original body unaffected',
        rear_underlay_triangles=len(backing_faces), rear_underlay_method='Continuous curved nape surface behind original locks, with wave relief and original brown strand texture UV',
        forehead_fitting=front_fit,
        previous_front_shape=dict(path=str((OUTPUT / 'hair_wavy_bone-before-front-fit.glb').relative_to(ROOT)),
            sha256='fc59c008c33e92aa10eb30afd8cb964cb5baceb913058dc56674a107b4950922', status='unused; loose forehead gap reported by user', retained=False),
        superseded_candidate=dict(path=str((OUTPUT / 'hair_wavy_bone-before-rear-fill.glb').relative_to(ROOT)),
            sha256='f9299483f510b4786889b7728a610f0fbcb97fbc8e3c695e90921baca9bae1cb', status='unused; rear gaps reported by user', retained=False),
        texture_bytes_and_source_uv_preserved=True, rigid_head_weighting=True,
        material_changes=dict(double_sided=True, roughness=.85, removed_specular_extension=True))
    (ROOT / 'doc/assets/modular-wavy-hair-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
