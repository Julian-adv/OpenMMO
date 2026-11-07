import argparse
import hashlib
import importlib.util
import io
import json
import shutil
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter
from scipy.interpolate import PchipInterpolator
from scipy.ndimage import map_coordinates
from scipy.spatial import cKDTree

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor, add_accessor
from lib.female_custom_eyelashes import select_lashes


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
WORK = OUT / 'lower_lid_ridge_v2'
BEFORE = WORK / 'before'
KNOTS = [.016, .020, .0242, .0277, .0321, .0356, .0391, .0409, .0435, .0462, .050, .055]
HEIGHT = [1.6813, 1.6813, 1.6804, 1.6804, 1.6804, 1.6813, 1.6822, 1.68305, 1.6848, 1.6857, 1.6857, 1.6857]
RIDGE = PchipInterpolator(KNOTS, HEIGHT, extrapolate=False)
UPPER_TREE = None
LANDMARKS = [[-.044327, 1.684807, .061924], [-.042568, 1.683048, .062803],
             [-.041689, 1.683048, .061924], [-.039930, 1.682169, .063682],
             [-.037292, 1.680410, .066320], [-.036413, 1.681290, .064562],
             [-.032896, 1.680410, .064562], [-.032896, 1.680410, .066320],
             [-.028499, 1.680410, .066320], [-.024982, 1.680410, .065441]]
SPEC = importlib.util.spec_from_file_location('female_fitter', ROOT / 'tools/fit-female-custom-face.py')
FIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIT)


def smooth(value):
    value = np.clip(value, 0, 1)
    return value * value * (3 - 2 * value)


def drop(points, amount):
    x, y, z = points.T
    absolute = np.abs(x + .0008)
    height = RIDGE(np.clip(absolute, KNOTS[0], KNOTS[-1]))
    offset = y - height
    ends = smooth((absolute - .016) / .005) * (1 - smooth((absolute - .042) / .007))
    core = (1 - smooth((offset - .0002) / .0004)) * (1 - smooth((-offset - .0020) / .0080))
    front = smooth((z - .040) / .012)
    upper_pin = smooth(UPPER_TREE.query(points)[0] / .001) if UPPER_TREE is not None else 1
    return amount * ends * core * front * upper_pin


def raster(polygon, width, height):
    low = np.maximum(np.floor(polygon.min(0)).astype(int), 0)
    high = np.minimum(np.ceil(polygon.max(0)).astype(int), [width - 1, height - 1])
    matrix = np.column_stack((polygon[1] - polygon[0], polygon[2] - polygon[0]))
    if np.any(high < low) or abs(np.linalg.det(matrix)) < 1e-10:
        return None
    xx, yy = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
    local = (np.column_stack((xx.ravel() + .5, yy.ravel() + .5)) - polygon[0]) @ np.linalg.inv(matrix).T
    bary = np.column_stack((1 - local.sum(1), local))
    inside = bary.min(1) >= -1e-6
    return xx.ravel()[inside], yy.ravel()[inside], bary[inside]


def projected_eye_maps(points, faces, uv, originals):
    projected = {role: np.zeros((1024, 2048, 3), dtype=np.float32) for role in originals}
    for side, offset in [(-1, 0), (1, 1024)]:
        xy = (points[:, :2] - [side * .033 - .0008, 1.681]) / [.060, -.040] * 1024 + 512
        depth = np.full((1024, 1024), -np.inf)
        for face in faces:
            if points[face, 2].max() < .045:
                continue
            sample = raster(xy[face], 1024, 1024)
            if sample is None:
                continue
            x, y, bary = sample
            z = bary @ points[face, 2]
            take = z > depth[y, x]
            x, y, z, bary = x[take], y[take], z[take], bary[take]
            for role, original in originals.items():
                height, width = original.shape[:2]
                pixels = bary @ uv[face] * [width, height] - .5
                projected[role][y, x + offset] = np.column_stack([map_coordinates(original[..., channel], pixels[:, ::-1].T, order=1, mode='wrap') for channel in range(3)])
            depth[y, x] = z
    return projected


