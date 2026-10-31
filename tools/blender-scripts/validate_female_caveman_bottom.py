import hashlib
import json
import sys
from pathlib import Path

import bpy
import bmesh
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
from lib.glb import read_glb
from lib.glb_accessors import accessor
from outfits.mesh_components import welded_islands

BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
DIRECTORY = ROOT / 'assets/items/female_caveman_bottom/tripo_v1/fitted_v9'
BOTTOM = DIRECTORY / 'bottom_female_caveman.glb'
body_doc, body_raw = read_glb(BODY)
bottom_doc, bottom_raw = read_glb(BOTTOM)
body_skin, bottom_skin = body_doc['skins'][0], bottom_doc['skins'][0]
def hierarchy(doc, skin):
    joints = set(skin['joints'])
    parents = {child: i for i, node in enumerate(doc['nodes']) for child in node.get('children', [])}
    return [(doc['nodes'][i]['name'], doc['nodes'][parents[i]]['name'] if parents.get(i) in joints else None,
             {key: doc['nodes'][i][key] for key in ('matrix', 'translation', 'rotation', 'scale') if key in doc['nodes'][i]})
            for i in skin['joints']]


assert hierarchy(body_doc, body_skin) == hierarchy(bottom_doc, bottom_skin)
assert np.array_equal(accessor(body_doc, body_raw, body_skin['inverseBindMatrices']), accessor(bottom_doc, bottom_raw, bottom_skin['inverseBindMatrices']))
attrs = bottom_doc['meshes'][0]['primitives'][0]['attributes']
weights = accessor(bottom_doc, bottom_raw, attrs['WEIGHTS_0'])
joints = accessor(bottom_doc, bottom_raw, attrs['JOINTS_0'])
assert np.isfinite(weights).all() and np.allclose(weights.sum(axis=1), 1, atol=1e-6)
assert (joints < len(bottom_skin['joints'])).all()

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(BODY), bone_heuristic='TEMPERANCE')
rig = next(o for o in bpy.data.objects if o.type == 'ARMATURE')
body = bpy.data.objects['female_body']
base_points = np.array([body.matrix_world @ v.co for v in body.data.vertices])
base_faces = np.array([f.vertices[:] for f in body.data.polygons])
_, _, labels = welded_islands(base_points, base_faces)
skin_label = np.bincount(labels).argmax()
skin_faces = base_faces[(labels[base_faces] == skin_label).all(axis=1)]
arm_groups = {g.index for g in body.vertex_groups if 'Arm' in g.name or 'Hand' in g.name}
arm_skin = np.array([sum(g.weight for g in v.groups if g.group in arm_groups) > .35 for v in body.data.vertices])
lower_faces = skin_faces[(base_points[skin_faces, 2].mean(axis=1) < 1.16) & ~arm_skin[skin_faces].any(axis=1)]
underwear = np.zeros(len(base_faces), dtype=bool)
for label in np.unique(labels):
    bounds = base_points[labels == label]
    if label != skin_label and len(bounds) > 100 and bounds[:, 2].min() > .85 and bounds[:, 2].max() < 1.15:
        underwear |= (labels[base_faces] == label).all(axis=1)
assert underwear.sum() == 1110
visual_body = body.copy()
visual_body.data = body.data.copy()
bpy.context.collection.objects.link(visual_body)
edit = bmesh.new()
edit.from_mesh(visual_body.data)
edit.faces.ensure_lookup_table()
bmesh.ops.delete(edit, geom=[edit.faces[i] for i in np.flatnonzero(underwear)], context='FACES')
edit.to_mesh(visual_body.data)
edit.free()
body.hide_render = True
for track in rig.animation_data.nla_tracks:
    track.mute = True
before = set(bpy.data.objects)
bpy.ops.import_scene.gltf(filepath=str(BOTTOM), bone_heuristic='TEMPERANCE')
bottom = next(o for o in set(bpy.data.objects) - before if o.type == 'MESH')
original = bottom.matrix_world.copy()
bottom.parent = rig
bottom.matrix_world = original
for modifier in bottom.modifiers:
    if modifier.type == 'ARMATURE':
        modifier.object = rig
