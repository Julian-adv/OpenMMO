import copy
import hashlib
import importlib.util
import json
import subprocess
from pathlib import Path

import numpy as np
import trimesh
from scipy.optimize import LinearConstraint, linprog, minimize
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose
from outfits.repair_sandal_panel import repair_panel

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'assets/items/female_barbarian_boots/tripo_v1'
SOURCE = DIRECTORY / 'source/model_url.glb'
BODY = ROOT / 'assets/modular_human_female_01/rig_v1/female-base.glb'
OUTPUT = DIRECTORY / 'fitted_v3'
OUTPUT.mkdir(exist_ok=True)
REPORT = ROOT / 'doc/assets/modular-female-barbarian-boots-fitting-v3.json'
HEEL_GROUND = ROOT / 'assets/modular_human_female_01/rig_v1/heel-ground.json'
SOLE_GAP = .0027
RING_CLEARANCE = .0054
MAX_RING_THICKNESS = .0108
STRAP_CLEARANCE = .0036
TOE_MARGIN = .0072
HEEL_MARGIN = .0054
SHOE_YAW_ADJUSTMENT_DEGREES = -7
LEFT_LATERAL_OFFSET = .0099
LEFT_REARWARD_OFFSET = .0054
RIGHT_TOE_LIFT = .0054
RIGHT_TOE_FOLLOW = .45
RIGHT_INSTEP_LIFT = .0072
RIGHT_INSTEP_CLEARANCE = .0018
LEFT_FRONT_RING_CLEARANCE = .0036
LEFT_FRONT_RING_INSET = .009
CONNECTOR_EMBED = .00036
CONNECTOR_THICKNESS = .0027
CONNECTOR_OUTER_CLEARANCE = .00216
RING_WALL_THICKNESS = .0027
RIGHT_RING_WALL_THICKNESS = .0054
REAR_RING_WIDTH_REDUCTION = .16
LOWER_RING_WALL_THICKNESS = .0036
FRONT_STRAP_THICKNESS = .0027
HEEL_STRAP_STRAIGHTENING = {'Right': (1., 1., .85, .08), 'Left': (1., 1., 1., .6)}


def smooth(value):
    t = np.clip(value, 0, 1)
    return t * t * (3 - 2 * t)


def smooth_share(points, share, radius=.0072):
    distance, neighbors = cKDTree(points).query(points, min(32, len(points)))
    kernel = np.exp(-(distance / radius) ** 2)
    for _ in range(2):
        share = (kernel * share[neighbors]).sum(1) / kernel.sum(1)
    return share


def islands(points, faces):
    _, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
    welded = welded.ravel()
    edges = welded[faces[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2)]
    _, labels = connected_components(coo_matrix((np.ones(len(edges)), (edges[:, 0], edges[:, 1])),
                                                shape=(welded.max() + 1,) * 2), directed=False)
    return labels[welded]


class Surface:
    """Posed skin queries."""

    def __init__(self, points, faces, normals):
        self.mesh = trimesh.Trimesh(points, faces, process=False)
        self.a, self.b, self.c = (points[faces[:, k]] for k in range(3))
        weights = np.array([(u, v) for u in np.linspace(0, 1, 7) for v in np.linspace(0, 1, 7) if u + v <= 1])
        samples = (self.a[:, None] * (1 - weights.sum(1))[None, :, None] + self.b[:, None] * weights[None, :, 0, None]
                   + self.c[:, None] * weights[None, :, 1, None])
        face_normal = np.cross(self.b - self.a, self.c - self.a)
        face_normal /= np.linalg.norm(face_normal, axis=1, keepdims=True)
        vertex_normal = (normals[faces[:, 0]][:, None] * (1 - weights.sum(1))[None, :, None]
                         + normals[faces[:, 1]][:, None] * weights[None, :, 0, None]
                         + normals[faces[:, 2]][:, None] * weights[None, :, 1, None])
        self.samples = samples.reshape(-1, 3)
        self.normals = vertex_normal.reshape(-1, 3)
        self.normals /= np.linalg.norm(self.normals, axis=1, keepdims=True)
        self.tree = cKDTree(self.samples)

    def signed(self, points):
        closest, distance, triangles = trimesh.proximity.closest_point(self.mesh, points)
        normals = self.mesh.face_normals[triangles]
        return np.einsum('ij,ij->i', points - closest, normals), normals

    def ray(self, origin, direction, limit=1.8):
        e1, e2 = self.b - self.a, self.c - self.a
        h = np.cross(direction, e2)
        det = (e1 * h).sum(1)
        valid = np.abs(det) > 3.24e-12
        inv = np.where(valid, 1 / np.where(valid, det, 1), 0)
        s = origin - self.a
        u = (s * h).sum(1) * inv
        q = np.cross(s, e1)
        v = (q @ direction) * inv
        t = (q * e2).sum(1) * inv
        hit = valid & (u >= 0) & (v >= 0) & (u + v <= 1) & (t > 0) & (t < limit)
        return t[hit].min() if hit.any() else None


def clear_surface(points, faces, surface, movable, clearance, welded, horizontal=False):
    movable_faces = movable[faces]
    for _ in range(40):
        probes = np.einsum('sk,fkj->fsj', sample_weights, points[faces]).reshape(-1, 3)
        distance, direction = surface.signed(probes)
        if horizontal:
            direction[:, 1] = 0
            direction /= np.maximum((direction ** 2).sum(1, keepdims=True), 1e-6)
        deficit = (clearance[:, None] - distance.reshape(-1, len(sample_weights))).clip(0)
        correction = np.zeros_like(points)
        counts = np.zeros(len(points))
        for sample, bary in enumerate(sample_weights):
            influence = movable_faces * bary
            divisor = (influence ** 2).sum(1)
            bad = (deficit[:, sample] > .000018) & (divisor > 0)
            displacement = direction.reshape(-1, len(sample_weights), 3)[:, sample] * deficit[:, sample, None]
            for corner in range(3):
                factor = np.divide(influence[:, corner], divisor, out=np.zeros(len(faces)), where=divisor > 0)
                active = bad & (influence[:, corner] > 0)
                np.add.at(correction, faces[active, corner], displacement[active] * factor[active, None])
                np.add.at(counts, faces[active, corner], 1)
        welded_correction = np.zeros((welded.max() + 1, 3))
        welded_counts = np.zeros(welded.max() + 1)
        np.add.at(welded_correction, welded, correction)
        np.add.at(welded_counts, welded, counts)
        step = welded_correction[welded] / np.maximum(welded_counts[welded, None], 1)
        step *= np.minimum(1, .0009 / np.maximum(np.linalg.norm(step, axis=1), 1.8e-12))[:, None]
        points += step * .8
        if np.linalg.norm(step, axis=1).max() < .000018:
            break


def clip_triangle_xy(polygon, triangle):
    orientation = np.sign(np.linalg.det((triangle[1:, :2] - triangle[0, :2]).T))
    for a, b in zip(triangle, np.roll(triangle, -1, axis=0)):
        edge = b[:2] - a[:2]
        values = orientation * (edge[0] * (polygon[:, 1] - a[1]) - edge[1] * (polygon[:, 0] - a[0]))
        clipped = []
        for i, point in enumerate(polygon):
            j = (i + 1) % len(polygon)
            if values[i] >= 0:
                clipped.append(point)
            if (values[i] >= 0) != (values[j] >= 0):
                clipped.append(point + (polygon[j] - point) * values[i] / (values[i] - values[j]))
        polygon = np.array(clipped)
        if not len(polygon):
            break
    return polygon