def map_ridge_patch(attrs, moved, updated, faces, projected, originals, lash_uv, lash_faces):
    height, width = originals['albedo'].shape[:2]
    occupied = Image.new('L', (width, height))
    draw = ImageDraw.Draw(occupied)
    for coords, triangles in [(attrs['TEXCOORD_0'], faces), (lash_uv, lash_faces)]:
        for triangle in triangles:
            draw.polygon([tuple(point) for point in coords[triangle] * [width, height]], fill=255)
    occupied = np.asarray(occupied.filter(ImageFilter.MaxFilter(5))) > 0
    maps = {role: np.round(image).astype(np.uint8) for role, image in originals.items()}
    parent = list(range(len(moved)))
    secondary_uv = [point.copy() for point in attrs['TEXCOORD_0']]
    new_faces = faces.copy()
    selected = np.max(np.abs(moved[faces] - attrs['POSITION'][faces]), axis=(1, 2)) > 1e-8
    tiles = []
    for side, offset in [(-1, 0), (1, 1024)]:
        for size in [512, 448, 384, 320, 256]:
            span = size + 12
            integral = np.pad(occupied.astype(np.int32), ((1, 0), (1, 0))).cumsum(0).cumsum(1)
            xs, ys = np.arange(0, width - span, 8), np.arange(0, height - span, 8)
            sums = integral[ys[:, None] + span, xs + span] - integral[ys[:, None], xs + span] - integral[ys[:, None] + span, xs] + integral[ys[:, None], xs]
            free = np.argwhere(sums == 0)
            if len(free):
                y, x = int(ys[free[0, 0]]), int(xs[free[0, 1]])
                break
        else:
            raise ValueError('No unused atlas area for the eyelid countershift')
        occupied[y:y + span, x:x + span] = True
        x, y = x + 6, y + 6
        for role, projection in projected.items():
            tile = Image.fromarray(np.round(np.clip(projection[:, offset:offset + 1024], 0, 255)).astype(np.uint8)).resize((size, size), Image.Resampling.LANCZOS)
            maps[role][y - 4:y + size + 4, x - 4:x + size + 4] = np.pad(np.asarray(tile), ((4, 4), (4, 4), (0, 0)), mode='edge')
        active_faces = np.flatnonzero(selected & (attrs['POSITION'][faces].mean(1)[:, 0] * side > 0))
        ids = np.unique(faces[active_faces])
        remap = {}
        for old in ids:
            remap[int(old)] = len(parent)
            parent.append(int(old))
            position = moved[old]
            planar = [(position[0] - (side * .033 - .0008)) / .060 + .5, (position[1] - 1.681) / -.040 + .5]
            secondary_uv.append((np.array([x, y]) + np.asarray(planar) * size) / [width, height])
        for face in active_faces:
            new_faces[face] = [remap[int(old)] for old in faces[face]]
        tiles.append({'side': side, 'origin': [x, y], 'size': size, 'triangles': len(active_faces)})
    parent = np.asarray(parent)
    result = {key: value[parent].copy() for key, value in attrs.items()}
    result['POSITION'] = moved[parent]
    result['NORMAL'] = updated[parent]
    result['TEXCOORD_1'] = np.asarray(secondary_uv, dtype=np.float32)
    np.testing.assert_array_equal(result['POSITION'][new_faces], moved[faces])
    for key in ['TEXCOORD_0', 'JOINTS_0', 'WEIGHTS_0']:
        np.testing.assert_array_equal(result[key][new_faces], attrs[key][faces])
    for role, image in maps.items():
        Image.fromarray(image).save(WORK / f'face-{role}-ridge-fit.png')
    return result, new_faces, maps, tiles


