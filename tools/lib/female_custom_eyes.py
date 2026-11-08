import numpy as np
from PIL import Image, ImageDraw
from scipy.ndimage import binary_dilation, distance_transform_edt, map_coordinates

from .female_custom_eyelashes import PATCHES, LOWER_PATCHES, uv_labels


def smoothstep(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


def eye_weight(points):
    x, y, z = np.abs(points[:, 0] + .0008), points[:, 1], points[:, 2]
    knots = [.016, .020, .026, .033, .040, .047, .052]
    top = np.interp(x, knots, [1.6819, 1.6860, 1.6891, 1.6894, 1.6878, 1.6854, 1.6835])
    bottom = np.interp(x, knots, [1.6819, 1.6804, 1.6795, 1.6796, 1.6806, 1.6826, 1.6835])
    weight = smoothstep((y - bottom + .0001) / .0005) * smoothstep((top - y + .0001) / .0005)
    weight *= smoothstep((x - .016) / .001) * smoothstep((.052 - x) / .001)
    return weight * smoothstep((z - .051) / .004)


def eye_color(points, iris):
    x, y = np.abs(points[:, 0] + .0008) - .033, points[:, 1] - 1.6846
    radius = np.hypot(x, y)
    iris_uv = np.column_stack([.5 + x / .0058 * .296, .5 - y / .0058 * .296])
    coordinates = np.clip(iris_uv, 0, 1) * [iris.shape[1] - 1, iris.shape[0] - 1]
    sampled = np.column_stack([map_coordinates(iris[..., channel], coordinates[:, ::-1].T, order=1)
                               for channel in range(3)])
    white = np.tile([232., 229., 223.], (len(points), 1))
    corner = smoothstep((np.abs(x) - .010) / .007)
    white += corner[:, None] * [0, -9, -8]
    blend = 1 - smoothstep((radius - .00565) / .0003)
    blend *= smoothstep((points[:, 2] - .0635) / .0018)
    return white * (1 - blend[:, None]) + sampled * blend[:, None]


def repair_eye_textures(points, faces, uv, images, output):
    iris = np.asarray(Image.open(output / 'eye-iris-clean-v1.png').convert('RGB'), dtype=np.float32)
    width, height = images['albedo'].size
    weight = np.zeros((height, width), dtype=np.float32)
    color = np.zeros((height, width, 3), dtype=np.float32)
    labels = uv_labels(points, faces)
    selected = ~np.isin(labels[faces[:, 0]], PATCHES + LOWER_PATCHES)
    triangles = points[faces]
    selected &= (triangles[..., 1].max(1) > 1.678) & (triangles[..., 1].min(1) < 1.691)
    selected &= (triangles[..., 2].max(1) > .051)
    selected &= (np.abs(triangles[..., 0] + .0008).min(1) < .054)
    selected &= (np.abs(triangles[..., 0] + .0008).max(1) > .015)
    painted_triangles = 0
    for face in faces[selected]:
        polygon = uv[face] * [width - 1, height - 1]
        low = np.maximum(np.floor(polygon.min(0)).astype(int), 0)
        high = np.minimum(np.ceil(polygon.max(0)).astype(int), [width - 1, height - 1])
        matrix = np.column_stack([polygon[1] - polygon[0], polygon[2] - polygon[0]])
        if abs(np.linalg.det(matrix)) < 1e-9:
            continue
        xx, yy = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
        pixels = np.column_stack([xx.ravel(), yy.ravel()])
        local = (pixels - polygon[0]) @ np.linalg.inv(matrix).T
        bary = np.column_stack([1 - local.sum(1), local])
        inside = bary.min(1) >= -1e-6
        positions = bary[inside] @ points[face]
        values = eye_weight(positions)
        active = values > .001
        if not active.any():
            continue
        painted_triangles += 1
        x, y = pixels[inside][active].T
        take = values[active] > weight[y, x]
        x, y = x[take], y[take]
        weight[y, x] = values[active][take]
        color[y, x] = eye_color(positions[active][take], iris)
    mask = weight > 0
    padding = binary_dilation(mask, iterations=1) & ~mask
    nearest = distance_transform_edt(~mask, return_distances=False, return_indices=True)
    py, px = nearest[:, padding]
    color[padding] = color[py, px]
    weight[padding] = weight[py, px]
    Image.fromarray(np.round(weight * 255).astype(np.uint8)).save(output / 'eye-cleanup-mask.png')
    record = {'method': 'World-aligned hazel iris and sclera baked into existing UV islands; localized normal and ORM cleanup',
              'iris_center_abs_x_m': .033, 'iris_center_y_m': 1.6846, 'iris_radius_m': .0058,
              'iris_front_z_range_m': [.0635, .0653],
              'painted_triangles': painted_triangles, 'mask_pixels': int((weight > 0).sum()),
              'pixels_outside_mask_preserved': True, 'geometry_uv_rig_and_lashes_preserved': True}
    for role, image in images.items():
        source = np.asarray(image.convert('RGB'))
        target = source.astype(float).copy()
        if role == 'albedo':
            target = color
        elif role == 'normal':
            target[:] = [128, 128, 255]
        else:
            target[..., 0], target[..., 1], target[..., 2] = 255, 92, 0
        result = np.round(source * (1 - weight[..., None]) + target * weight[..., None]).astype(np.uint8)
        assert np.array_equal(source[weight == 0], result[weight == 0])
        record[role + '_changed_pixels'] = int(np.any(result != source, axis=2).sum())
        Image.fromarray(result).save(output / f'face-eye-{role}.png')
    return record


def remap_eye_uv(attrs, faces, images, output):
    points, uv = attrs['POSITION'], attrs['TEXCOORD_0']
    width, height = images['albedo'].size
    occupied_image = Image.new('L', (width, height))
    draw = ImageDraw.Draw(occupied_image)
    for face in faces:
        draw.polygon([tuple(p) for p in uv[face] * [width - 1, height - 1]], fill=255)
    occupied = np.asarray(occupied_image) > 0
    originals = {role: np.asarray(image.convert('RGB'), dtype=np.float32) for role, image in images.items()}
    textures = {role: np.asarray(Image.open(output / f'face-eye-{role}.png')).copy() for role in images}
    mask = np.asarray(Image.open(output / 'eye-cleanup-mask.png')).copy()
    iris = np.asarray(Image.open(output / 'eye-iris-clean-v1.png').convert('RGB'), dtype=np.float32)
    triangles = points[faces]
    absolute_x = np.abs(triangles[..., 0] + .0008)
    candidate = (absolute_x.min(1) > .014) & (absolute_x.max(1) < .054)
    candidate &= (triangles[..., 1].min(1) > 1.675) & (triangles[..., 1].max(1) < 1.6965)
    candidate &= triangles[..., 2].min(1) > .0635
    candidate &= eye_weight(triangles.mean(1)) > .05
    parents = list(range(len(points)))
    new_faces = faces.copy()
    result = {key: list(value) for key, value in attrs.items()}
    tiles = []
    for side in [-1, 1]:
        for size in [512, 448, 384, 320]:
            span = size + 12
            integral = np.pad(occupied.astype(np.int32), ((1, 0), (1, 0))).cumsum(0).cumsum(1)
            xs, ys = np.arange(0, width - span, 8), np.arange(0, height - span, 8)
            sums = (integral[ys[:, None] + span, xs + span] - integral[ys[:, None], xs + span]
                    - integral[ys[:, None] + span, xs] + integral[ys[:, None], xs])
            free = np.argwhere(sums == 0)
            if len(free):
                y, x = int(ys[free[0, 0]]), int(xs[free[0, 1]])
                break
        else:
            raise ValueError('No unused head-atlas area for the eye texture')
        occupied[y:y + span, x:x + span] = True
        x, y = x + 6, y + 6
        selected = np.flatnonzero(candidate & (triangles[..., 0].mean(1) * side > 0))
        ids = np.unique(faces[selected])
        remap = {}
        for old in ids:
            remap[int(old)] = len(parents)
            parents.append(int(old))
            for key, values in result.items():
                values.append(attrs[key][old].copy())
            projected = [(abs(points[old, 0] + .0008) - .012) / .044, (1.6965 - points[old, 1]) / .0215]
            result['TEXCOORD_0'][-1] = (np.array([x, y]) + np.asarray(projected) * (size - 1)) / [width - 1, height - 1]
        for triangle_id in selected:
            face = faces[triangle_id]
            new_face = [remap[int(i)] for i in face]
            new_faces[triangle_id] = new_face
            polygon = np.asarray([result['TEXCOORD_0'][i] for i in new_face]) * [width - 1, height - 1]
            low, high = np.floor(polygon.min(0)).astype(int), np.ceil(polygon.max(0)).astype(int)
            matrix = np.column_stack([polygon[1] - polygon[0], polygon[2] - polygon[0]])
            if abs(np.linalg.det(matrix)) < 1e-9:
                continue
            xx, yy = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
            pixels = np.column_stack([xx.ravel(), yy.ravel()])
            local = (pixels - polygon[0]) @ np.linalg.inv(matrix).T
            bary = np.column_stack([1 - local.sum(1), local])
            inside = bary.min(1) >= -1e-6
            bary, pixels = bary[inside], pixels[inside]
            world = bary @ points[face]
            source_uv = bary @ uv[face]
            coordinates = (source_uv * [width - 1, height - 1])[:, ::-1].T
            weight = eye_weight(world)[:, None]
            px, py = pixels.T
            for role in textures:
                old = np.column_stack([map_coordinates(originals[role][..., c], coordinates, order=1)
                                       for c in range(3)])
                target = eye_color(world, iris) if role == 'albedo' else np.tile([128, 128, 255] if role == 'normal' else [255, 92, 0], (len(world), 1))
                textures[role][py, px] = np.round(old * (1 - weight) + target * weight).astype(np.uint8)
            mask[py, px] = 255
        tile_mask = mask[y:y + size, x:x + size] > 0
        nearest = distance_transform_edt(~tile_mask, return_distances=False, return_indices=True)
        padding = binary_dilation(tile_mask, iterations=2) & ~tile_mask
        for role in textures:
            region = textures[role][y:y + size, x:x + size]
            region[padding] = region[nearest[0][padding], nearest[1][padding]]
        mask[y:y + size, x:x + size][padding] = 255
        tiles.append({'side': side, 'pixel_origin': [x, y], 'size': size, 'triangles': len(selected), 'vertices': len(ids)})
    for role, texture in textures.items():
        Image.fromarray(texture).save(output / f'face-eye-{role}.png')
    Image.fromarray(mask).save(output / 'eye-cleanup-mask.png')
    return {key: np.asarray(values) for key, values in result.items()}, new_faces, np.asarray(parents), tiles
