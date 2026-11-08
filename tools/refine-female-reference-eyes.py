import hashlib
import importlib.util
import io
import json
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.interpolate import PchipInterpolator
from scipy.ndimage import map_coordinates

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'reference_v2'
DEFAULT = {
    'iris_radius': .0060, 'iris_center_x': .0330, 'iris_center_y': 1.6859,
    'pupil_fraction': .35, 'iris_saturation': .82, 'iris_gain': 1.0,
    'sclera': [181, 177, 164], 'skin': [177, 143, 111],
    'bulge': .0028, 'upper_length': .0038, 'lower_length': .0020,
    'upper_count': 84, 'lower_count': 48, 'upper_band': .00065, 'lower_band': .00013,
    'upper_lash_rgb': [27, 21, 15], 'lower_lash_rgb': [79, 55, 34],
    'face_normal_scale': .22, 'skin_roughness_floor': 185,
}
STYLE = {**DEFAULT, **json.loads((WORK / 'style.json').read_text())} if (WORK / 'style.json').exists() else DEFAULT
KNOTS = np.array([.0182, .0220, .028, .034, .0365, .041, .048, .0522])
EYE_INNER, EYE_WIDTH = KNOTS[0], KNOTS[-1] - KNOTS[0]
TOP = PchipInterpolator(KNOTS, [1.68125, 1.68549, 1.68910, 1.69148, 1.69170, 1.6910, 1.68817, 1.68570])
BOTTOM = PchipInterpolator(KNOTS, [1.68125, 1.68060, 1.67985, 1.67995, 1.68005, 1.68070, 1.68260, 1.68570])
TOP_Z = PchipInterpolator(KNOTS, [.0650, .0685, .0700, .0690, .0676, .0650, .0595, .0550])
BOTTOM_Z = PchipInterpolator(KNOTS, [.0645, .0670, .0680, .0675, .0659, .0630, .0575, .0550])


