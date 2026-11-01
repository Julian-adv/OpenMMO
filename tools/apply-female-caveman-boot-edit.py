import hashlib
import json
import shutil
from pathlib import Path

import numpy as np

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_caveman_boots'
SOURCE = DIRECTORY / 'tripo_v1/fitted_v3'
OUTPUT = DIRECTORY / 'tripo_v1/fitted_v4'
EDIT = DIRECTORY / 'manual_edit_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUTPUT.mkdir(exist_ok=True)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def to_gltf(points):
    return np.column_stack((points[:, 0], points[:, 2], -points[:, 1]))


def rotate_normals(old, new, faces, normals):
    _, mapping = np.unique(np.round(old, 6), axis=0, return_inverse=True)
    count = mapping.max()+1
    def surface_normals(points):
        tri = points[faces]
        face = np.cross(tri[:, 1]-tri[:, 0], tri[:, 2]-tri[:, 0])
        total = np.zeros((count, 3), dtype=np.float64)
        for corner in range(3):
            np.add.at(total, mapping[faces[:, corner]], face)
        length = np.linalg.norm(total, axis=1)
        return total/np.maximum(length[:, None], 1e-15), length
    before, before_len = surface_normals(old)
    after, after_len = surface_normals(new)
    before, after = before[mapping], after[mapping]
    usable = (before_len[mapping] > 1e-12) & (after_len[mapping] > 1e-12)
    axis = np.cross(before, after)
    cosine = np.einsum('ij,ij->i', before, after)
    assert np.all(cosine[usable] > -.99), 'Surface normals reversed'
    axis[~usable] = 0
    result = normals+np.cross(axis, normals)+np.cross(axis, np.cross(axis, normals))/np.maximum(1+cosine[:, None], 1e-6)
    return result/np.maximum(np.linalg.norm(result, axis=1)[:, None], 1e-15)


doc, raw = read_glb(SOURCE / 'boots_female_caveman.glb')
base, body_raw = read_glb(BODY)
matrices, _ = heel_pose(base, body_raw)
edits = np.load(EDIT / 'mesh-edit.npz')
blender_old = [edits['original_right'], edits['original_left']]
blender_new_right = edits['edited_right']
primitives = [mesh['primitives'][0] for mesh in doc['meshes']]
original = [accessor(doc, raw, p['attributes']['POSITION']).astype(np.float64) for p in primitives]
for old, blender in zip(original, blender_old):
    assert np.allclose(old, to_gltf(blender), atol=3e-7), 'Vertex order or base mesh changed'
right_delta = to_gltf(blender_new_right)-to_gltf(blender_old[0])
changed = np.linalg.norm(right_delta, axis=1) > 1e-6
assert int(changed.sum()) == 18, 'Unexpected user edit'


def skin_linear(primitive):
    a = primitive['attributes']
    joints = accessor(doc, raw, a['JOINTS_0']).astype(int)
    weights = accessor(doc, raw, a['WEIGHTS_0'])
    return np.einsum('nk,nkij->nij', weights, matrices[joints, :3, :3])


right_linear, left_linear = [skin_linear(p) for p in primitives]
posed_right_delta = np.einsum('nij,nj->ni', right_linear, right_delta)
posed_left_delta = posed_right_delta.copy()
posed_left_delta[:, 0] *= -1
left_delta = np.linalg.solve(left_linear, posed_left_delta[..., None])[..., 0]
points = [original[0]+right_delta, original[1]+left_delta]
mirror_error = np.linalg.norm(np.einsum('nij,nj->ni', left_linear, left_delta)-posed_left_delta, axis=1)
assert mirror_error.max() < 1e-8
for old, delta in zip(original, [right_delta, left_delta]):
    _, groups = np.unique(np.round(old, 6), axis=0, return_inverse=True)
    for group in np.unique(groups[changed]):
        part = delta[groups == group]
        assert np.linalg.norm(part-part[0], axis=1).max() < 2e-6, 'Edit separated a seam'

binary = bytearray(raw)
for primitive, old, new in zip(primitives, original, points):
    attrs = primitive['attributes']
    faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    normals = accessor(doc, raw, attrs['NORMAL']).astype(np.float64)
    attrs['POSITION'] = add_accessor(doc, binary, new, 'VEC3')
    attrs['NORMAL'] = add_accessor(doc, binary, rotate_normals(old, new, faces, normals), 'VEC3')
doc['buffers'][0]['byteLength'] = len(binary)
target = OUTPUT / 'boots_female_caveman.glb'
write_glb(target, doc, binary)
for name in ['body-occlusion.json', 'heel-pose.json']:
    shutil.copyfile(SOURCE / name, OUTPUT / name)
report = {**json.loads((SOURCE / 'fitting.json').read_text()),
          'date': '2026-11-01', 'source_glb': str((SOURCE / 'boots_female_caveman.glb').relative_to(ROOT)),
          'source_sha256': sha(SOURCE / 'boots_female_caveman.glb'),
          'user_blend': str((EDIT / 'user-edited.blend').relative_to(ROOT)),
          'user_blend_sha256': sha(EDIT / 'user-edited.blend'),
          'output_sha256': sha(target), 'changed_vertices_per_boot': int(changed.sum()),
          'maximum_right_delta_mm': float(np.linalg.norm(right_delta, axis=1).max()*1000),
          'heel_pose_mirror_error_mm': float(mirror_error.max()*1000),
          'method': 'Apply user vertex edits to right bind mesh; reflect displacement in the heel pose and inverse skin into left bind mesh. Rotate existing normals with local surface changes; retain topology, UVs, weights, morphs and textures.'}
(OUTPUT / 'fitting.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report, indent=2))
