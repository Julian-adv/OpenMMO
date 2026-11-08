import numpy as np
from PIL import Image, ImageDraw
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.ndimage import binary_dilation, gaussian_filter
from scipy.spatial import cKDTree
from trimesh.triangles import closest_point


PATCHES = [392, 393, 394, 395, 486, 504, 603, 1262, 1263, 1421, 1423, 1428, 1433, 1434]
LOWER_PATCHES = [485, 488, 489, 499, 503, 522, 527, 538, 541, 555, 556, 557, 1375, 1382, 1402, 1404, 1405, 1416, 1417, 1420, 1424]
CORNER_PATCHES = [492, 504, 505, 509, 1412, 1415, 1421, 1428]


def uv_labels(points, faces):
    edges = faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)
    graph = coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])), shape=(len(points),) * 2)
    return connected_components(graph, directed=False)[1]


def select_lashes(points, faces):
    labels = uv_labels(points, faces)
    upper = np.isin(labels, PATCHES)
    lower = np.isin(labels, LOWER_PATCHES)
    selected = upper | lower
    assert np.all((points[selected, 1] > .92) & (points[selected, 1] < .95))
    return (upper.astype(float) + lower.astype(float) * 2).reshape(-1, 1)


def repair_corner_maps(points, faces, uv, images, output):
    labels = uv_labels(points, faces)
    selected = np.isin(labels[faces[:, 0]], CORNER_PATCHES)
    center = points[faces].mean(axis=1)
    corner = (np.abs(center[:, 0] + .0008) > .038) & (np.abs(center[:, 0] + .0008) < .065)
    corner &= (center[:, 1] > 1.681) & (center[:, 1] < 1.698) & (center[:, 2] > .045)
    selected |= corner
    record = {'source_uv_components': CORNER_PATCHES, 'triangles': int(selected.sum()),
              'outer_corner_region_m': {'absolute_x': [.038, .065], 'y': [1.681, 1.698], 'minimum_z': .045},
              'normal_rgb': [128, 128, 255], 'roughness': 230 / 255, 'metallic': 0,
              'pixels_outside_mask_preserved': True}
    for role, image in images.items():
        image = image.convert('RGB')
        mask = Image.new('L', image.size)
        draw = ImageDraw.Draw(mask)
        for triangle in faces[selected]:
            polygon = uv[triangle] * (np.asarray(image.size) - 1)
            draw.polygon([tuple(point) for point in polygon], fill=255)
        weight = gaussian_filter(binary_dilation(np.asarray(mask) > 0, iterations=1).astype(float), .5)
        source = np.asarray(image)
        target = source.copy()
        if role == 'normal':
            target[:] = [128, 128, 255]
        else:
            target[..., 1], target[..., 2] = 230, 0
        result = np.round(source * (1 - weight[..., None]) + target * weight[..., None]).astype(np.uint8)
        Image.fromarray(result).save(output / f'face-corner-{role}.png')
        record[f'{role}_changed_pixels'] = int(np.any(result != source, axis=2).sum())
        assert np.array_equal(result[weight == 0], source[weight == 0])
    return record


def lash_uv(points, kinds):
    x = np.abs(points[:, 0] + .0008)
    knots = [.018, .023, .030, .038, .046, .054, .062]
    root_y = np.interp(x, knots, [1.684, 1.6866, 1.6892, 1.6892, 1.6866, 1.6857, 1.6848])
    root_z = np.interp(x, knots, [.068, .065, .0663, .0663, .0602, .0567, .053])
    distance = (points[:, 1] - root_y) * .55 + (points[:, 2] - root_z) * .84 + .00025
    uv = np.column_stack([(x - .0175) / .045, (.92 - distance / .014) * .5])
    lower = kinds > 1.5
    lower_root = np.interp(x[lower], [.016, .025, .033, .041, .049, .057],
                           [1.6815, 1.679, 1.6785, 1.680, 1.682, 1.684])
    uv[lower, 0] = (x[lower] - .016) / .041
    uv[lower, 1] = .5 + (.92 + (points[lower, 1] - lower_root) / .008) * .5
    uv[~lower, 1] = np.clip(uv[~lower, 1], .001, .499)
    uv[lower, 1] = np.clip(uv[lower, 1], .501, .999)
    return uv