def attach_connector(points, faces, bands, first_vertex, joints, weights, bone_count):
    rows = points[first_vertex:].reshape(-1, 4, 3)
    before = rows.copy()
    band_faces = faces[bands[faces].any(1)]
    mesh = trimesh.Trimesh(points.copy(), band_faces, process=False)
    rear_limit = (mesh.bounds[0, 2] + mesh.bounds[1, 2]) / 2
    heights = rows[:, :, 1].mean(1)
    rows[:, :, 1] = heights[:, None]
    anchors = []
    for inner, outer in [(0, 3), (1, 2)]:
        x = rows[:, [inner, outer], 0].mean(1)
        rows[:, inner, 0] = rows[:, outer, 0] = x
        origins = np.column_stack([x, heights, np.full(len(rows), -.36)])
        locations, rays, triangles = mesh.ray.intersects_location(origins, np.tile([0., 0., 1.], (len(rows), 1)))
        depth = np.full(len(rows), np.nan)
        shares = np.zeros((len(rows), bone_count))
        for row in range(len(rows)):
            candidates = np.flatnonzero((rays == row) & (locations[:, 2] < rear_limit))
            if not len(candidates):
                continue
            hit = candidates[np.argmin(locations[candidates, 2])]
            triangle = triangles[hit]
            depth[row] = locations[hit, 2]
            bary = trimesh.triangles.points_to_barycentric(mesh.triangles[[triangle]], locations[[hit]])[0].clip(0, 1)
            bary /= bary.sum()
            anchors.append([first_vertex + row * 4 + inner, *band_faces[triangle], *bary])
            for corner, vertex in enumerate(band_faces[triangle]):
                np.add.at(shares[row], joints[vertex], bary[corner] * weights[vertex])
        valid = np.isfinite(depth)
        if valid.sum() < 2:
            raise ValueError('Rear connector has insufficient ring attachment points')
        surface = np.interp(heights, heights[valid], depth[valid])
        rows[:, inner, 2] = surface + CONNECTOR_EMBED
        rows[:, outer, 2] = rows[:, inner, 2] - CONNECTOR_THICKNESS
        for bone in range(bone_count):
            shares[:, bone] = np.interp(heights, heights[valid], shares[valid, bone])
        bones = np.argsort(-shares, axis=1)[:, :4]
        values = np.take_along_axis(shares, bones, axis=1)
        values /= values.sum(1, keepdims=True)
        for corner in [inner, outer]:
            vertices = first_vertex + np.arange(len(rows)) * 4 + corner
            joints[vertices], weights[vertices] = bones, values
    outer_faces = faces[(faces >= first_vertex).all(1)
                        & np.isin((faces - first_vertex) % 4, [2, 3]).all(1)]
    correction = np.zeros(len(points))
    triangles = mesh.triangles
    for face in outer_faces:
        triangle = points[face]
        lower, upper = triangle[:, :2].min(0), triangle[:, :2].max(0)
        near = ((triangles[:, :, :2].min(1) <= upper).all(1)
                & (triangles[:, :, :2].max(1) >= lower).all(1))
        matrix = (triangle[1:, :2] - triangle[0, :2]).T
        if abs(np.linalg.det(matrix)) < 3.24e-12:
            continue
        for band in triangles[near]:
            polygon = clip_triangle_xy(band.copy(), triangle)
            if not len(polygon):
                continue
            uv = (polygon[:, :2] - triangle[0, :2]) @ np.linalg.inv(matrix).T
            depth = triangle[0, 2] + uv @ (triangle[1:, 2] - triangle[0, 2])
            deficit = max(0, (depth - polygon[:, 2] + CONNECTOR_OUTER_CLEARANCE).max())
            correction[face] = np.maximum(correction[face], deficit)
    points[:, 2] -= correction
    for _ in range(12):
        for corner in [2, 3]:
            profile = rows[:, corner, 2].copy()
            rows[:, corner, 2] = np.minimum(profile, np.r_[profile[1],
                .25 * profile[:-2] + .5 * profile[1:-1] + .25 * profile[2:], profile[-2]])
    return float(np.linalg.norm(rows - before, axis=2).max()), np.array(anchors)


def ring_walls(points, faces, rings):
    groups = []
    for ring in rings:
        ids = np.flatnonzero(ring[faces].any(1))
        mesh = trimesh.Trimesh(points, faces[ids], process=False)
        center = (points[ring].min(0) + points[ring].max(0)) / 2
        radial = mesh.triangles_center - center
        radial[:, 1] = 0
        radius = np.linalg.norm(radial, axis=1)
        direction = radial / np.maximum(radius[:, None], 1e-12)
        facing = np.einsum('ij,ij->i', mesh.face_normals, direction)
        candidates = np.flatnonzero(facing < -.5)
        origins = mesh.triangles_center[candidates] - direction[candidates] * radius[candidates, None]
        locations, rays, _ = mesh.ray.intersects_location(origins, direction[candidates])
        distance = np.linalg.norm(locations - origins[rays], axis=1)
        nearest = np.full(len(candidates), np.inf)
        np.minimum.at(nearest, rays, distance)
        inner = candidates[radius[candidates] - nearest < .001]
        groups.append((ids[inner], ids[facing > .5]))
    return groups


def separate_ring_wall(points, faces, inner_ids, outer_ids, normals, joints, weights, bone_count, welded, thickness, horizontal=False):
    inner_faces = faces[inner_ids, ::-1]
    outer_faces = faces[outer_ids]
    outside = np.zeros(len(points), dtype=bool)
    outside[np.unique(outer_faces)] = True
    if horizontal:
        outside = np.isin(welded, welded[outside])
    inner_surface = Surface(points.copy(), inner_faces, normals)
    clear_surface(points, outer_faces, inner_surface, outside,
                  np.full(len(outer_faces), thickness), welded, horizontal)
    vertices = np.flatnonzero(outside)
    closest, _, triangles = trimesh.proximity.closest_point(inner_surface.mesh, points[vertices])
    bary = trimesh.triangles.points_to_barycentric(inner_surface.mesh.triangles[triangles], closest).clip(0, 1)
    bary /= bary.sum(1, keepdims=True)
    transferred = np.zeros((len(vertices), bone_count))
    for corner in range(3):
        neighbor = inner_faces[triangles, corner]
        for influence in range(4):
            np.add.at(transferred, (np.arange(len(vertices)), joints[neighbor, influence]),
                      bary[:, corner] * weights[neighbor, influence])
    bones = np.argsort(-transferred, axis=1)[:, :4]
    joints[vertices] = bones
    weights[vertices] = np.take_along_axis(transferred, bones, axis=1)
    weights[vertices] /= weights[vertices].sum(1, keepdims=True)
    return outside


def separate_front_strap(points, faces, inner_ids, outer_ids, joints, weights, bone_count, welded,
                         projection=None, fixed_ids=None):
    inner_faces, outer_faces = faces[inner_ids], faces[outer_ids]
    if projection is None:
        projection = np.eye(3)[:, [0, 2, 1]]
    inner_groups = np.unique(welded[faces[fixed_ids] if fixed_ids is not None else inner_faces])
    groups = np.setdiff1d(np.unique(welded[outer_faces]), inner_groups)
    movable = np.isin(welded, groups)
    vertices = np.flatnonzero(movable)
    lookup = np.full(welded.max() + 1, -1, dtype=int)
    lookup[groups] = np.arange(len(groups))
    constraints, deficits = [], []
    projected = points @ projection
    inner_triangles = projected[inner_faces]
    for face in outer_faces:
        triangle = projected[face]
        matrix = (triangle[1:, :2] - triangle[0, :2]).T
        if abs(np.linalg.det(matrix)) < 3.24e-12:
            continue
        for inner in inner_triangles:
            polygon = clip_triangle_xy(inner.copy(), triangle)
            for point in polygon:
                uv = np.linalg.solve(matrix, point[:2] - triangle[0, :2])
                bary = np.r_[1 - uv.sum(), uv].clip(0, 1)
                bary /= bary.sum()
                influence = bary * movable[face]
                deficit = point[2] + FRONT_STRAP_THICKNESS * influence.sum() - bary @ triangle[:, 2]
                row = np.zeros(len(groups))
                for corner in range(3):
                    if movable[face[corner]]:
                        row[lookup[welded[face[corner]]]] += bary[corner]
                if row.sum() > 1e-7:
                    constraints.append(row)
                    deficits.append(deficit * 1000)
    matrix = np.array(constraints)
    deficit = np.array(deficits)
    initial = linprog(np.ones(len(groups)), A_ub=-matrix, b_ub=-deficit, bounds=(0, 54), method='highs')
    if not initial.success:
        raise ValueError('Front strap separation is infeasible')
    result = minimize(lambda value: .5 * value @ value, initial.x, jac=lambda value: value,
                      bounds=[(0, 54)] * len(groups), constraints=LinearConstraint(matrix, deficit, np.inf),
                      method='SLSQP', options={'ftol': 1e-9, 'maxiter': 200})
    if not result.success or (matrix @ result.x - deficit).min() < -1.8e-5:
        raise ValueError('Front strap separation did not converge')
    points[vertices] += (result.x[lookup[welded[vertices]]] / 1000)[:, None] * projection[:, 2]
    surface = trimesh.Trimesh(points.copy(), inner_faces, process=False)
    closest, _, triangles = trimesh.proximity.closest_point(surface, points[vertices])
    bary = trimesh.triangles.points_to_barycentric(surface.triangles[triangles], closest).clip(0, 1)
    bary /= bary.sum(1, keepdims=True)
    transferred = np.zeros((len(vertices), bone_count))
    for corner in range(3):
        neighbor = inner_faces[triangles, corner]
        for influence in range(4):
            np.add.at(transferred, (np.arange(len(vertices)), joints[neighbor, influence]),
                      bary[:, corner] * weights[neighbor, influence])
    bones = np.argsort(-transferred, axis=1)[:, :4]
    joints[vertices] = bones
    weights[vertices] = np.take_along_axis(transferred, bones, axis=1)
    weights[vertices] /= weights[vertices].sum(1, keepdims=True)
    return movable, float(result.x.max() / 1000)