def smooth(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


def unit(value):
    length = np.linalg.norm(value, axis=-1, keepdims=True)
    return np.where(length > 1e-12, value / np.maximum(length, 1e-12), [0., 0., 1.])


def eye_offset(points):
    x, y, z = np.moveaxis(np.asarray(points), -1, 0)
    weight = smooth((np.abs(x + .0008) - .006) / .008)
    weight *= 1 - smooth((np.abs(x + .0008) - .057) / .016)
    weight *= smooth((y - 1.658) / .018) * (1 - smooth((y - 1.700) / .025))
    weight *= smooth((z - .030) / .020)
    return STYLE.get('eye_region_y_offset', 0) * weight


def eye_displacement(points):
    points = np.asarray(points)
    vertical = eye_offset(points)
    x = np.abs(points[..., 0] + .0008)
    weight = vertical / STYLE['eye_region_y_offset'] if STYLE.get('eye_region_y_offset') else np.zeros_like(vertical)
    if STYLE.get('eye_socket_shape') == 'ellipsoid':
        sides = np.where(points[..., 0] + .0008 < 0, -1, 1)
        u = eye_parameter(x, sides)
        canonical_x = EYE_INNER + EYE_WIDTH * np.clip(u, .0001, .9999)
        bottom, top = BOTTOM(canonical_x), TOP(canonical_x)
        v = (points[..., 1] - bottom) / np.maximum(top - bottom, 1e-9)
        nearest = surface(np.clip(u, .0001, .9999), np.clip(v, 0, 1), sides)
        eye_x_local = np.abs(nearest[..., 0] + .0008)
        radius = ((eye_x_local - .034) / .023) ** 2 + ((nearest[..., 1] - 1.6865) / .016) ** 2
        target_z = .0545 + .009 * np.sqrt(np.maximum(1 - radius, .002))
        target_z += .00035 * np.exp(-((eye_x_local - STYLE['iris_center_x'] - sides * STYLE.get('gaze_x', 0)) / .006) ** 2
                                    - ((nearest[..., 1] - STYLE['iris_center_y']) / .006) ** 2)
        distance_y = np.maximum(np.maximum(bottom - points[..., 1], points[..., 1] - top), 0)
        distance_x = np.abs(x - np.abs(nearest[..., 0] + .0008))
        feather = (1 - smooth(distance_y / .012)) * (1 - smooth(distance_x / .010))
        depth = (target_z - nearest[..., 2]) * feather * weight
    else:
        depth = STYLE.get('eye_socket_offset', 0) * weight
        depth *= np.exp(-((x - .0332) / .015) ** 2 - ((points[..., 1] - 1.686) / .010) ** 2)
    return np.stack([np.zeros_like(vertical), vertical, depth], axis=-1)


def warp_eye_region(attrs, rigid_strands=False):
    points = attrs['POSITION'].copy()
    if rigid_strands:
        strands = points.reshape(-1, 10, 3)
        roots = (strands[:, 0] + strands[:, 5]) / 2
        return {**attrs, 'POSITION': points + np.repeat(eye_displacement(roots), 10, axis=0)}
    offset = eye_displacement(points)
    selected = np.linalg.norm(offset, axis=-1) > 1e-10
    jacobian = np.tile(np.eye(3), (int(selected.sum()), 1, 1))
    for axis in range(3):
        delta = np.eye(3)[axis] * 1e-5
        jacobian[:, :, axis] += (eye_displacement(points[selected] + delta) - eye_displacement(points[selected] - delta)) / 2e-5
    normals = attrs['NORMAL'].copy()
    normals[selected] = unit(np.linalg.solve(jacobian.transpose(0, 2, 1), normals[selected, :, None])[..., 0])
    points += offset
    return {**attrs, 'POSITION': points, 'NORMAL': normals}


def eye_x(u, side):
    shift = np.where(np.asarray(side) > 0,
                     STYLE.get('right_inner_x_shift', 0) * (1 - u) ** 3 + STYLE.get('right_outer_x_shift', 0) * u ** 3, 0)
    return EYE_INNER + EYE_WIDTH * u + shift


def eye_parameter(x, side):
    grid = np.linspace(0, 1, 257)
    result = np.empty_like(x, dtype=float)
    for direction in [-1, 1]:
        selected = np.asarray(side) == direction
        knots = eye_x(grid, direction)
        values = np.interp(x[selected], knots, grid)
        values = np.where(x[selected] < knots[0], (x[selected] - knots[0]) / (knots[1] - knots[0]) / 256, values)
        values = np.where(x[selected] > knots[-1], 1 + (x[selected] - knots[-1]) / (knots[-1] - knots[-2]) / 256, values)
        result[selected] = values
    return result


def surface(u, v, side, optical=False):
    x = EYE_INNER + EYE_WIDTH * u
    y = BOTTOM(x) * (1 - v) + TOP(x) * v
    z = BOTTOM_Z(x) * (1 - v) + TOP_Z(x) * v
    z += (0 if optical else STYLE.get('eye_depth_offset', 0)) * np.sin(np.pi * u) ** .5
    bulge = STYLE.get('optical_bulge', STYLE['bulge']) if optical else STYLE['bulge']
    cornea = STYLE.get('optical_cornea_bulge', STYLE.get('cornea_bulge', .0014)) if optical else STYLE.get('cornea_bulge', .0014)
    z += bulge * np.sin(np.pi * v) * np.sin(np.pi * u) ** .5
    z += cornea * np.sin(np.pi * v) ** .8 * np.exp(-((x - STYLE['iris_center_x'] - side * STYLE.get('gaze_x', 0)) / .006) ** 2 - ((y - STYLE['iris_center_y']) / .006) ** 2)
    if not optical:
        anchor = STYLE.get('eye_depth_anchor', .055)
        z = anchor + (z - anchor) * STYLE.get('eye_depth_scale', 1)
    return np.stack([side * eye_x(u, side) - .0008, y, z], axis=-1)


def recess_original_eye_surface(attrs):
    points = attrs['POSITION'].copy()
    x = np.abs(points[:, 0] + .0008)
    sides = np.where(points[:, 0] + .0008 < 0, -1, 1)
    inner_x = eye_x(0, sides)
    inner = (x > .0148) & (x < inner_x) & (points[:, 1] > 1.6803) & (points[:, 1] < 1.6855) & (points[:, 2] > .060)
    blend = smooth((inner_x[inner] - x[inner]) / .0015)
    side = sides[inner]
    target = surface(np.zeros(len(side)), np.zeros(len(side)), side)
    target[:, 2] -= .001
    points[inner] = points[inner] * (1 - blend[:, None]) + target * blend[:, None]
    x = np.abs(points[:, 0] + .0008)
    u = eye_parameter(x, sides)
    canonical_x = EYE_INNER + EYE_WIDTH * u
    bottom, top = BOTTOM(canonical_x), TOP(canonical_x)
    v = (points[:, 1] - bottom) / np.maximum(top - bottom, 1e-9)
    cap = surface(np.clip(u, .0001, .9999), np.clip(v, 0, 1), sides)
    margin = .0030 * np.sin(np.clip(u, 0, 1) * np.pi) ** .3
    selected = (u > 0) & (u < 1) & (points[:, 1] > bottom - margin) & (points[:, 1] < top + margin)
    selected &= points[:, 2] > cap[:, 2] - .0008
    distance = np.maximum(bottom - points[:, 1], points[:, 1] - top)
    blend = 1 - smooth((distance - .0020) / np.maximum(margin - .0020, 1e-9))
    points[selected, 2] += (cap[selected, 2] - .0008 - points[selected, 2]) * blend[selected]
    return {**attrs, 'POSITION': points}


def skin_texture(size):
    u, v = np.meshgrid(np.linspace(0, 1, size), np.linspace(0, 1, size))
    y = 1.698 - .026 * v
    crease = np.exp(-((y - 1.6916) / .0015) ** 2) * np.sin(np.pi * u) ** .4
    lower = np.exp(-((y - 1.679) / .0012) ** 2) * .12
    shade = 1 - .18 * crease - lower
    return np.round(np.array(STYLE['skin'])[None, None] * shade[..., None]).astype(np.uint8)


def iris_source():
    path = WORK / 'iris-hazel.png'
    iris = np.asarray(Image.open(path).convert('RGB'), dtype=float)
    yy, xx = np.nonzero(iris.mean(2) < 175)
    center = [(xx.min() + xx.max()) / 2, (yy.min() + yy.max()) / 2]
    radius = (xx.max() - xx.min() + yy.max() - yy.min()) / 4
    return iris, np.asarray(center), radius


def cap_texture(width, height, side):
    u, v = np.meshgrid(np.linspace(.0001, .9999, width), np.linspace(1, 0, height))
    world = surface(u, v, side)
    dx = np.abs(world[..., 0] + .0008) - STYLE['iris_center_x'] - side * STYLE.get('gaze_x', 0)
    dy = world[..., 1] - STYLE['iris_center_y']
    r = np.hypot(dx, dy) / STYLE['iris_radius']
    sample_dy = dy - STYLE.get('pupil_y_offset', 0) * (1 - smooth((r - .30) / .65))
    sample_radius = np.hypot(dx, sample_dy) / STYLE['iris_radius']
    sample_r = np.interp(sample_radius, [0, STYLE['pupil_fraction'], 1, 3], [0, .30, 1, 3])
    scale = sample_r / np.maximum(sample_radius, 1e-9)
    iris, center, radius = iris_source()
    xx = np.clip(center[0] + dx / STYLE['iris_radius'] * scale * radius, 0, iris.shape[1] - 1)
    yy = np.clip(center[1] - sample_dy / STYLE['iris_radius'] * scale * radius, 0, iris.shape[0] - 1)
    color = np.stack([map_coordinates(iris[..., c], [yy.ravel(), xx.ravel()], order=1).reshape(u.shape)
                      for c in range(3)], axis=-1)
    gray = color.mean(2, keepdims=True)
    color = (gray + (color - gray) * STYLE['iris_saturation']) * STYLE['iris_gain']
    ring = smooth((r - .90) / .10) * .42
    color = color * (1 - ring[..., None]) + np.array([51, 42, 28]) * ring[..., None]
    blend = 1 - smooth((r - .975) / .04)
    sclera = np.tile(STYLE['sclera'], (*u.shape, 1)).astype(float)
    corners = (1 - np.sin(np.pi * u) ** .35)
    sclera += corners[..., None] * [1, -10, -9]
    albedo = sclera * (1 - blend[..., None]) + color * blend[..., None]
    shadow = 1 - .30 * smooth((v - .65) / .35)
    albedo *= shadow[..., None]
    orm = np.zeros_like(albedo)
    orm[..., 0] = 255
    orm[..., 1] = 113 * (1 - blend) + STYLE.get('iris_roughness_byte', 42) * blend
    return np.round(np.clip(albedo, 0, 255)).astype(np.uint8), np.round(orm).astype(np.uint8)


def lashes_texture():
    pixels = np.empty((64, 64, 4), dtype=np.uint8)
    for row, name in enumerate(['upper_lash_rgb', 'lower_lash_rgb']):
        base = np.asarray(STYLE[name], dtype=float)
        for y, t in enumerate(np.linspace(0, 1, 64)):
            color = base * (1 + .30 * t)
            fade = .90 if row == 0 else .65
            alpha = 255 * (1 - fade * smooth((t - .45) / .55))
            pixels[y, row * 32:(row + 1) * 32] = [*np.round(color).astype(int), int(alpha)]
    image = Image.fromarray(pixels)
    image.save(WORK / 'eyelashes-natural.png')
    return image


def mesh_attrs(points, normals, uv, template, uv1=None):
    count = len(points)
    attrs = {'POSITION': np.asarray(points), 'NORMAL': unit(np.asarray(normals)), 'TEXCOORD_0': np.asarray(uv),
             'JOINTS_0': np.tile(template['JOINTS_0'][0], (count, 1)),
             'WEIGHTS_0': np.tile(template['WEIGHTS_0'][0], (count, 1))}
    if uv1 is not None:
        attrs['TEXCOORD_1'] = np.asarray(uv1)
    return attrs


def grid_mesh(points, side):
    nu, nv = points.shape[:2]
    du, dv = np.gradient(points, axis=(0, 1))
    normals = unit(np.cross(du, dv) * side)
    faces = []
    for i in range(nu - 1):
        for j in range(nv - 1):
            a = i * nv + j
            faces.extend([[a, a + nv, a + 1], [a + nv, a + nv + 1, a + 1]])
    faces = np.asarray(faces)
    if side < 0:
        faces = faces[:, ::-1]
    return normals.reshape(-1, 3), faces


def combine(parts):
    attrs = {key: [] for key in parts[0][0]}
    faces, offset = [], 0
    for item, triangles in parts:
        for key in attrs:
            attrs[key].append(item[key])
        faces.append(triangles + offset)
        offset += len(item['POSITION'])
    return {key: np.concatenate(values) for key, values in attrs.items()}, np.concatenate(faces)


def skin_depth(point, skin):
    triangles, inverse = skin
    local = np.einsum('nij,nj->ni', inverse, point[:2] - triangles[:, 0, :2])
    bary = np.column_stack([1 - local.sum(1), local])
    inside = bary.min(1) >= 0
    return float(np.einsum('ni,ni->n', bary[inside], triangles[inside, :, 2]).max()) if inside.any() else None


def cap_mesh(side, tile, template):
    u, v = np.meshgrid(np.linspace(.0001, .9999, 49), np.linspace(0, 1, 9), indexing='ij')
    points = surface(u, v, side)
    normals, faces = grid_mesh(surface(u, v, side, optical=True), side)
    radius = np.hypot(np.abs(points[..., 0] + .0008) - STYLE['iris_center_x'] - side * STYLE.get('gaze_x', 0),
                      points[..., 1] - STYLE['iris_center_y']) / STYLE['iris_radius']
    flatten = STYLE.get('sclera_normal_flatten', .32) * smooth((radius.ravel() - .92) / .30)
    normals = unit(normals * (1 - flatten[:, None]) + [0, 0, 1] * flatten[:, None])
    origin = np.array(tile['pixel_origin']) + [4, 4]
    uv = (origin + .5 + np.stack([u, 1 - v], axis=-1) * [367, 495]) / [2048, 2576]
    return mesh_attrs(points.reshape(-1, 3), normals, uv.reshape(-1, 2), template), faces


def lower_lid_mesh(side, tile, template, skin):
    u, v = np.meshgrid(np.linspace(.0001, .9999, 49), np.linspace(0, 1, 2), indexing='ij')
    points = surface(u, np.zeros_like(u), side)
    points[..., 1] -= v * .00075 * np.sin(np.pi * u) ** .3
    for point, row in zip(points.reshape(-1, 3), v.ravel()):
        if row:
            depth = skin_depth(point, skin)
            if depth is not None:
                point[2] = depth + .00005
    normals, faces = grid_mesh(points, -side)
    projected = np.stack([(np.abs(points[..., 0] + .0008) - .013) / .043,
                          (1.698 - points[..., 1]) / .026], axis=-1)
    uv = (np.array(tile['pixel_origin']) + [384.5, 8.5] + np.clip(projected, 0, 1) * 123) / [2048, 2576]
    return mesh_attrs(points.reshape(-1, 3), normals, uv.reshape(-1, 2), template), faces


def liner_mesh(side, row, template, skin):
    u, v = np.meshgrid(np.linspace(.001, 1.22 if row == 0 else .999, 61 if row == 0 else 49),
                       np.linspace(0, 1, 4), indexing='ij')
    points = surface(np.minimum(u, .9999), np.full_like(u, 1 if row == 0 else 0), side)
    width = STYLE['upper_band' if row == 0 else 'lower_band']
    if row == 0:
        extension = np.maximum(u - 1, 0)
        points[..., 0] += side * STYLE.get('upper_wing_length', .00814) * extension / .22
        points[..., 1] += STYLE.get('upper_wing_rise', .0040) * extension / .22
        width = width * (.30 + .70 * np.sin(np.minimum(u, 1) * np.pi / 2) ** .7)
        width = width * (1 - smooth(extension / .22))
    points[..., 1] += (v if row == 0 else v - .20) * width * (1 if row == 0 else -1)
    points[..., 2] += .00005
    for point, parameter in zip(points.reshape(-1, 3), u.ravel()):
        depth = skin_depth(point, skin)
        if depth is not None:
            depth += .00045
            point[2] = max(point[2], depth) if parameter <= 1 else point[2] * (1 - smooth((parameter - 1) / .025)) + depth * smooth((parameter - 1) / .025)
    uv = np.zeros((*u.shape, 2)) + ([.25, .01] if row == 0 else [.75, .01])
    if row == 0:
        uv[..., 1] = v * (1 - smooth((u - .94) / .06))
        uv[..., 1] += .75 * smooth((u - 1) / .22)
    normals, faces = grid_mesh(points, side * (1 if row == 0 else -1))
    return mesh_attrs(points.reshape(-1, 3), normals, uv.reshape(-1, 2), template, uv.reshape(-1, 2)), faces


def strand_mesh(side, row, template, skin):
    rng = np.random.default_rng(20261109 + row)
    parts = []
    count = STYLE['upper_count' if row == 0 else 'lower_count']
    for strand, root_u in enumerate(np.linspace(.018, .982, count)):
        if row == 0:
            root_u = .018 + ((strand // 3) * 3 + 1) * (.964 / (count - 1)) + ((strand % 3) - 1) * .0045
        root_u += rng.uniform(-.002, .002)
        t = np.linspace(0, 1, 5)
        u = np.full_like(t, root_u)
        center = surface(u, np.full_like(t, 1 if row == 0 else 0), side)
        if row == 0:
            width = STYLE['upper_band'] * (.30 + .70 * np.sin(root_u * np.pi / 2) ** .7)
            center[:, 1] += .8 * width
            depth = skin_depth(center[0], skin)
            if depth is not None:
                center[:, 2] = max(center[0, 2], depth + .0004)
        if row == 1:
            center[:, 1] -= .00075 * np.sin(np.pi * root_u) ** .3
            depth = skin_depth(center[0], skin)
            if depth is not None:
                center[:, 2] = depth + .00010
        length = STYLE['upper_length' if row == 0 else 'lower_length']
        fraction = (.34 + .47 * np.sin(np.pi * root_u) ** .6 + .10 * root_u if row == 0
                    else .30 + .28 * np.sin(np.pi * root_u) ** .6)
        fraction *= rng.uniform(.86, 1.10)
        fan = (.0002 + .0036 * root_u ** 2 + (((strand % 3) - 1) * .00040 if row == 0 else 0))
        center[:, 0] += side * fan * t ** 1.4 * (1 if row == 0 else .28)
        center[:, 1] += (1 if row == 0 else -1) * length * fraction * t ** (1.4 if row == 0 else 1.15)
        center[:, 2] += (.0065 if row == 0 else .0025) * np.sin(t * np.pi / 2) ** .65 + .00005
        base = rng.uniform(.00017, .00027) if row == 0 else rng.uniform(.00010, .00016)
        base *= STYLE.get('upper_width_scale' if row == 0 else 'lower_width_scale', 1)
        half_width = (base * (1 - t) ** .85 + .000006) * .5
        points = np.stack([center.copy(), center.copy()])
        points[0, :, 0] -= half_width
        points[1, :, 0] += half_width
        normals, faces = grid_mesh(points, 1 if row == 0 else -1)
        uv = np.tile(np.column_stack([np.full_like(t, .25 if row == 0 else .75), t]), (2, 1))
        parts.append((mesh_attrs(points.reshape(-1, 3), normals, uv, template, uv), faces))
    return combine(parts)


def load_attrs(doc, raw, mesh):
    primitive = doc['meshes'][mesh]['primitives'][0]
    attrs = {key: accessor(doc, raw, value) for key, value in primitive['attributes'].items()}
    return attrs, accessor(doc, raw, primitive['indices']).reshape(-1, 3)


def update_image(doc, raw, texture, image, name):
    buffer = io.BytesIO()
    image.save(buffer, format='PNG')
    raw.extend(b'\0' * (-len(raw) % 4))
    view = len(doc['bufferViews'])
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(buffer.getvalue())})
    raw.extend(buffer.getvalue())
    doc['images'][doc['textures'][texture]['source']] = {'bufferView': view, 'mimeType': 'image/png', 'name': name}


def save_fitting(record):
    fitting = json.loads((WORK / 'before-reference-fitting.json').read_text())
    for key in ['eyelashes', 'eye_texture_repair', 'eye_corner_material_repair', 'files']:
        fitting.pop(key, None)
    fitting['head_affine_scope'] = 'Original face, scalp and neck outside the local eye region; added eyes and eyelashes excluded'
    fitting['head_triangles'] = record['face_triangles'] + record['lash_triangles']
    fitting['assembled_triangles'] = fitting['retained_body_triangles'] + fitting['head_triangles']
    fitting['reference_eye_rebuild'] = record
    fitting['eyelashes'] = {
        'method': 'Tapered individual ribbons and fitted root liner; source cards replaced',
        'triangles': record['lash_triangles'], 'source_cards_replaced': 508,
        'upper_strands_per_eye': STYLE['upper_count'], 'lower_strands_per_eye': STYLE['lower_count'],
        'alpha_mode': 'BLEND', 'size': [64, 64], 'topology_preserved': False,
    }
    fitting['source_color_pixels_outside_eye_mask_preserved'] = True
    fitting['original_face_uv_pixel_addresses_preserved'] = True
    fitting['face_atlas_size'] = [2048, 2576]
    paths = ['base_custom.glb', 'face_custom.glb', 'body-neck-albedo.png']
    paths += ['reference_v2/' + name for name in ['iris-hazel.png', 'style.json',
              'face-reference-albedo.png', 'face-reference-normal.png', 'face-reference-orm.png', 'eyelashes-natural.png']]
    fitting['files'] = {name: hashlib.sha256((OUT / name).read_bytes()).hexdigest() for name in paths}
    fitting['status'] = 'Reference v2; see geometry, compression and animation validation reports'
    payload = json.dumps(fitting, indent=2) + '\n'
    (OUT / 'fitting.json').write_text(payload)
    (ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json').write_text(payload)


def main():
    WORK.mkdir(exist_ok=True)
    doc, original_raw = read_glb(WORK / 'before-reference.glb')
    raw = bytearray(original_raw)
    face, faces = load_attrs(doc, original_raw, 1)
    lash, _ = load_attrs(doc, original_raw, 2)
    original_face = np.load(WORK / 'original-face.npz')
    face = {key: original_face[key] for key in original_face.files if key != 'faces'}
    face = recess_original_eye_surface(face)
    faces = original_face['faces']
    tiles = [{'side': -1, 'pixel_origin': [8, 2056], 'size': 512}, {'side': 1, 'pixel_origin': [536, 2056], 'size': 512}]
    material = doc['materials'][doc['meshes'][1]['primitives'][0]['material']]
    refs = {'albedo': material['pbrMetallicRoughness']['baseColorTexture']['index'],
            'normal': material['normalTexture']['index'], 'orm': material['pbrMetallicRoughness']['metallicRoughnessTexture']['index']}
    images = {}
    source_doc, source_raw = read_glb(OUT / 'source.glb')
    original_color = source_doc['materials'][0]['pbrMetallicRoughness']['baseColorTexture']['index']
    entry = source_doc['images'][source_doc['textures'][original_color]['source']]
    images['albedo'] = np.asarray(Image.open(io.BytesIO(view_bytes(source_doc, source_raw, entry['bufferView']))).convert('RGB')).copy()
    for role in ['normal', 'orm']:
        images[role] = np.asarray(Image.open(OUT / f'face-corner-{role}.png').convert('RGB')).copy()
    images['orm'][..., 1] = np.maximum(images['orm'][..., 1], STYLE['skin_roughness_floor'])
    for role, image in images.items():
        extended = np.empty((2576, 2048, 3), dtype=np.uint8)
        extended[:2048] = image
        extended[2048:] = image[-1]
        images[role] = extended
    material['normalTexture']['scale'] = STYLE['face_normal_scale']
    face['TEXCOORD_0'][:, 1] *= 2048 / 2576
    original_count = len(face['POSITION'])
    triangles = face['POSITION'][faces]
    matrices = np.stack([triangles[:, 1, :2] - triangles[:, 0, :2], triangles[:, 2, :2] - triangles[:, 0, :2]], axis=-1)
    valid = np.abs(np.linalg.det(matrices)) > 1e-12
    skin_surface = triangles[valid], np.linalg.inv(matrices[valid])
    cap_parts, lash_parts = [], []
    for tile in tiles:
        side = tile['side']
        px, py = tile['pixel_origin']
        skin = skin_texture(124)
        images['albedo'][py + 8:py + 132, px + 384:px + 508] = skin
        images['normal'][py + 8:py + 132, px + 384:px + 508] = [128, 128, 255]
        images['orm'][py + 8:py + 132, px + 384:px + 508] = [255, 205, 0]
        albedo, orm = cap_texture(368, 496, side)
        images['albedo'][py + 4:py + 500, px + 4:px + 372] = albedo
        images['normal'][py + 4:py + 500, px + 4:px + 372] = [128, 128, 255]
        images['orm'][py + 4:py + 500, px + 4:px + 372] = orm
        cap_parts.extend([cap_mesh(side, tile, lash), lower_lid_mesh(side, tile, lash, skin_surface)])
        for row in [0, 1]:
            lash_parts.extend([strand_mesh(side, row, lash, skin_surface), liner_mesh(side, row, lash, skin_surface)])
    face, faces = combine([(face, faces), *cap_parts])
    lash, lash_faces = combine([(warp_eye_region(attrs, rigid_strands=index % 2 == 0), triangles)
                               for index, (attrs, triangles) in enumerate(lash_parts)])
    optical_normals = face['NORMAL'].copy()
    face = warp_eye_region(face)
    offset = original_count
    for index, (attrs, _) in enumerate(cap_parts):
        count = len(attrs['POSITION'])
        if index % 2 == 0:
            face['NORMAL'][offset:offset + count] = optical_normals[offset:offset + count]
        offset += count
    for role, image in images.items():
        image = Image.fromarray(image)
        image.save(WORK / f'face-reference-{role}.png')
        update_image(doc, raw, refs[role], image, f'Female reference eyes {role}')
    lash_image = lashes_texture()
    lash_material = doc['materials'][doc['meshes'][2]['primitives'][0]['material']]
    update_image(doc, raw, lash_material['pbrMetallicRoughness']['baseColorTexture']['index'], lash_image, 'Female natural reference lashes')
    lash_material['alphaMode'] = 'BLEND'
    lash_material.pop('alphaCutoff', None)
    for mesh, attrs, triangles in [(1, face, faces), (2, lash, lash_faces)]:
        primitive = doc['meshes'][mesh]['primitives'][0]
        kinds = {'POSITION': 'VEC3', 'NORMAL': 'VEC3', 'TEXCOORD_0': 'VEC2', 'TEXCOORD_1': 'VEC2', 'JOINTS_0': 'VEC4', 'WEIGHTS_0': 'VEC4'}
        primitive['attributes'] = {key: add_accessor(doc, raw, value, kinds[key], 5123 if key == 'JOINTS_0' else 5126) for key, value in attrs.items()}
        primitive['indices'] = add_accessor(doc, raw, triangles.reshape(-1, 1), 'SCALAR', 5125)
    spec = importlib.util.spec_from_file_location('female_face_fitter', ROOT / 'tools/fit-female-custom-face.py')
    fitter = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fitter)
    write_glb(OUT / 'base_custom.glb', doc, fitter.compact(doc, raw))
    face_doc, face_raw = read_glb(OUT / 'base_custom.glb')
    face_doc.pop('animations', None)
    face_doc['nodes'][0].pop('mesh')
    face_doc['nodes'][0].pop('skin')
    face_doc['meshes'] = face_doc['meshes'][1:]
    for node in face_doc['nodes']:
        if 'mesh' in node:
            node['mesh'] -= 1
    write_glb(OUT / 'face_custom.glb', face_doc, fitter.compact(face_doc, face_raw))
    record = {'style': STYLE, 'source': 'before-reference.glb', 'reference': 'target.png',
              'original_face_vertices': original_count, 'eye_caps_triangles': sum(len(f) for _, f in cap_parts),
              'lash_triangles': len(lash_faces), 'face_triangles': len(faces), 'meshes': 3,
              'body_and_neck_geometry_preserved': True, 'original_head_positions_preserved': False,
              'original_head_positions_preserved_outside_eye_region': True,
              'eye_region_y_offset_m': STYLE.get('eye_region_y_offset', 0),
              'eye_contour_knots_m': KNOTS.tolist(), 'upper_contour_y_m_before_offset': TOP(KNOTS).tolist(),
              'eye_contour_x_by_side_m': {str(side): eye_x((KNOTS - EYE_INNER) / EYE_WIDTH, side).tolist() for side in [-1, 1]},
              'lower_contour_y_m_before_offset': BOTTOM(KNOTS).tolist(),
              'sha256': hashlib.sha256((OUT / 'base_custom.glb').read_bytes()).hexdigest()}
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    save_fitting(record)
    print(json.dumps(record))


if __name__ == '__main__':
    main()
