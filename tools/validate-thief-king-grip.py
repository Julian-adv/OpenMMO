import hashlib
import json
from pathlib import Path

import numpy as np
import trimesh
from scipy.spatial.transform import Rotation

ROOT = Path(__file__).resolve().parents[1]
DATA = Path('/tmp/thief-king-validation/grip')
OUT = Path('/tmp/thief-king-review/grip-validation.json')
FIT = ROOT / 'assets/monsters/thief_king/grip_v1/fit.json'
fit = json.loads(FIT.read_text())
mesh = json.loads((DATA / 'mesh.json').read_text())
faces = np.asarray(mesh['faces']).reshape(-1, 3)
transform = np.eye(4)
transform[:3, :3] = Rotation.from_euler('XYZ', fit['rotation_degrees_xyz'], degrees=True).as_matrix()
transform[:3, 3] = fit['offset_m']
weapons = {}
for name in ['dagger', 'cutthroat_dirk', 'shadowfang']:
    path = ROOT / f'client/public/models/weapons/{name}.glb'
    weapon = trimesh.load(path, force='mesh', process=False)
    manager = trimesh.collision.CollisionManager()
    manager.add_object(name, weapon, transform=transform)
    weapons[name] = {'manager': manager, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                     'samples': 0, 'colliding_samples': 0, 'minimum_clearance_m': float('inf'), 'max_penetration_m': 0.0}
for name in ['idle1', 'walk', 'run', 'slash1', 'combat_idle', 'dying']:
    frames = json.loads((DATA / f'{name}.json').read_text())
    for frame in frames:
        hand = np.asarray(frame['hand_matrix']).reshape(4, 4).T
        positions = np.asarray(frame['vertices']).reshape(-1, 3)
        local = (np.c_[positions, np.ones(len(positions))] @ np.linalg.inv(hand).T)[:, :3]
        part = trimesh.Trimesh(vertices=local, faces=faces, process=False)
        manager = trimesh.collision.CollisionManager()
        manager.add_object('hand', part)
        for result in weapons.values():
            colliding, contacts = manager.in_collision_other(result['manager'], return_data=True)
            result['samples'] += 1
            if colliding:
                result['colliding_samples'] += 1
                result['max_penetration_m'] = max(result['max_penetration_m'], max(c.depth for c in contacts))
            else:
                result['minimum_clearance_m'] = min(result['minimum_clearance_m'], manager.min_distance_other(result['manager']))
for result in weapons.values():
    del result['manager']
report = {'method': 'FCL triangle-mesh collision against faces with more than 5% right-hand or right-finger influence; 49 poses per baked clip; monster attachment transform.',
          'fit': fit, 'weapons': weapons}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
if any(result['colliding_samples'] for result in weapons.values()):
    raise SystemExit('Hand/weapon intersections found')
