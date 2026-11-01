import json
from pathlib import Path

import numpy as np
import trimesh

from lib.glb import read_glb
from lib.glb_accessors import accessor

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_caveman_bracers/tripo_v1/fitted_v1'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
body, body_raw = read_glb(BODY)
bracers, bracer_raw = read_glb(DIRECTORY / 'gloves_female_caveman.glb')
primitive = body['meshes'][0]['primitives'][0]
attrs = primitive['attributes']
body_vertices = accessor(body, body_raw, attrs['POSITION'])
body_faces = accessor(body, body_raw, primitive['indices']).reshape(-1, 3)
joints = accessor(body, body_raw, attrs['JOINTS_0']).astype(int)
weights = accessor(body, body_raw, attrs['WEIGHTS_0'])
names = [body['nodes'][i]['name'] for i in body['skins'][0]['joints']]
hidden = np.zeros(len(body_faces), dtype=bool)
hidden[json.loads((DIRECTORY / 'body-occlusion.json').read_text())['hide_triangle_indices']] = True
barycentric = np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1], [.5, .5, 0],
                        [0, .5, .5], [.5, 0, .5], [1/3, 1/3, 1/3]])
for side, index in [('Right', 0), ('Left', 1)]:
    allowed = [names.index(side+name) for name in ['Arm', 'ForeArm', 'Hand']]
    influence = (weights*np.isin(joints, allowed)).sum(1)
    face_ids = np.flatnonzero(influence[body_faces].mean(1) > .5)
    surface = trimesh.Trimesh(body_vertices, body_faces[face_ids], process=False)
    primitive = bracers['meshes'][index]['primitives'][0]
    vertices = accessor(bracers, bracer_raw, primitive['attributes']['POSITION'])
    faces = accessor(bracers, bracer_raw, primitive['indices']).reshape(-1, 3)
    samples = np.einsum('sk,fkj->fsj', barycentric, vertices[faces]).reshape(-1, 3)
    nearest, distance, contacts = trimesh.proximity.closest_point(surface, samples)
    signed = np.einsum('ij,ij->i', samples-nearest, surface.face_normals[contacts])
    visible = ~hidden[face_ids[contacts]]
    inside = visible & (signed < -.001) & (distance < .03)
    result = {'side': side, 'surface_samples': len(samples),
              'visible_skin_samples_deeper_than_1mm': int(inside.sum()),
              'minimum_signed_distance_mm': float(signed[inside].min()*1000) if inside.any() else None}
    print(json.dumps(result))
    assert not inside.any(), f'{side}: visible skin overlap candidates'