def narrow_rear_ring(points, source, rings):
    ring = rings & (source[:, 1] < .27)
    blend = smooth((source[:, 0] - .18) / .16) * ring
    center = (points[ring, 1].min() + points[ring, 1].max()) / 2
    displacement = (center - points[:, 1]) * REAR_RING_WIDTH_REDUCTION * blend
    points[:, 1] += displacement
    return blend > 0, float(np.abs(displacement).max())


def straighten_heel_straps(points, source, side):
    height = source[:, 1]
    panels = [((source[:, 0] > .35) & (source[:, 0] < .385) & (source[:, 2] > .06)
               & (height > -.015) & (height < .08)),
              ((source[:, 0] > .28) & (source[:, 0] < .305) & (source[:, 2] > .07)
               & (height > -.04) & (height < .08)),
              ((np.abs(source[:, 0] - (.327 - .5 * height)) < .022) & (source[:, 2] < -.15)
               & (height > -.02) & (height < .08)),
              ((np.abs(source[:, 0] - (.238 - .3 * height)) < .014) & (source[:, 2] < -.115)
               & (height > -.05) & (height < .08))]
    moved = np.zeros(len(points), dtype=bool)
    maximum = 0
    for panel, strength in zip(panels, HEEL_STRAP_STRAIGHTENING[side]):
        vertices = np.flatnonzero(panel)
        _, unique, inverse = np.unique(np.round(source[vertices], 6), axis=0, return_index=True, return_inverse=True)
        inverse = inverse.ravel()
        if len(unique) % 2:
            raise ValueError('Heel strap must have paired edge vertices')
        rows = unique[np.argsort(height[vertices[unique]])].reshape(-1, 2)
        rows = np.take_along_axis(rows, np.argsort(source[vertices[rows], 2], axis=1), axis=1)
        progress = source[vertices[rows], 1].mean(1)
        progress = (progress - progress[0]) / (progress[-1] - progress[0])
        endpoints = points[vertices[rows[[0, -1]]]].copy()
        target = points[vertices].copy()
        for row, t in zip(rows, progress):
            for edge in range(2):
                aliases = inverse == inverse[row[edge]]
                target[aliases] = endpoints[0, edge] * (1 - t) + endpoints[1, edge] * t
        displacement = (target - points[vertices]) * strength
        points[vertices] += displacement
        maximum = max(maximum, np.linalg.norm(displacement, axis=1).max())
        moved |= panel
    return moved, float(maximum)


base, base_raw = read_glb(BODY)
heel_matrices, pose_config = heel_pose(base, base_raw)
body_primitive = base['meshes'][0]['primitives'][0]
bv = accessor(base, base_raw, body_primitive['attributes']['POSITION'])
bn = accessor(base, base_raw, body_primitive['attributes']['NORMAL'])
bf = accessor(base, base_raw, body_primitive['indices']).reshape(-1, 3)
body_joints = accessor(base, base_raw, body_primitive['attributes']['JOINTS_0'])
body_weights = accessor(base, base_raw, body_primitive['attributes']['WEIGHTS_0'])
deform = np.einsum('nk,nkij->nij', body_weights, heel_matrices[body_joints])
posed = np.einsum('nij,nj->ni', deform, np.column_stack([bv, np.ones(len(bv))]))[:, :3]
posed_normals = np.einsum('nij,nj->ni', deform[:, :3, :3], bn)
posed_normals /= np.linalg.norm(posed_normals, axis=1, keepdims=True)
body_labels = islands(bv, bf)
skin_label = np.bincount(body_labels).argmax()
leg = bf[(body_labels[bf[:, 0]] == skin_label) & (posed[bf, 1] < .54).all(1) & (posed[bf, 0] < -.054).all(1)]
skin = Surface(posed, leg, posed_normals)
foot = posed[np.unique(leg)]
foot = foot[foot[:, 1] < .18]
names = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']]
node_index = {node.get('name'): i for i, node in enumerate(base['nodes'])}
toe_base = np.array(pose_config['posed_foot_positions_before_root_lift']['RightToeBase'])

doc, raw = read_glb(SOURCE)
primitive = doc['meshes'][0]['primitives'][0]
matrix = np.array(doc['nodes'][0]['matrix']).reshape(4, 4).T
source = accessor(doc, raw, primitive['attributes']['POSITION']) @ matrix[:3, :3].T + matrix[:3, 3]
source_normals = accessor(doc, raw, primitive['attributes']['NORMAL']) @ matrix[:3, :3].T
uv = accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
faces = accessor(doc, raw, primitive['indices']).reshape(-1, 3)
labels = islands(source, faces)
for i, j in cKDTree(source).query_pairs(1e-3):
    source[max(i, j)] = source[min(i, j)]
sizes = np.bincount(labels)
main = labels == sizes.argmax()
buckles = [labels == label for label in np.flatnonzero((sizes > 0) & (sizes < 100))]
rings = [labels == label for label in np.flatnonzero(sizes >= 100) if label != sizes.argmax()]
rings.append(main & (source[:, 1] > .09))
ring_any = np.any(rings, axis=0)
lower = main & ~ring_any
wall_groups = ring_walls(source, faces, rings)
wall_groups = [sorted(wall_groups, key=lambda group: source[faces[group[0]], 1].mean())[1]]
lower_ring = ring_any & (source[:, 1] < .27)
lower_faces = np.flatnonzero(lower_ring[faces].all(1))
lower_wall_groups = [(lower_faces[inner], lower_faces[outer]) for inner, outer in
                     ring_walls(source, faces[lower_faces], [lower_ring])]
lower_wall_groups = [(inner, outer[(source[faces[outer], 0] > .18).all(1)
                                       & (source[faces[outer], 1].mean(1) > .12)])
                     for inner, outer in lower_wall_groups]


foot_origin = np.array(pose_config['posed_foot_positions_before_root_lift']['RightFoot'])
forward = toe_base - foot_origin
forward[1] = 0
forward /= np.linalg.norm(forward)
yaw = np.deg2rad(SHOE_YAW_ADJUSTMENT_DEGREES)
forward = np.array([[np.cos(yaw), 0, np.sin(yaw)], [0, 1, 0],
                    [-np.sin(yaw), 0, np.cos(yaw)]]) @ forward
