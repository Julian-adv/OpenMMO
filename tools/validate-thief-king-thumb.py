import importlib.util
import json
from pathlib import Path

import numpy as np
import trimesh
from scipy.spatial.transform import Rotation

from lib.glb import read_glb
from lib.glb_accessors import accessor

ROOT = Path(__file__).resolve().parents[1]
DATA = Path('/tmp/thief-king-validation/grip')
OUT = Path('/tmp/thief-king-review/thumb-validation.json')
fit = json.loads((ROOT / 'assets/monsters/thief_king/grip_v1/fit.json').read_text())
doc, binary = read_glb(ROOT / 'assets/monsters/thief_king/rig_v1/thief_king.glb')
nodes = {node.get('name'): i for i, node in enumerate(doc['nodes'])}
for finger, angles in fit['fingers_degrees_x'].items():
    for i, angle in enumerate(angles, 1):
        node = doc['nodes'][nodes[f'RightHand{finger}{i}']]
        rotation = Rotation.from_quat(node['rotation']) * Rotation.from_euler('x', angle, degrees=True)
        if finger == 'Thumb':
            rotation *= Rotation.from_euler('y', fit['thumb_degrees_y'][i - 1], degrees=True)
            rotation *= Rotation.from_euler('z', fit['thumb_degrees_z'][i - 1], degrees=True)
        node['rotation'] = rotation.as_quat().tolist()
spec = importlib.util.spec_from_file_location('barkan_rig', ROOT / 'tools/rig-barkan.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
worlds = module.world_matrices(doc)
hand_inverse = np.linalg.inv(worlds[nodes['RightHand']])
weapon_rotation = Rotation.from_euler('XYZ', fit['rotation_degrees_xyz'], degrees=True)

def location(name):
    hand_point = (hand_inverse @ np.r_[worlds[nodes[name]][:3, 3], 1])[:3]
    return weapon_rotation.inv().apply(hand_point - fit['offset_m'])

thumb = [location(f'RightHandThumb{i}') for i in range(1, 5)]
index = location('RightHandIndex4')
middle = location('RightHandMiddle4')
assert thumb[0][1] < -.04 and thumb[1][1] < -.03 and thumb[2][1] < -.01
assert thumb[3][1] > -.005 and thumb[3][2] > index[2] and thumb[3][2] > middle[2] - .002
mesh = json.loads((DATA / 'mesh.json').read_text())
thumb_faces = np.asarray(mesh['thumb_faces']).reshape(-1, 3)
opposing_faces = np.asarray(mesh['opposing_faces']).reshape(-1, 3)
assert len(thumb_faces) and len(opposing_faces)
primitive = doc['meshes'][0]['primitives'][0]
base = accessor(doc, binary, primitive['attributes']['POSITION'])[mesh['ids']]

def triangle_areas(vertices):
    v = vertices[thumb_faces]
    return np.linalg.norm(np.cross(v[:, 1] - v[:, 0], v[:, 2] - v[:, 0]), axis=1) / 2

bind_area = triangle_areas(base)
area_mask = bind_area > 1e-8
min_gap, max_gap, collisions, samples = float('inf'), 0., 0, 0
area_ratios = []
for clip in ['idle1', 'walk', 'run', 'slash1', 'combat_idle', 'dying']:
    for frame in json.loads((DATA / f'{clip}.json').read_text()):
        positions = np.asarray(frame['vertices']).reshape(-1, 3)
        hand = np.asarray(frame['hand_matrix']).reshape(4, 4).T
        local = (np.c_[positions, np.ones(len(positions))] @ np.linalg.inv(hand).T)[:, :3]
        left = trimesh.collision.CollisionManager()
        right = trimesh.collision.CollisionManager()
        left.add_object('thumb', trimesh.Trimesh(vertices=local, faces=thumb_faces, process=False))
        right.add_object('index_middle', trimesh.Trimesh(vertices=local, faces=opposing_faces, process=False))
        hit = left.in_collision_other(right)
        samples += 1
        if hit:
            collisions += 1
        else:
            gap = left.min_distance_other(right)
            min_gap, max_gap = min(min_gap, gap), max(max_gap, gap)
        area_ratios.extend((triangle_areas(local)[area_mask] / bind_area[area_mask]).tolist())
report = {'coordinate_system': 'weapon-local; +X toward blade; Y and Z across handle',
          'thumb_joint_centres_m': [p.tolist() for p in thumb],
          'index_tip_joint_m': index.tolist(), 'middle_tip_joint_m': middle.tolist(),
          'samples': samples, 'thumb_index_middle_collision_samples': collisions,
          'thumb_to_index_middle_gap_m': [min_gap, max_gap],
          'thumb_triangle_area_ratio_percentiles': np.quantile(area_ratios, [.01, .05, .5, .95]).tolist()}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
assert collisions == 0 and max_gap < .006