def fit_lash_roots(points, faces, uv, kinds, skin_points, skin_faces, texture, kind):
    alpha = np.asarray(Image.open(texture))[..., 3]
    active = np.flatnonzero(alpha[234 + (kind - 1) * 256] >= 46)
    groups = np.split(active, np.flatnonzero(np.diff(active) > 1) + 1)
    strands = np.array([group.mean() / 1023 for group in groups if len(group)])
    weights = np.array([[i / 48, j / 48, 1 - (i + j) / 48]
                        for i in range(49) for j in range(49 - i)])
    lash_faces = faces[np.all(kinds[faces] == kind, axis=1)]
    sample_uv = np.einsum('ki,nij->nkj', weights, uv[lash_faces]).reshape(-1, 2)
    pixels = np.clip(np.round(sample_uv * [1023, 511]).astype(int), [0, 0], [1023, 511])
    visible = alpha[pixels[:, 1], pixels[:, 0]] >= 46
    sample_uv = sample_uv[visible]
    assignment = np.abs(sample_uv[:, 0, None] - strands).argmin(axis=1)
    skin = skin_points[skin_faces]
    tree = cKDTree(skin.mean(axis=1))
    fitted = points.copy()
    records = {}
    for _ in range(3):
        samples = np.einsum('ki,nij->nkj', weights, fitted[lash_faces]).reshape(-1, 3)[visible]
        for side in [-1, 1]:
            roots, offsets, anchors = [], [], []
            for strand, u in enumerate(strands):
                selected = (assignment == strand) & (samples[:, 0] * side > 0)
                if not selected.any():
                    continue
                selected &= sample_uv[:, 1] > sample_uv[selected, 1].max() - .005
                source = samples[selected]
                _, nearest = tree.query(source, k=64)
                target = closest_point(skin[nearest].reshape(-1, 3, 3), np.repeat(source, 64, axis=0)).reshape(-1, 64, 3)
                distance = np.linalg.norm(target - source[:, None], axis=2)
                a, b = np.unravel_index(distance.argmin(), distance.shape)
                roots.append(u)
                offsets.append(target[a, b] - source[a])
                anchors.append(target[a, b].tolist())
            offsets = np.array(offsets)
            selected = (kinds == kind) & (points[:, 0] * side > 0)
            fitted[selected] += np.column_stack([np.interp(uv[selected, 0], roots, offsets[:, axis]) for axis in range(3)])
            records[side] = {'side': side, 'strand_u': roots, 'root_positions_m': anchors,
                             'offsets_m': (offsets + np.asarray(records[side]['offsets_m']) if side in records else offsets).tolist()}
    adjustment = np.linalg.norm(fitted - points, axis=1)
    assert adjustment.max() < .009
    assert np.array_equal(fitted[kinds != kind], points[kinds != kind])
    return fitted, {'method': 'Alpha-visible card roots projected onto the opaque eyelid surface',
                    'kind': 'upper' if kind == 1 else 'lower',
                    'maximum_adjustment_m': float(adjustment.max()), 'other_lash_positions_preserved': True,
                    'original_face_positions_preserved': True, 'topology_preserved': True, 'eyes': list(records.values())}


def sweep_outer_upper_lashes(points, uv, kinds, attachment):
    fitted = points.copy()
    for eye in attachment['eyes']:
        side = eye['side']
        selected = (kinds == 1) & (points[:, 0] * side > 0)
        u = uv[selected, 0]
        blend = np.clip((u - .38) / .30, 0, 1)
        angle = np.deg2rad(80) * blend * blend * (3 - 2 * blend)
        root_y = np.interp(u, eye['strand_u'], np.asarray(eye['root_positions_m'])[:, 1])
        rise = np.maximum(points[selected, 1] - root_y, 0)
        fitted[selected, 0] += side * rise * np.sin(angle)
        fitted[selected, 1] += rise * (np.cos(angle) - 1)
    return fitted, {'method': 'Mirror outward fan of upper outer lashes around the fitted eyelid roots',
                    'start_u': .38, 'full_u': .68, 'maximum_angle_degrees': 80,
                    'maximum_adjustment_m': float(np.linalg.norm(fitted - points, axis=1).max()),
                    'lower_positions_preserved': True, 'topology_and_uv_preserved': True}


def make_texture(path, width_scale=1.35, root_width_scale=1.8):
    width, height, supersampling = 1024, 256, 4
    alpha = Image.new('L', (width, height * 2))
    for row, strands in enumerate([36, 22]):
        strip = Image.new('L', (width * supersampling, height * supersampling))
        draw = ImageDraw.Draw(strip)
        rng = np.random.default_rng(20261107 + row)
        for u in np.linspace(.035, .95, strands):
            u += rng.uniform(-.004, .004)
            length = (.26 + .36 * np.sin(np.pi * u) ** .6 + .10 * u + rng.uniform(-.06, .06)
                      if row == 0 else .16 + .17 * np.sin(np.pi * u) ** .6 + rng.uniform(-.025, .025))
            sweep = .012 + .055 * u
            t = np.linspace(0, 1, 45)
            x = (u + sweep * t ** 1.5) * width * supersampling
            y = (.92 - length * (t - .12 * t * (1 - t))) * height * supersampling
            root_width = rng.uniform(2.6, 4) if row == 0 else rng.uniform(1.6, 2.4)
            for i in range(len(t) - 1):
                root_blend = np.clip(1 - t[i] / .45, 0, 1)
                root_blend = root_blend * root_blend * (3 - 2 * root_blend)
                local_scale = 1 + (root_width_scale - 1) * root_blend
                thickness = max(1, int((root_width * (1 - t[i]) ** .7 + .3) * width_scale * local_scale * supersampling))
                draw.line([(x[i], y[i]), (x[i + 1], y[i + 1])], fill=255, width=thickness)
        alpha.paste(strip.resize((width, height), Image.Resampling.LANCZOS), (0, row * height))
    image = Image.new('RGBA', (width, height * 2), (29, 18, 12, 0))
    image.putalpha(alpha)
    image.save(path)
    values = np.asarray(alpha)
    return {'size': [width, height * 2], 'upper_strands_per_eye': 36, 'lower_strands_per_eye': 22,
            'width_scale': width_scale, 'root_width_scale': root_width_scale, 'root_taper_end': .45, 'alpha_cutoff': .18,
            'opaque_fraction': float(np.mean(values >= 46)), 'transparent_fraction': float(np.mean(values == 0))}