fit = np.load(DIRECTORY / 'fit-data.npz')
rest, faces = fit['points'], fit['faces']
_, _, labels = welded_islands(fit['source'], faces)
shell_label = np.bincount(labels).argmax()
shell_faces = faces[(labels[faces] == shell_label).all(axis=1)]
barycentric = np.array([(1, 0, 0), (0, 1, 0), (0, 0, 1), (1/3, 1/3, 1/3), (.5, .5, 0), (.5, 0, .5), (0, .5, .5)])
edges = np.unique(np.sort(faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1), axis=0)
lengths = np.linalg.norm(rest[edges[:, 0]] - rest[edges[:, 1]], axis=1)
edges = edges[lengths > .005]
lengths = lengths[lengths > .005]
scene = bpy.context.scene
scene.render.engine = 'BLENDER_WORKBENCH'
scene.display.shading.color_type = 'TEXTURE'
scene.display.shading.light = 'STUDIO'
scene.render.resolution_x = scene.render.resolution_y = 900
bpy.ops.object.camera_add()
camera = bpy.context.object
camera.data.type = 'ORTHO'
camera.data.ortho_scale = .82
scene.camera = camera
center = Vector((0, 0, .88))
review = DIRECTORY / 'review'
review.mkdir(exist_ok=True)


def render(name, direction):
    camera.location = center + Vector(direction)
    camera.rotation_euler = (center - camera.location).to_track_quat('-Z', 'Y').to_euler()
    scene.render.filepath = str(review / f'{name}.png')
    bpy.ops.render.render(write_still=True)


def measure():
    bpy.context.view_layer.update()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    part_eval = bottom.evaluated_get(depsgraph)
    body_eval = body.evaluated_get(depsgraph)
    part_mesh = part_eval.to_mesh()
    body_mesh = body_eval.to_mesh()
    points = np.array([part_eval.matrix_world @ v.co for v in part_mesh.vertices])
    skin = np.array([body_eval.matrix_world @ v.co for v in body_mesh.vertices])
    assert len(points) == len(rest) and np.isfinite(points).all()
    tree = BVHTree.FromPolygons([Vector(p) for p in skin], lower_faces.tolist())
    samples = np.einsum('kj,fji->fki', barycentric, points[shell_faces]).reshape(-1, 3)
    distances = []
    for point in samples:
        hit, normal, _, _ = tree.find_nearest(Vector(point))
        distances.append(np.dot(point - np.array(hit), np.array(normal)))
    distances = np.array(distances)
    ratio = np.linalg.norm(points[edges[:, 0]] - points[edges[:, 1]], axis=1) / lengths
    result = {'penetrating_samples_over_3mm': int((distances < -.003).sum()),
              'maximum_penetration_m': float(max(0, -distances.min())),
              'maximum_edge_ratio': float(ratio.max())}
    part_eval.to_mesh_clear()
    body_eval.to_mesh_clear()
    return result

rig.animation_data.action = None
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
bpy.context.view_layer.update()
rest_report = measure()
for name, direction in [('front', (0, -3, 0)), ('back', (0, 3, 0)), ('side', (3, 0, 0)), ('angle', (2, -3, .4))]:
    render(name, direction)
reports = []
for name in ['idle1', 'idle_armed', 'walk', 'run', 'jump', 'slash1', 'sit_idle', 'combat_idle', 'dying']:
    action = bpy.data.actions[name]
    rig.animation_data.action = action
    samples = []
    for i, frame in enumerate(np.linspace(*action.frame_range, 13)):
        scene.frame_set(int(frame), subframe=float(frame % 1))
        samples.append(measure())
        if i == 6 and name in ('idle1', 'walk', 'run', 'jump', 'sit_idle'):
            render(name, (0, -3, 0))
    report = {'clip': name, 'samples': len(samples),
              'penetrating_samples_over_3mm': max(p['penetrating_samples_over_3mm'] for p in samples),
              'maximum_penetration_m': max(p['maximum_penetration_m'] for p in samples),
              'maximum_edge_ratio': max(p['maximum_edge_ratio'] for p in samples)}
    reports.append(report)
    print(name, report, flush=True)
report = {'body_sha256': hashlib.sha256(BODY.read_bytes()).hexdigest(),
          'bottom_sha256': hashlib.sha256(BOTTOM.read_bytes()).hexdigest(),
          'bones': len(bottom_skin['joints']), 'exact_hierarchy_and_inverse_bind': True,
          'weights_normalized': True, 'rest': rest_report, 'clips': reports,
          'shell_faces': len(shell_faces), 'samples_per_face': len(barycentric),
          'scope': 'Nearest lower-body skin distances and seven samples per shell triangle; 13 poses per clip. Approximate, not a full collision test.'}
(DIRECTORY / 'animation-validation.json').write_text(json.dumps(report, indent=2) + '\n')