up = np.array([0., 1., 0.])
lateral = np.cross(up, forward)
foot_frame = np.column_stack([lateral, up, forward])
foot_local = foot @ foot_frame
toe_local = toe_base @ foot_frame
foot_length = np.ptp(foot_local[:, 2]) + TOE_MARGIN + HEEL_MARGIN
sole = lower & (source[:, 1] < -.4)
toe_tip = source[sole, 0].min()
heel_back = source[lower, 0].max()
scale = foot_length / (heel_back - toe_tip)
points_local = np.column_stack([source[:, 2], source[:, 1], -source[:, 0]]) * scale
normals_local = np.column_stack([source_normals[:, 2], source_normals[:, 1], -source_normals[:, 0]])
forefoot_local = foot_local[foot_local[:, 2] > toe_local[2] - .018]
points_local += np.array([
    (forefoot_local[:, 0].min() + forefoot_local[:, 0].max()) / 2 - (points_local[sole, 0].min() + points_local[sole, 0].max()) / 2,
    foot_local[:, 1].min() - SOLE_GAP - points_local[sole & (source[:, 1] > -.47), 1].max(),
    foot_local[:, 2].max() + TOE_MARGIN - points_local[:, 2].max(),
])
points = points_local @ foot_frame.T
normals = normals_local @ foot_frame.T
start = points.copy()

targets = points.copy()
fixed = np.zeros(len(points), dtype=bool)
up = np.array([0., 1., 0.])
toe_strap = (source[:, 0] < -.1) & (source[:, 1] > -.42)
front_face_ids = np.flatnonzero(toe_strap[faces].all(1))
front_source_mesh = trimesh.Trimesh(source, faces[front_face_ids], process=False)
front_inner = front_face_ids[front_source_mesh.face_normals[:, 1] < -.5]
front_outer = front_face_ids[front_source_mesh.face_normals[:, 1] > .5]
front_side_groups = []
for direction in [-1, 1]:
    area = direction * source[faces[front_face_ids], 2].mean(1) > .08
    inner = front_face_ids[area & (direction * front_source_mesh.face_normals[:, 2] < -.5)]
    outer = front_face_ids[area & (direction * front_source_mesh.face_normals[:, 2] > .5)]
    front_side_groups.append((direction, inner, outer))
front_fixed_inner = np.unique(np.r_[front_inner, *[group[1] for group in front_side_groups]])


footbed = lower & (normals[:, 1] > .5) & (source[:, 1] < -.05) & ~toe_strap
for i in np.flatnonzero(footbed):
    hit = skin.ray(points[i] - up * .018, up, .108)
    if hit is not None:
        targets[i, 1] = points[i, 1] - .018 + hit - SOLE_GAP
        fixed[i] = True
ground = -json.loads(HEEL_GROUND.read_text())['posed_floor_offset']
sole_bottom = lower & (normals[:, 1] < -.5) & (source[:, 1] < -.45) & (source[:, 0] < .1)
targets[sole_bottom, 1] = ground
fixed |= sole_bottom
post = lower & (source[:, 0] > .3) & (source[:, 1] < -.25)
seat = lower & (source[:, 0] > .2) & (source[:, 1] > -.25) & (source[:, 1] < -.05)
tip = post & (source[:, 1] < -.49)
targets[tip, 1] = ground
fixed |= tip

section_heights = np.arange(-.036, .288, .0036)
section_centers = []
for height in section_heights:
    section = skin.samples[np.abs(skin.samples[:, 1] - height) < .0036]
    section_centers.append((section.min(0) + section.max(0)) / 2)
section_centers = np.array(section_centers)


def axis_at(height):
    return np.array([np.interp(height, section_heights, section_centers[:, 0]), height,
                     np.interp(height, section_heights, section_centers[:, 2])])


for ring in rings:
    center = (points[ring].min(0) + points[ring].max(0)) / 2
    offset = points[ring][:, [0, 2]] - center[[0, 2]]
    radius = np.linalg.norm(offset, axis=1)
    direction = offset / radius[:, None]
    slot = ((np.arctan2(direction[:, 1], direction[:, 0]) + np.pi) / (2 * np.pi) * 48).astype(int) % 48
    inner = np.array([radius[slot == k].min() if (slot == k).any() else np.nan for k in range(48)])
    valid = np.isfinite(inner)
    inner = np.interp(np.arange(48), np.flatnonzero(valid), inner[valid], period=48)
    depth_over_inner = np.minimum(radius - inner[slot], MAX_RING_THICKNESS)
    for k, i in enumerate(np.flatnonzero(ring)):
        ray = np.array([direction[k, 0], 0, direction[k, 1]])
        origin = axis_at(points[i, 1])
        hit = skin.ray(origin + ray * .27, -ray, .27)
        if hit is not None:
            targets[i] = origin + ray * (.27 - hit + RING_CLEARANCE + depth_over_inner[k])
    fixed |= ring
for buckle in buckles:
    nearest = cKDTree(points[ring_any]).query(points[buckle].mean(0))[1]
    k = np.flatnonzero(ring_any)[nearest]
    moved = points[buckle] + targets[k] - points[k]
    away = np.r_[moved.mean(0)[[0, 2]] - axis_at(moved[:, 1].mean())[[0, 2]], 0][[0, 2, 1]]
    away /= np.linalg.norm(away)
    for _ in range(20):
        if skin.signed(moved)[0].min() >= RING_CLEARANCE:
            break
        moved += away * .0009
    targets[buckle] = moved
    fixed |= buckle

np.savez(OUTPUT / 'arap-input.npz', points=points, faces=faces, fixed=fixed, targets=targets,
         colors=np.where(ring_any[:, None] | np.any(buckles, axis=0)[:, None], 1., 0.).repeat(3, 1))
subprocess.run([str(ROOT / '.venv/bin/python'), str(ROOT / 'tools/outfits/fit_female_knight_top_surface.py'),
                str(OUTPUT / 'arap-input.npz')], check=True)
arap = np.load(OUTPUT / 'arap-result.npz')
fitted = arap['points'].copy()
rotations = arap['rotations']

free = (~fixed | ring_any | toe_strap) & ~post & ~sole_bottom
sample_weights = np.array([(u, v, 1 - u - v) for u in np.linspace(0, 1, 5)
                           for v in np.linspace(0, 1, 5) if u + v <= 1])
_, welded = np.unique(np.round(source, 6), axis=0, return_inverse=True)
welded = welded.ravel()
gap = np.where(np.any(ring_any[faces], axis=1), RING_CLEARANCE, STRAP_CLEARANCE)
clear_surface(fitted, faces, skin, free, gap, welded)

fitted[:, 1] = np.maximum(fitted[:, 1], ground)
final_depth = skin.signed(fitted)[0]
fitted_normals = np.einsum('nij,nj->ni', rotations, normals)
fitted_normals /= np.linalg.norm(fitted_normals, axis=1, keepdims=True)

leg_bone, foot_bone, toe_bone = (names.index('Right' + name) for name in ['Leg', 'Foot', 'ToeBase'])
heel_top = fitted[seat, 1].max()
ring_bottom = fitted[ring_any, 1].min()
shin = np.where(np.any(buckles, axis=0) | ring_any, 1, smooth((fitted[:, 1] - heel_top) / max(ring_bottom - heel_top, .018)))
toe = (1 - shin) * .2 * smooth((fitted[:, 2] - (toe_base[2] - .0144)) / .054) * smooth((fitted[:, 1] - ground - .0036) / .018)
weights = np.column_stack([shin, 1 - shin - toe, toe, np.zeros(len(fitted))])
joints = np.tile([leg_bone, foot_bone, toe_bone, 0], (len(fitted), 1))
ring_points = cKDTree(fitted[ring_any])
strap = ~ring_any & ~np.any(buckles, axis=0) & (fitted[:, 1] > heel_top)
skin_mesh = skin.mesh
closest, _, triangle = trimesh.proximity.closest_point(skin_mesh, fitted)
barycentric = trimesh.triangles.points_to_barycentric(skin_mesh.triangles[triangle], closest).clip(0, 1)
barycentric /= barycentric.sum(1, keepdims=True)
transferred = np.zeros((len(fitted), len(names)))
for corner in range(3):
    vertex = leg[triangle, corner]
    for influence in range(4):
        np.add.at(transferred, (np.arange(len(fitted)), body_joints[vertex, influence]),
                  barycentric[:, corner] * body_weights[vertex, influence])
