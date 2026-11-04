import json
from pathlib import Path

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parents[2]
BLEND = ROOT / 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1/female-warrior-hair-fitted.blend'
REPORT = ROOT / 'doc/assets/modular-female-warrior-hair-clearance-v1.json'
bpy.ops.wm.open_mainfile(filepath=str(BLEND))
scene = bpy.context.scene
rig = next(obj for obj in scene.objects if obj.type == 'ARMATURE')
body = bpy.data.objects['female_body']
hair = bpy.data.objects['hair_female_warrior']
if rig.animation_data:
    rig.animation_data.action = None
    for track in rig.animation_data.nla_tracks:
        track.mute = True
for bone in rig.pose.bones:
    bone.matrix_basis.identity()
scene.frame_set(0)
bpy.context.view_layer.update()
rest = [hair.matrix_world @ v.co for v in hair.data.vertices]
selected = [index for index, point in enumerate(rest)
            if .085 < abs(point.x) < .2 and -.18 < point.y < -.055 and 1.24 < point.z < 1.49]
body_points = [body.matrix_world @ vertex.co for vertex in body.data.vertices]
body_surface = BVHTree.FromPolygons(body_points, [face.vertices[:] for face in body.data.polygons])
rest_samples = {-1: [], 1: []}
for face in hair.data.polygons:
    if len(face.vertices) != 3:
        continue
    vertices = [rest[index] for index in face.vertices]
    for i in range(5):
        for j in range(5 - i):
            point = (vertices[0] * i + vertices[1] * j + vertices[2] * (4 - i - j)) / 4
            side = 1 if point.x > 0 else -1
            if not (.085 < point.x * side < .2 and -.18 < point.y < -.055 and 1.24 < point.z < 1.49):
                continue
            nearest, normal, _, _ = body_surface.ray_cast(Vector((side * .21, point.y, point.z)),
                                                            Vector((-side, 0, 0)), .21)
            if nearest is not None and normal.x * side > .15:
                rest_samples[side].append((point.x - nearest.x) * side)
assert all(values and min(values) > 0 for values in rest_samples.values()), 'Front hair intersects the body in rest pose'
rest_report = {str(side): {'samples': len(values), 'minimum_horizontal_clearance_m': min(values)}
               for side, values in rest_samples.items()}
clips = []
for name in ['idle1', 'idle_armed', 'walk', 'run', 'jump', 'combat_idle', 'slash1', 'dying', 'sit_idle']:
    action = bpy.data.actions[name]
    rig.animation_data.action = action
    rig.animation_data.action_slot = action.slots[0]
    first, last = action.frame_range
    worst = 0.0
    frames_with_penetration = samples_with_penetration = 0
    for sample in range(13):
        frame = first + (last - first) * sample / 12
        scene.frame_set(int(frame), subframe=frame - int(frame))
        bpy.context.view_layer.update()
        depsgraph = bpy.context.evaluated_depsgraph_get()
        body_eval = body.evaluated_get(depsgraph)
        body_mesh = body_eval.to_mesh()
        points = [body_eval.matrix_world @ vertex.co for vertex in body_mesh.vertices]
        surface = BVHTree.FromPolygons(points, [face.vertices[:] for face in body_mesh.polygons])
        hair_eval = hair.evaluated_get(depsgraph)
        hair_mesh = hair_eval.to_mesh()
        frame_penetrations = 0
        for index in selected:
            point = hair_eval.matrix_world @ hair_mesh.vertices[index].co
            nearest, normal, _, distance = surface.find_nearest(point)
            if nearest is None or distance > .05:
                continue
            signed = (point - nearest).dot(normal)
            if signed < -.001:
                frame_penetrations += 1
                worst = min(worst, signed)
        frames_with_penetration += bool(frame_penetrations)
        samples_with_penetration += frame_penetrations
        body_eval.to_mesh_clear()
        hair_eval.to_mesh_clear()
    clips.append({'name': name, 'frames_sampled': 13, 'frames_with_penetration': frames_with_penetration,
                  'penetrating_vertex_samples': samples_with_penetration, 'minimum_signed_distance_m': worst})
report = {'rest': rest_report, 'motion_vertices': len(selected), 'clips': clips,
          'method': 'Rest pose: sample triangle surfaces and measure horizontal clearance against the body. Motion: sample the same front-hair vertices against the nearest evaluated body surface, counting signed distances below -1mm.',
          'limitations': 'Motion samples are not exhaustive and include contacts with arms. Nearest-surface signs may be unreliable near open or overlapping surfaces. Garments and helmet are not checked.'}
REPORT.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