def main():
    global UPPER_TREE
    parser = argparse.ArgumentParser()
    parser.add_argument('--drop-mm', type=float, default=2.8)
    parser.add_argument('--activate', action='store_true')
    args = parser.parse_args()
    if args.activate:
        record = json.loads((WORK / 'build.json').read_text())
        fitting = json.loads((BEFORE / 'fitting.json').read_text())
        for name in ['base_custom.glb', 'face_custom.glb', 'base_custom_preview.glb']:
            shutil.copy2(WORK / name, OUT / name)
        fitting.update(date='2026-11-08', new_head_scale_and_translation_only=False,
                       original_head_geometry_preserved=False, geometry_rig_and_animations_unchanged=False,
                       lower_eyelid_fit=record, source_head_material_preserved=False,
                       head_affine_scope='Original scale and translation except the explicitly translated lower eyelid ridge',
                       maximum_head_affine_error_m=record['drop_mm'] / 1000,
                       maximum_exported_head_affine_error_m=record['drop_mm'] / 1000,
                       status='Lower eyelid ridge translated with its curve retained; original eye textures countershifted upward on UV1 patches')
        fitting['eyelashes']['positions_normals_weights_and_triangles_preserved'] = False
        fitting['eyelashes']['weights_uv_triangles_and_color_preserved'] = True
        fitting['eyelashes']['position_and_normal_change_scope'] = 'Lower eyelid ridge fit; original upper lash positions pinned'
        fitting['files'].update({name: hashlib.sha256((OUT / name).read_bytes()).hexdigest() for name in ['base_custom.glb', 'face_custom.glb']})
        for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
            path.write_text(json.dumps(fitting, indent=2) + '\n')
        print('Activated the reviewed eyelid ridge candidate')
        return
    assert 0 < args.drop_mm <= 3.5
    amount = args.drop_mm / 1000
    BEFORE.mkdir(parents=True, exist_ok=True)
    for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json']:
        if not (BEFORE / name).exists():
            shutil.copy2(OUT / name, BEFORE / name)
    source, source_raw = read_glb(OUT / 'source.glb')
    source_part = source['meshes'][0]['primitives'][0]
    source_points = accessor(source, source_raw, source_part['attributes']['POSITION'])
    source_faces = accessor(source, source_raw, source_part['indices']).reshape(-1, 3)
    kinds = select_lashes(source_points, source_faces)[:, 0]
    baseline_fitting = json.loads((BEFORE / 'fitting.json').read_text())
    world = source_points.astype(float) * baseline_fitting['scale'] + baseline_fitting['offset_m']
    UPPER_TREE = cKDTree(world[kinds == 1])
    records, landmark_positions = {}, []
    corrected_maps, patch_attrs, patch_faces, tile_record = {}, None, None, None
    for name in ['base_custom.glb', 'face_custom.glb']:
        doc, binary = read_glb(BEFORE / name)
        raw = bytearray(binary)
        for mesh in doc['meshes']:
            for part in mesh['primitives']:
                attrs = part['attributes']
                points = accessor(doc, binary, attrs['POSITION'])
                normals = accessor(doc, binary, attrs['NORMAL'])
                delta = drop(points.astype(float), amount)
                active = delta > 1e-10
                moved = points.copy()
                moved[active, 1] -= delta[active]
                jacobian = np.tile(np.eye(3), (int(active.sum()), 1, 1))
                for axis in range(3):
                    step = np.eye(3)[axis] * 1e-5
                    jacobian[:, 1, axis] -= (drop(points[active] + step, amount) - drop(points[active] - step, amount)) / 2e-5
                assert np.all(np.linalg.det(jacobian) > .35)
                updated = normals.copy()
                transformed = np.linalg.solve(jacobian.transpose(0, 2, 1), normals[active, :, None])[..., 0]
                transformed /= np.maximum(np.linalg.norm(transformed, axis=1, keepdims=True), 1e-12)
                updated[active] = transformed
                faces = accessor(doc, binary, part['indices']).reshape(-1, 3)
                old, new = points[faces], moved[faces]
                a = np.cross(old[:, 1] - old[:, 0], old[:, 2] - old[:, 0])
                b = np.cross(new[:, 1] - new[:, 0], new[:, 2] - new[:, 0])
                valid = np.linalg.norm(a, axis=1) > 1e-12
                assert np.all(np.einsum('ij,ij->i', a[valid], b[valid]) > 0)
                np.testing.assert_array_equal(moved[~active], points[~active])
                np.testing.assert_array_equal(moved[:, [0, 2]], points[:, [0, 2]])
                for semantic, values in [('POSITION', moved), ('NORMAL', updated)]:
                    a = doc['accessors'][attrs[semantic]]
                    view = doc['bufferViews'][a['bufferView']]
                    target = np.ndarray(values.shape, dtype='<f4', buffer=raw,
                                        offset=view.get('byteOffset', 0) + a.get('byteOffset', 0),
                                        strides=(view.get('byteStride', values.shape[1] * 4), 4))
                    target[:] = values
                    a['min'], a['max'] = values.min(0).tolist(), values.max(0).tolist()
                    del target
                if mesh['name'] == 'female_body':
                    assert not active.any()
                if mesh['name'] == 'face_female_custom':
                    if name == 'base_custom.glb':
                        for point in LANDMARKS:
                            nearest = np.linalg.norm(points - point, axis=1).argmin()
                            assert np.linalg.norm(points[nearest] - point) < 1e-6
                            if abs(point[0] + .0008) <= .042:
                                assert abs(delta[nearest] - amount) < 1e-7
                            else:
                                assert delta[nearest] > amount * .5
                            landmark_positions.append({'vertex': int(nearest), 'before': points[nearest].tolist(), 'after': moved[nearest].tolist(), 'drop_mm': float(delta[nearest] * 1000)})
                    material = doc['materials'][part['material']]
                    roles = {'albedo': material['pbrMetallicRoughness']['baseColorTexture']['index'],
                             'orm': material['pbrMetallicRoughness']['metallicRoughnessTexture']['index']}
                    if patch_attrs is None:
                        originals = {}
                        for role, texture in roles.items():
                            image_id = doc['textures'][texture]['source']
                            originals[role] = np.asarray(Image.open(io.BytesIO(view_bytes(doc, binary, doc['images'][image_id]['bufferView']))).convert('RGB'), dtype=float)
                        original_attrs = {key: accessor(doc, binary, index) for key, index in attrs.items()}
                        projected = projected_eye_maps(points, faces, original_attrs['TEXCOORD_0'], originals)
                        lash_part = next(m for m in doc['meshes'] if m['name'] == 'eyelashes_female_custom')['primitives'][0]
                        lash_uv = accessor(doc, binary, lash_part['attributes']['TEXCOORD_0'])
                        lash_faces = accessor(doc, binary, lash_part['indices']).reshape(-1, 3)
                        patch_attrs, patch_faces, corrected_maps, tile_record = map_ridge_patch(original_attrs, moved, updated, faces, projected, originals, lash_uv, lash_faces)
                    types = {'POSITION': 'VEC3', 'NORMAL': 'VEC3', 'TEXCOORD_0': 'VEC2', 'TEXCOORD_1': 'VEC2', 'JOINTS_0': 'VEC4', 'WEIGHTS_0': 'VEC4'}
                    part['attributes'] = {key: add_accessor(doc, raw, values, types[key], 5123 if key == 'JOINTS_0' else 5126) for key, values in patch_attrs.items()}
                    part['indices'] = add_accessor(doc, raw, patch_faces.reshape(-1, 1), 'SCALAR', 5125)
                    material['pbrMetallicRoughness']['baseColorTexture']['texCoord'] = 1
                    material['pbrMetallicRoughness']['metallicRoughnessTexture']['texCoord'] = 1
                    if 'occlusionTexture' in material:
                        material['occlusionTexture']['texCoord'] = 1
                    for role, texture in roles.items():
                        image_id = doc['textures'][texture]['source']
                        stream = io.BytesIO()
                        Image.fromarray(corrected_maps[role]).save(stream, format='PNG')
                        raw.extend(b'\0' * (-len(raw) % 4))
                        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(stream.getvalue())})
                        doc['images'][image_id] = {'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': f'Original eye {role} countershifted on separate UV patches'}
                        raw.extend(stream.getvalue())
                if name == 'base_custom.glb':
                    records[mesh['name']] = {'moved_vertices': int(active.sum()), 'maximum_drop_mm': float(delta.max(initial=0) * 1000), 'minimum_jacobian': float(np.linalg.det(jacobian).min(initial=1))}
        write_glb(WORK / name, doc, FIT.compact(doc, raw))
    record = {'date': '2026-11-08', 'drop_mm': args.drop_mm, 'baseline_path': 'lower_lid_ridge_v2/before/base_custom.glb',
              'baseline_sha256': hashlib.sha256((BEFORE / 'base_custom.glb').read_bytes()).hexdigest(),
              'ridge_x_knots_m': KNOTS, 'ridge_y_knots_m': HEIGHT, 'meshes': records, 'ridge_landmarks': landmark_positions,
              'core_ridge_translation_preserves_curve': True, 'new_triangles': 0, 'body_and_neck_unchanged': True,
              'horizontal_and_depth_coordinates_unchanged': True, 'triangle_order_winding_uv0_weights_rig_animations_unchanged': True,
              'original_maps_countershifted_against_mesh_motion': True, 'secondary_uv_patches': tile_record,
              'source': 'Original user Tripo model, iris cleanup baseline and user red/blue lower eyelid annotation',
              'license': 'Source license not supplied; same source conditions apply. Local geometry and UV baking; no new AI generation.'}
    (WORK / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record))


if __name__ == '__main__':
    main()