flexible = ring_any
weights[flexible] = transferred[flexible][:, [leg_bone, foot_bone, toe_bone, 0]]
weights[flexible] /= weights[flexible].sum(1, keepdims=True)
for ring in rings:
    share = smooth_share(fitted[ring], weights[ring, 1] + weights[ring, 2])
    weights[ring] = np.column_stack([1 - share, share, np.zeros((ring.sum(), 2))])
strap &= ~toe_strap
nearest = np.flatnonzero(ring_any)[ring_points.query(fitted[strap])[1]]
blend = np.clip((fitted[strap, 1] - heel_top) / max(ring_bottom - heel_top, .018), 0, 1)[:, None]
weights[strap] = (1 - blend) * np.array([0, 1, 0, 0]) + blend * weights[nearest]
for buckle in buckles:
    nearest = ring_points.query(fitted[buckle].mean(0), 20)[1]
    weights[buckle] = weights[np.flatnonzero(ring_any)[nearest]].mean(0)

toe_share = .35 * smooth((fitted[:, 1] - ground - .0072) / .036) * smooth((fitted[:, 2] - toe_base[2] + .027) / .072)
weights[toe_strap, 0] = 0
weights[toe_strap, 1] = 1 - toe_share[toe_strap]
weights[toe_strap, 2] = toe_share[toe_strap]
weights[toe_strap, 3] = 0

original_vertex_count = len(fitted)
rear_sections = []
for ring in rings:
    rear = ring & (np.abs(fitted[:, 0] - axis_at(fitted[ring, 1].mean())[0]) < .0216)
    rear_sections.append((fitted[ring, 1].mean(), fitted[rear, 2].min()))
rear_sections.sort()
rear_sections = np.array(rear_sections)
heights = np.linspace(fitted[ring_any, 1].min() + .0018, fitted[ring_any, 1].max() - .0018, 19)
connector = []
for y in heights:
    center = axis_at(y)
    rear_z = np.interp(y, rear_sections[:, 0], rear_sections[:, 1])
    for x_offset, thickness in [(-.009, 0), (.009, 0), (.009, .0027), (-.009, .0027)]:
        x = center[0] + x_offset
        hit = skin.ray(np.array([x, y, -.36]), np.array([0., 0., 1.]))
        z = min(-.36 + hit - .0063, rear_z - .0009) if hit is not None else rear_z - .0009
        connector.append([x, y, z - thickness])
connector = np.array(connector)
connector_faces = []
for row in range(len(heights) - 1):
    for side in range(4):
        a, b = row * 4 + side, row * 4 + (side + 1) % 4
        connector_faces.extend([[a, b, b + 4], [a, b + 4, a + 4]])
connector_faces.extend([[0, 2, 1], [0, 3, 2]])
last = (len(heights) - 1) * 4
connector_faces.extend([[last, last + 1, last + 2], [last, last + 2, last + 3]])
connector_faces = np.array(connector_faces)
connector_mesh = trimesh.Trimesh(connector, connector_faces, process=False)
closest, _, triangle = trimesh.proximity.closest_point(skin_mesh, connector)
barycentric = trimesh.triangles.points_to_barycentric(skin_mesh.triangles[triangle], closest).clip(0, 1)
barycentric /= barycentric.sum(1, keepdims=True)
connector_weights = np.zeros((len(connector), len(names)))
for corner in range(3):
    vertex = leg[triangle, corner]
    for influence in range(4):
        np.add.at(connector_weights, (np.arange(len(connector)), body_joints[vertex, influence]),
                  barycentric[:, corner] * body_weights[vertex, influence])
connector_weights = connector_weights[:, [leg_bone, foot_bone, toe_bone, 0]]
connector_weights /= connector_weights.sum(1, keepdims=True)
connector_share = smooth_share(connector, connector_weights[:, 1] + connector_weights[:, 2], .0144)
connector_weights = np.column_stack([1 - connector_share, connector_share, np.zeros((len(connector), 2))])
leather_faces = faces[(post[faces]).all(1)]
uv_triangle = uv[leather_faces[np.argmax(trimesh.triangles.area(fitted[leather_faces]))]]
connector_uv = []
for row in range(len(heights)):
    for corner in range(4):
        u, v = (.2 if corner in [0, 3] else .4), .2 + .3 * row / (len(heights) - 1)
        connector_uv.append(uv_triangle[0] * (1 - u - v) + uv_triangle[1] * u + uv_triangle[2] * v)
fitted = np.vstack([fitted, connector])
fitted_normals = np.vstack([fitted_normals, connector_mesh.vertex_normals])
faces = np.vstack([faces, connector_faces + original_vertex_count])
uv = np.vstack([uv, connector_uv])
joints = np.vstack([joints, np.tile([leg_bone, foot_bone, toe_bone, 0], (len(connector), 1))])
weights = np.vstack([weights, connector_weights])
for key in ['ring_any', 'fixed', 'post', 'seat', 'toe_strap', 'strap']:
    globals()[key] = np.r_[globals()[key], np.zeros(len(connector), dtype=bool)]
buckles = [np.r_[mask, np.zeros(len(connector), dtype=bool)] for mask in buckles]
source = np.vstack([source, np.zeros_like(connector)])
start = np.vstack([start, connector])
final_depth = skin.signed(fitted)[0]

order = np.argsort(-weights, axis=1)
joints, weights = np.take_along_axis(joints, order, 1), np.take_along_axis(weights, order, 1)

binary = bytearray(raw)
meshes, nodes = [], []
left_fit = {}
right_fitted = fitted.copy()
right_weights = weights.copy()
right_joints = joints.copy()
right_toe_lift = RIGHT_TOE_LIFT
left_front_ring_inset = 0
connector_adjustments = {}
connector_anchors = {}
heel_strap_adjustments = {}
heel_strap_vertices = np.zeros(len(fitted), dtype=bool)
rear_ring_adjustments = {}
rear_ring_vertices = np.zeros(len(fitted), dtype=bool)
lower_wall_vertices = np.zeros(len(fitted), dtype=bool)
lower_wall_adjustments = {}
front_wall_adjustments = {}
front_wall_vertices = np.zeros(len(fitted), dtype=bool)
front_side_vertices = np.zeros(len(fitted), dtype=bool)
front_side_adjustments = {}
right_panel = {}
right_source = None
right_faces = None
right_panel_vertices = np.zeros(len(fitted), dtype=bool)
for side in ['Right', 'Left']:
    points_side, normals_side, faces_side, joints_side = fitted.copy(), fitted_normals.copy(), faces.copy(), joints.copy()
    weights_side = weights.copy()
    uv_side = uv.copy()
    if side == 'Right':
        strap_progress = np.clip((points_side[:, 1] - ground - .009) / .036, 0, 1)
        points_side[:, 1] += RIGHT_INSTEP_LIFT * smooth(strap_progress) * toe_strap
        strap_slope = RIGHT_INSTEP_LIFT * 6 * strap_progress * (1 - strap_progress) / .036 * toe_strap
        normals_side[:, 1] /= 1 + strap_slope
        normals_side /= np.linalg.norm(normals_side, axis=1, keepdims=True)
        _, strap_direction = skin.signed(points_side)
        points_side += (RIGHT_INSTEP_CLEARANCE * smooth(strap_progress) * toe_strap)[:, None] * strap_direction
        toe_start = toe_local[2] + .0684
        toe_span = (points_side @ forward).max() - toe_start
        toe_progress = np.clip((points_side @ forward - toe_start) / toe_span, 0, 1)
        ankle_progress = np.clip((points_side[:, 1] - heel_top) / (ring_bottom - heel_top), 0, 1)
        ankle_share = 1 - smooth(ankle_progress)
        lift_share = toe_progress ** 2 * ankle_share
        triangles = points_side[faces]
        face_normals = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
        top = ((source[faces, 1] < -.4).all(1)
               & (face_normals[:, 1] > .4 * np.linalg.norm(face_normals, axis=1)))
        probes = np.einsum('sk,fkj->fsj', sample_weights, triangles[top]).reshape(-1, 3)
        probe_share = np.einsum('sk,fk->fs', sample_weights, lift_share[faces[top]]).reshape(-1)
        for probe, share in zip(probes, probe_share):
            if share < .001:
                continue
            hit = skin.ray(probe, up, .072)
            if hit is not None:
                clearance = min(SOLE_GAP * .2, hit * .2)
                right_toe_lift = min(right_toe_lift, (hit - clearance) / share)
        points_side[:, 1] += right_toe_lift * lift_share
        slope = right_toe_lift * 2 * toe_progress / toe_span * ankle_share
        gradient = slope[:, None] * forward
        gradient[:, 1] = -right_toe_lift * toe_progress ** 2 * 6 * ankle_progress * (1 - ankle_progress) / (ring_bottom - heel_top)
        normals_side -= gradient * (normals_side[:, 1] / (1 + gradient[:, 1]))[:, None]
        normals_side /= np.linalg.norm(normals_side, axis=1, keepdims=True)
        follow_start = toe_local[2] + .0468
        follow_span = (points_side @ forward).max() - follow_start
        follow_progress = np.clip((points_side @ forward - follow_start) / follow_span, 0, 1)
        toe_follow = RIGHT_TOE_FOLLOW * follow_progress ** 2 * ankle_share
        weights_side *= 1 - toe_follow[:, None]
        weights_side += (joints_side == toe_bone) * toe_follow[:, None]
        right_weights = weights_side.copy()
        right_fitted = points_side.copy()
        final_depth = skin.signed(points_side)[0]
    if side == 'Left':
        points_side[:, 0] *= -1
        normals_side[:, 0] *= -1
        faces_side = faces_side[:, ::-1]
        transition = np.clip((points_side[:, 1] - heel_top) / (ring_bottom - heel_top), 0, 1)
        offset = lateral * [1, 1, -1] * LEFT_LATERAL_OFFSET - forward * [-1, 1, 1] * LEFT_REARWARD_OFFSET
        points_side += (1 - smooth(transition))[:, None] * offset
        slope = -6 * transition * (1 - transition) / (ring_bottom - heel_top)
        normals_side[:, 1] -= slope * (normals_side @ offset)
        normals_side /= np.linalg.norm(normals_side, axis=1, keepdims=True)
        for name in ['Leg', 'Foot', 'ToeBase']:
            joints_side[joints == names.index('Right' + name)] = names.index('Left' + name)
        left_faces = bf[(body_labels[bf[:, 0]] == skin_label) & (posed[bf, 1] < .54).all(1)
                        & (posed[bf, 0] > .054).all(1)]
        left_skin = Surface(posed, left_faces, posed_normals)
        left_centers = []
        for height in section_heights:
            section = left_skin.samples[np.abs(left_skin.samples[:, 1] - height) < .0036]
            left_centers.append((section.min(0) + section.max(0)) / 2)
        left_centers = np.array(left_centers)
        front_ring_area = ring_any.copy()
        front_ring_area[np.unique(faces_side[ring_any[faces_side].any(1)])] = True
        ring_movement = np.zeros_like(points_side)
        left_forward = forward * [-1, 1, 1]
        for i in np.flatnonzero(front_ring_area):
            height = points_side[i, 1]
            center = np.array([np.interp(height, section_heights, left_centers[:, 0]), height,
                               np.interp(height, section_heights, left_centers[:, 2])])
            radial = points_side[i] - center
            radius = np.linalg.norm(radial)
            ray = radial / radius
            blend = smooth((ray @ left_forward - .2) / .6)
            ring_movement[i] = -left_forward * LEFT_FRONT_RING_INSET * blend
        for buckle in buckles:
            near = cKDTree(points_side[ring_any]).query(points_side[buckle].mean(0))[1]
            ring_movement[buckle] = ring_movement[np.flatnonzero(ring_any)[near]]
        points_side += ring_movement
        left_front_ring_inset = np.linalg.norm(ring_movement, axis=1).max()
        front_neighborhood = np.zeros(len(fitted), dtype=bool)
        front_neighborhood[np.unique(faces[toe_strap[faces].any(1)])] = True
        front_neighborhood &= points_side[:, 1] > ground + .0072
        adjustable = front_ring_area | front_neighborhood | (np.arange(len(fitted)) >= original_vertex_count)
        front_ring = np.linalg.norm(ring_movement, axis=1) > 0
        ring_clearance = np.where(front_ring, LEFT_FRONT_RING_CLEARANCE, RING_CLEARANCE)
        clearance = np.where(front_ring_area, ring_clearance, STRAP_CLEARANCE + .0045)
        for _ in range(5):
            distance, direction = left_skin.signed(points_side)
            inside = adjustable & (distance < clearance)
            points_side[inside] += direction[inside] * (clearance[inside] - distance[inside])[:, None]
        ankle = front_ring_area | np.any(buckles, axis=0)
        ankle_faces = faces_side[ankle[faces_side].any(1)]
        _, left_welded = np.unique(np.round(fitted, 7), axis=0, return_inverse=True)
        clear_surface(points_side, ankle_faces, left_skin, ankle,
                      ring_clearance[ankle_faces].max(1), left_welded.ravel())
        points_side[:, 1] = np.maximum(points_side[:, 1], ground)
        left_bones = [names.index('Left' + suffix) for suffix in ['Leg', 'Foot', 'ToeBase']]
        left_ring_groups = [np.r_[ring, np.zeros(len(connector), dtype=bool)] for ring in rings]
        left_ring_groups.append(front_ring_area & ~ring_any & front_ring)
        for ring in left_ring_groups:
            if not ring.any():
                continue
            closest, _, triangles = trimesh.proximity.closest_point(left_skin.mesh, points_side[ring])
            bary = trimesh.triangles.points_to_barycentric(left_skin.mesh.triangles[triangles], closest).clip(0, 1)
            bary /= bary.sum(1, keepdims=True)
            skin_vertices = left_faces[triangles]
            foot_share = np.sum(bary * np.sum(
                ((body_joints[skin_vertices] == left_bones[1]) | (body_joints[skin_vertices] == left_bones[2]))
                * body_weights[skin_vertices], axis=2), axis=1)
            foot_share = smooth_share(points_side[ring], foot_share)
            weights_side[ring] = np.column_stack([1 - foot_share, foot_share, np.zeros((ring.sum(), 2))])
            joints_side[ring] = [*left_bones, 0]
        near = np.flatnonzero(ring_any)[cKDTree(points_side[ring_any]).query(points_side[strap])[1]]
        left_blend = np.clip((points_side[strap, 1] - heel_top) / max(ring_bottom - heel_top, .018), 0, 1)[:, None]
        weights_side[strap] = (1 - left_blend) * np.array([0, 1, 0, 0]) + left_blend * weights_side[near]
        joints_side[strap] = [*left_bones, 0]
        for buckle in buckles:
            near = cKDTree(points_side[ring_any]).query(points_side[buckle].mean(0), 20)[1]
            ring_vertices = np.flatnonzero(ring_any)[near]
            weights_side[buckle] = weights_side[ring_vertices].mean(0)
            joints_side[buckle] = [*left_bones, 0]
    before_wall_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals.copy()
    heel_straps, heel_strap_adjustments[side.lower()] = straighten_heel_straps(points_side, source, side)
    heel_strap_vertices |= heel_straps
    _, wall_welded = np.unique(np.round(source, 6), axis=0, return_inverse=True)
    wall_welded = wall_welded.ravel()
    wall_welded[original_vertex_count:] = wall_welded.max() + 1 + np.arange(len(connector))
    wall_vertices = np.zeros(len(points_side), dtype=bool)
    wall_thickness = RIGHT_RING_WALL_THICKNESS if side == 'Right' else RING_WALL_THICKNESS
    for inner_ids, outer_ids in wall_groups:
        wall_vertices |= separate_ring_wall(points_side, faces_side, inner_ids, outer_ids, normals_side,
                                            joints_side, weights_side, len(names), wall_welded, wall_thickness)
    after_wall_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals
    updated_normals = wall_vertices | heel_straps
    normals_side[updated_normals] += after_wall_normals[updated_normals] - before_wall_normals[updated_normals]
    normals_side /= np.linalg.norm(normals_side, axis=1, keepdims=True)
    before_rear_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals.copy()
    rear_ring, rear_ring_adjustments[side.lower()] = narrow_rear_ring(points_side, source, ring_any)
    rear_ring_vertices |= rear_ring
    before_lower_wall = points_side.copy()
    lower_outside = np.zeros(len(points_side), dtype=bool)
    for inner_ids, outer_ids in lower_wall_groups:
        lower_outside |= separate_ring_wall(points_side, faces_side, inner_ids, outer_ids, normals_side,
                                             joints_side, weights_side, len(names), wall_welded,
                                             LOWER_RING_WALL_THICKNESS, horizontal=True)
    lower_wall_vertices |= lower_outside
    lower_wall_adjustments[side.lower()] = float(np.linalg.norm(points_side - before_lower_wall, axis=1).max())
    rear_ring = rear_ring | lower_outside
    after_rear_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals
    normals_side[rear_ring] += after_rear_normals[rear_ring] - before_rear_normals[rear_ring]
    normals_side[rear_ring] /= np.linalg.norm(normals_side[rear_ring], axis=1, keepdims=True)
    connector_adjustments[side.lower()], connector_anchors[side.lower()] = attach_connector(
        points_side, faces_side, ring_any | np.any(buckles, axis=0), original_vertex_count,
        joints_side, weights_side, len(names))
    connector_triangles = faces_side[(faces_side >= original_vertex_count).all(1)] - original_vertex_count
    normals_side[original_vertex_count:] = trimesh.Trimesh(
        points_side[original_vertex_count:], connector_triangles, process=False).vertex_normals
    before_front_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals.copy()
    front_wall, front_wall_adjustments[side.lower()] = separate_front_strap(
        points_side, faces_side, front_inner, front_outer, joints_side, weights_side, len(names), wall_welded)
    side_front = np.zeros(len(points_side), dtype=bool)
    side_adjustment = 0
    mirror = np.array([-1, 1, 1]) if side == 'Left' else np.ones(3)
    for direction, inner, outer in front_side_groups:
        projection = np.column_stack([forward * mirror, up, lateral * mirror * direction])
        panel, adjustment = separate_front_strap(points_side, faces_side, inner, outer, joints_side,
                                                 weights_side, len(names), wall_welded,
                                                 projection=projection, fixed_ids=front_fixed_inner)
        side_front |= panel
        side_adjustment = max(side_adjustment, adjustment)
    front_side_vertices |= side_front
    front_side_adjustments[side.lower()] = side_adjustment
    extra_front, extra_lift = separate_front_strap(
        points_side, faces_side, front_inner, front_outer, joints_side, weights_side, len(names), wall_welded,
        fixed_ids=front_fixed_inner)
    front_wall = front_wall | extra_front | side_front
    front_wall_adjustments[side.lower()] += extra_lift
    if side == 'Right':
        panel_frame = np.column_stack([forward, up, lateral])
        right_source, right_panel_vertices, right_panel = repair_panel(
            points_side, faces_side, front_side_groups[1][2], uv_side, joints_side, weights_side,
            source, panel_frame, len(names), np.array([[.14, .52], [.29, .64]]))
        panel_inner = np.unique(np.r_[front_fixed_inner, 481, 495, 1204])
        _, right_panel['clearance_adjustment'] = separate_front_strap(
            points_side, faces_side, panel_inner, front_side_groups[1][2], joints_side, weights_side,
            len(names), np.arange(len(points_side)), projection=panel_frame, fixed_ids=panel_inner)
        front_wall |= right_panel_vertices
        right_faces = faces_side.copy()
    front_wall_vertices |= front_wall
    after_front_normals = trimesh.Trimesh(points_side, faces_side, process=False).vertex_normals
    normals_side[front_wall] += after_front_normals[front_wall] - before_front_normals[front_wall]
    normals_side[front_wall] /= np.linalg.norm(normals_side[front_wall], axis=1, keepdims=True)
    if side == 'Right':
        panel_vertices = np.unique(faces_side[front_side_groups[1][2]])
        normals_side[panel_vertices] = after_front_normals[panel_vertices]

    if side == 'Right':
        right_fitted = points_side.copy()
        right_weights = weights_side.copy()
        right_joints = joints_side.copy()
        final_depth = skin.signed(points_side)[0]
    else:
        left_fit = {'left_fitted': points_side.copy(), 'left_joints': joints_side.copy(), 'left_weights': weights_side.copy()}
    deformation = np.einsum('nk,nkij->nij', weights_side, heel_matrices[joints_side])
    bind = np.linalg.solve(deformation, np.column_stack([points_side, np.ones(len(points_side))])[..., None])[:, :3, 0]
    bind_normals = np.einsum('nji,nj->ni', deformation[:, :3, :3], normals_side)
    bind_normals /= np.linalg.norm(bind_normals, axis=1, keepdims=True)
    attributes = {'POSITION': add_accessor(doc, binary, bind.astype(np.float32), 'VEC3'),
                  'NORMAL': add_accessor(doc, binary, bind_normals.astype(np.float32), 'VEC3'),
                  'TEXCOORD_0': add_accessor(doc, binary, uv_side, 'VEC2'),
                  'JOINTS_0': add_accessor(doc, binary, joints_side.astype(np.uint16), 'VEC4', 5123),
                  'WEIGHTS_0': add_accessor(doc, binary, weights_side.astype(np.float32), 'VEC4')}
    for key in ['POSITION', 'NORMAL', 'TEXCOORD_0', 'JOINTS_0', 'WEIGHTS_0']:
        doc['bufferViews'][doc['accessors'][attributes[key]]['bufferView']]['target'] = 34962
    meshes.append({'name': f'boots_female_barbarian_{side.lower()}', 'primitives': [{
        'attributes': attributes, 'material': primitive['material'],
        'indices': add_accessor(doc, binary, faces_side.reshape(-1, 1).astype(np.uint32), 'SCALAR', 5125)}]})
    nodes.append({'name': f'boots_female_barbarian_{side.lower()}', 'mesh': len(meshes) - 1, 'skin': 0,
                  'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'boots_female_barbarian'}})
doc['meshes'], doc['nodes'] = meshes, nodes
ids = base['skins'][0]['joints']
mapping = {index: len(doc['nodes']) + i for i, index in enumerate(ids)}
for index in ids:
    node = copy.deepcopy(base['nodes'][index])
    if 'children' in node:
        node['children'] = [mapping[i] for i in node['children'] if i in mapping]
    doc['nodes'].append(node)
children = {child for index in ids for child in base['nodes'][index].get('children', [])}
pose_config['body_sha256'] = hashlib.sha256(BODY.read_bytes()).hexdigest()
doc['scenes'] = [{'nodes': [0, 1, len(doc['nodes'])], 'extras': {'rig_id': 'human_female_01_local_v1', 'heel_pose': pose_config}}]
doc['scene'] = 0
doc['nodes'].append({'name': 'Armature', 'children': [mapping[i] for i in ids if i not in children]})
doc['skins'] = [{'joints': [mapping[i] for i in ids], 'skeleton': mapping[base['skins'][0]['skeleton']],
                 'inverseBindMatrices': add_accessor(doc, binary, accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']), 'MAT4')}]
spec = importlib.util.spec_from_file_location('plate', ROOT / 'tools/fit-modular-plate.py')
plate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plate)
binary = plate.compact(doc, binary)
write_glb(OUTPUT / 'boots_female_barbarian.glb', doc, binary)

garment = Surface(np.vstack([right_fitted, left_fit['left_fitted']]), np.vstack([right_faces, faces[:, ::-1] + len(fitted)]),
                  np.vstack([fitted_normals, fitted_normals * [-1, 1, 1]]))
covered = np.zeros(len(bv), dtype=bool)
for i in np.flatnonzero((posed[:, 1] < .108) & (body_labels == skin_label) & (posed_normals[:, 1] < -.3)):
    covered[i] = garment.ray(posed[i], posed_normals[i], .0216) is not None
skin_points = np.flatnonzero(body_labels == skin_label)
near = cKDTree(posed[skin_points])
for i in np.flatnonzero(covered):
    if not covered[skin_points[near.query_ball_point(posed[i], .009)]].all():
        covered[i] = False
hidden = covered[bf].all(1)
mask = {'body': str(BODY.relative_to(ROOT)), 'body_sha256': pose_config['body_sha256'],
        'mesh_index': 0, 'primitive_index': 0, 'primitive_triangles': len(bf),
        'hide_triangle_indices': np.flatnonzero(hidden).tolist(), 'covered_skin_triangles': int(hidden.sum()),
        'scope': 'Reversible equipment mask on this exact body. Underside skin of the foot whose posed normal ray meets the '
                 'footbed within 0.0216 m and lies 0.009 m inside the covered area; everything else stays visible. Restore on unequip/load failure.'}
(OUTPUT / 'body-occlusion.json').write_text(json.dumps(mask, indent=2) + '\n')

np.savez(OUTPUT / 'fit-data.npz', source=source, start=start, fitted=right_fitted, faces=faces, fixed=fixed,
         rings=ring_any, buckles=np.any(buckles, axis=0), depth=final_depth, joints=right_joints, weights=right_weights,
         bone_names=np.array(names), original_vertex_count=original_vertex_count,
         **{f'wall_{kind}_{i}': ids for i, group in enumerate(wall_groups)
            for kind, ids in zip(['inner', 'outer'], group)},
         **{f'connector_anchors_{side}': anchors for side, anchors in connector_anchors.items()},
         heel_straps=heel_strap_vertices, rear_ring=rear_ring_vertices, lower_wall=lower_wall_vertices,
         lower_wall_inner=lower_wall_groups[0][0], lower_wall_outer=lower_wall_groups[0][1],
         front_wall=front_wall_vertices, front_wall_inner=front_inner, front_wall_outer=front_outer,
         front_side=front_side_vertices, right_panel=right_panel_vertices, source_right=right_source, right_faces=right_faces,
         right_panel_faces=front_side_groups[1][2],
         **{f'front_side_{kind}_{i}': ids for i, (_, inner, outer) in enumerate(front_side_groups)
            for kind, ids in [('inner', inner), ('outer', outer)]}, **left_fit)
report = {'date': '2026-10-15',
          'source': {'path': str(SOURCE.relative_to(ROOT)), 'sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest()},
          'body': {'path': str(BODY.relative_to(ROOT)), 'sha256': pose_config['body_sha256']},
          'output': str((OUTPUT / 'boots_female_barbarian.glb').relative_to(ROOT)),
          'rig_id': 'human_female_01_local_v1', 'triangles': int(len(faces) * 2), 'scale': float(scale),
          'heel_pose': {'pitch_degrees': pose_config['pitch_degrees'], 'shared_with': 'female_knight_boots', 'toe_animation_override': False},
          'shoe_yaw_degrees': {'right': float(np.rad2deg(np.arctan2(forward[0], forward[2]))), 'left': float(-np.rad2deg(np.arctan2(forward[0], forward[2])))},
          'shoe_yaw_adjustment_degrees': {'right': SHOE_YAW_ADJUSTMENT_DEGREES, 'left': -SHOE_YAW_ADJUSTMENT_DEGREES},
          'lateral_offset_m': {'right': 0, 'left': LEFT_LATERAL_OFFSET},
          'rearward_offset_m': {'right': 0, 'left': LEFT_REARWARD_OFFSET},
          'toe_lift_m': {'right': float(right_toe_lift), 'left': 0},
          'toe_animation_follow': {'right': RIGHT_TOE_FOLLOW, 'left': 0},
          'toe_profile': 'Quadratic toe lift and animation blend for a continuous, gradually increasing curve.',
          'instep_strap_lift_m': {'right': RIGHT_INSTEP_LIFT, 'left': 0},
          'instep_surface_offset_m': {'right': RIGHT_INSTEP_CLEARANCE, 'left': 0},
          'ground_source': str(HEEL_GROUND.relative_to(ROOT)),
          'posed_ground': float(ground), 'heel_height_m': float(fitted[seat, 1].min() - ground),
          'clearance_m': {'sole': SOLE_GAP, 'rings': RING_CLEARANCE, 'straps': STRAP_CLEARANCE},
          'left_front_ring_clearance_m': LEFT_FRONT_RING_CLEARANCE,
          'left_front_ring_maximum_inset_m': float(left_front_ring_inset),
          'connector_attachment_embed_m': CONNECTOR_EMBED,
          'connector_thickness_m': CONNECTOR_THICKNESS,
          'connector_outer_clearance_target_m': CONNECTOR_OUTER_CLEARANCE,
          'heel_strap_maximum_adjustment_m': heel_strap_adjustments,
          'heel_strap_straightening_strength': HEEL_STRAP_STRAIGHTENING,
          'rear_ring_width_reduction': REAR_RING_WIDTH_REDUCTION,
          'right_side_panel_repair': right_panel,
          'front_side_maximum_adjustment_m': front_side_adjustments,
          'front_strap_thickness_target_m': FRONT_STRAP_THICKNESS,
          'front_strap_maximum_lift_m': front_wall_adjustments,
          'lower_ring_wall_thickness_m': LOWER_RING_WALL_THICKNESS,
          'lower_ring_wall_maximum_adjustment_m': lower_wall_adjustments,
          'rear_ring_maximum_adjustment_m': rear_ring_adjustments,
          'ring_wall_minimum_thickness_m': {'right': RIGHT_RING_WALL_THICKNESS, 'left': RING_WALL_THICKNESS},
          'connector_maximum_adjustment_m': connector_adjustments,
          'rest_clearance': {'penetrating_vertices': int((final_depth < 0).sum()), 'minimum': float(final_depth.min())},
          'arap': {'iterations': int(arap['iterations']), 'fixed_vertices': int(fixed.sum())},
          'hidden_body_triangles': int(hidden.sum()),
          'method': 'Rotate the shoe 7 degrees outward from the Foot-to-ToeBase heading to center the heel on the footbed; fit one right sandal on the shared 30-degree heel pose: uniform scale to foot length, footbed SOLE_GAP under '
                    'the posed sole, stiletto tip on the forefoot ground, rings radially RING_CLEARANCE off the ankle, ARAP for '
                    'the straps, outward push for any strap inside the skin. Mirror for the left and move its footbed outward '
                    'by LEFT_LATERAL_OFFSET and rearward by LEFT_REARWARD_OFFSET. Fade these translations '
                    'to zero at the ankle rings and refit the front straps. Curve the right toe tip upward, capped by '
                    'sampled footbed-to-skin clearance while preserving the ball contact. Use quadratic toe lift and animation '
                    'blends for a gradual curve, and raise the right instep strap while tapering to its sole attachments. Inverse-skin both into the '
                    'canonical bind pose and store heel_pose in scene extras like the knight boots.',
          'weights': 'Transfer ankle skin weights separately on both legs and smooth within 7.2 mm. Buckles take the ring mean and the heel strap blends from Foot to the lower ring. The rear connector takes barycentric ring weights at its attachment points and interpolates them across gaps; paired inner/outer corners share weights. Toe strap influence ramps to 0.35. The right toe tip additionally blends up to 0.45 toward ToeBase to follow the original toe animation. Ball contact, heel and stiletto remain on Foot. Existing foot and toe animation evaluation is unchanged.',
          'status': 'Candidate; validation pending'}
REPORT.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({k: report[k] for k in ['scale', 'posed_ground', 'heel_height_m', 'rest_clearance', 'hidden_body_triangles', 'triangles']}))
