"""Fit a bald Tripo head (--face) to the female base rig with one neck seam.

Reuses the seam, weight and neck colour steps of fit-female-custom-face.py.
The head only gets a rotation, a uniform scale and a translation.
"""
import argparse
import copy
import io
import importlib.util
import json
from pathlib import Path

import numpy as np
import trimesh

from PIL import Image
from scipy.ndimage import binary_dilation, gaussian_filter1d
from scipy.spatial import cKDTree

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('custom_face', ROOT / 'tools/fit-female-custom-face.py')
custom = importlib.util.module_from_spec(spec)
spec.loader.exec_module(custom)

BASE = custom.BASE
# Per face: uniform scale and offset of the head, and the frontal projection its retouched eye images were
# made in (Tripo frame, +X front; see iris_cleanup_v1/sources.json): scale, size, centre y, centre z per side.
FACES = {
    # Chin 4 mm above the base face's (1.578 m), which every collar was fitted to; the crown ends 3 mm below the base head's.
    'ranger': {'date': '2026-11-10', 'scale': 0.293, 'offset': [-0.002, 1.6506, -0.008],
               'iris': (4600, (640, 400), .1155, {'L': .0928, 'R': -.107})},
    'chic': {'date': '2026-11-11', 'folder': 'tripo_chic_v2', 'scale': 0.293, 'offset': [0, 1.6506, 0.008],
             'iris': (4600, (640, 400), .1195, {'L': .0967, 'R': -.1006}), 'retouched_irises': False,
             'lashes_on_lids': True, 'ease_seam_weights': True},
}
FACE = OUT = SOURCE = SCALE = OFFSET = IRIS = IRIS_SCALE = IRIS_SIZE = IRIS_Y = IRIS_Z = None


def configure(face):
    global FACE, OUT, SOURCE, SCALE, OFFSET, IRIS, IRIS_SCALE, IRIS_SIZE, IRIS_Y, IRIS_Z
    FACE = face
    OUT = custom.OUT = ROOT / f"assets/modular_human_female_01/faces/{FACES[face].get('folder', f'tripo_{face}_v1')}"
    SOURCE, IRIS = OUT / 'model_url.glb', OUT / 'iris_cleanup_v1'
    SCALE, OFFSET = FACES[face]['scale'], np.array(FACES[face]['offset'])
    IRIS_SCALE, IRIS_SIZE, IRIS_Y, IRIS_Z = FACES[face]['iris']


NECK_Y = 1.552
# The body's colour and ORM maps matched to this face's skin (tools/match-female-body-skin.py).
BODY_SKIN = ROOT / 'assets/modular_human_female_01/rig_v1/skin_tone_v1'
LASH_SOURCE = ROOT / 'assets/items/female_ranger_outfit/tripo_head_v1/model_url.glb'
BLEND_BAND = 0.05
SMOOTH_BELOW, SMOOTH_ABOVE = 0.04, 0.02
SMOOTH_RADIUS = 0.064
SMOOTH_SIGMA = 0.008
NECK_AXIS_Z = -0.03
BLEND_HALF_WIDTH = 0.10


def material_image(doc, raw, texture):
    image = doc['images'][doc['textures'][texture['index']]['source']]
    return np.asarray(Image.open(io.BytesIO(view_bytes(doc, raw, image['bufferView']))).convert('RGB'), dtype=np.float32) / 255


def ring_angles(points, start=None):
    """Turn of each ring point around the neck axis, as a fraction of a full turn from the first point."""
    angle = np.arctan2(points[:, 0], points[:, 2] - NECK_AXIS_Z) / (2 * np.pi)
    step = np.diff(np.unwrap(angle * 2 * np.pi)) / (2 * np.pi)
    assert np.all(step > 0) or np.all(step < 0), 'Neck ring must wind once around the axis'
    turn = (angle - (angle[0] if start is None else start)) * (1 if step[0] > 0 else -1)
    return turn % 1, angle[0]


def angular_seam(body, body_faces, head, head_faces):
    """custom.direct_seam with ring points paired by angle around the neck instead of by arc length.

    Pairing by arc length slides ring vertices sideways and moves each body vertex by its nearest ring
    segment, which creases the neck. Paired by angle, every body vertex moves only along its own radius,
    by the ring's radius change at its angle.
    """
    lower, upper = custom.ring(body, body_faces, NECK_Y), custom.ring(head, head_faces, NECK_Y)
    low_turn, start = ring_angles(body['POSITION'][lower])
    high_turn, _ = ring_angles(head['POSITION'][upper], start)
    shift = int(np.argmin(high_turn))
    upper, high_turn = np.roll(upper, -shift), np.roll(high_turn, -shift)
    bottom, top = body['POSITION'][lower].copy(), head['POSITION'][upper].copy()
    a, b = np.r_[low_turn, 1], np.r_[high_turn, 1 + high_turn[0]]
    assert np.all(np.diff(a) > 0) and np.all(np.diff(b) > 0)
    targets = np.unique(np.round(np.r_[low_turn, high_turn], 12))
    wrapped = np.sort(np.where(targets < high_turn[0], targets + 1, targets))
    body, body_faces = custom.refine_ring(body, body_faces, lower, a, np.r_[targets, 1], NECK_Y)
    head, head_faces = custom.refine_ring(head, head_faces, upper, b, wrapped, NECK_Y)
    canonical = custom.sample(bottom, a, targets)
    destination = np.column_stack([np.interp(np.where(targets < high_turn[0], targets + 1, targets), b,
                                             np.r_[top[:, i], top[0, i]]) for i in range(3)])
    original = body['POSITION'].copy()
    offset = original - [0, 0, NECK_AXIS_Z]
    radius = np.hypot(offset[:, 0], offset[:, 2])
    turn = ((np.arctan2(offset[:, 0], offset[:, 2]) / (2 * np.pi) - start)
            * (1 if low_turn[1] < .5 else -1)) % 1
    grow = np.hypot(destination[:, 0], destination[:, 2] - NECK_AXIS_Z) - np.hypot(canonical[:, 0], canonical[:, 2] - NECK_AXIS_Z)
    change = np.interp(turn, np.r_[targets, 1], np.r_[grow, grow[0]])
    fade = custom.smoothstep((original[:, 1] - NECK_Y + .05) / .05) * (1 - custom.smoothstep((radius - .07) / .03))
    outward = np.stack([offset[:, 0], np.zeros(len(original)), offset[:, 2]], axis=1) / np.maximum(radius, 1e-9)[:, None]
    body['POSITION'] = original + outward * (change * fade)[:, None]
    selected = fade > 0
    for old, new in zip(np.round(canonical, 8), destination):
        body['POSITION'][np.all(np.round(original, 8) == old, axis=1)] = new
    body['NORMAL'][selected] = custom.smooth_normals(body['POSITION'], body_faces)[selected]
    head_positions = np.round(head['POSITION'], 8)
    for point in destination:
        lower_ids = np.flatnonzero(np.all(np.round(body['POSITION'], 8) == np.round(point, 8), axis=1))
        upper_ids = np.flatnonzero(np.all(np.abs(head['POSITION'] - point) < 2e-8, axis=1))
        assert len(lower_ids) and len(upper_ids), 'The seam must share every boundary vertex'
        head['POSITION'][upper_ids] = body['POSITION'][lower_ids[0]]
        head['dense'][upper_ids] = body['dense'][lower_ids[0]]
        body['NORMAL'][lower_ids] = custom.unit(head['NORMAL'][upper_ids[:1]])
    return body, body_faces, head, head_faces, len(destination), float(np.linalg.norm(body['POSITION'] - original, axis=1).max() * 1000)


def ease_seam_weights(head, reach=.025):
    """Blend head weights just above the seam from the ring's own, so a slim neck has no weight step there.

    Weights sampled from the nearest body surface jump where the head's neck stands off the body's.
    """
    points = head['POSITION']
    turn = np.arctan2(points[:, 0], points[:, 2] - NECK_AXIS_Z)
    ring = np.flatnonzero(np.abs(points[:, 1] - NECK_Y) < 1e-7)
    order = ring[np.argsort(turn[ring])]
    near = np.flatnonzero((points[:, 1] > NECK_Y + 1e-7) & (points[:, 1] < NECK_Y + reach))
    anchor = np.column_stack([np.interp(turn[near], turn[order], head['dense'][order, joint], period=2 * np.pi)
                              for joint in range(head['dense'].shape[1])])
    keep = custom.smoothstep((points[near, 1] - NECK_Y) / reach)[:, None]
    mixed = anchor * (1 - keep) + head['dense'][near] * keep
    head['dense'][near] = mixed / mixed.sum(1, keepdims=True)
    return len(near)


def smooth_seam(body, body_faces, head, head_faces):
    """Smooth the neck radius along its height in a band around the seam, so the outline has no ridge there.

    Vertices move only towards or away from the neck axis. The weight is 1 on the seam and 0 at the band ends.
    """
    points = np.concatenate([body['POSITION'], head['POSITION']])
    faces = np.concatenate([body_faces, head_faces + len(body['POSITION'])])
    axis = np.array([0, 0, NECK_AXIS_Z])
    mesh = trimesh.Trimesh(points, faces, process=False)
    heights = np.arange(NECK_Y - SMOOTH_BELOW - .03, NECK_Y + SMOOTH_ABOVE + .03, .002)
    angles = np.radians(np.arange(0, 360, 3))
    directions = np.stack([np.sin(angles), np.zeros_like(angles), np.cos(angles)], axis=1)
    origins = np.repeat(axis[None] + np.outer(heights, [0, 1, 0]), len(angles), axis=0)
    rays = np.tile(directions, (len(heights), 1))
    hits, ray_ids, _ = mesh.ray.intersects_location(origins, rays, multiple_hits=True)
    field = np.zeros(len(origins))
    np.maximum.at(field, ray_ids, np.einsum('ij,ij->i', hits - origins[ray_ids], rays[ray_ids]))
    field = field.reshape(len(heights), len(angles))
    column_grid = ((field > 0) & (field < SMOOTH_RADIUS + .008)).astype(np.float64)
    sigma = SMOOTH_SIGMA / .002
    blurred = gaussian_filter1d(field * column_grid, sigma, axis=0, mode='nearest')
    blurred /= np.maximum(gaussian_filter1d(column_grid, sigma, axis=0, mode='nearest'), 1e-6)
    change = np.where(column_grid > 0, blurred - field, 0)
    change = gaussian_filter1d(change, 1.5, axis=1, mode='wrap')

    offset = points - axis
    radius = np.hypot(offset[:, 0], offset[:, 2])
    turn = np.degrees(np.arctan2(offset[:, 0], offset[:, 2])) % 360
    # Only the neck column: the shoulders and the underside of the jaw stay as they are.
    column = 1 - custom.smoothstep((radius - SMOOTH_RADIUS) / .008)

    def band(y):
        return np.where(y < NECK_Y, custom.smoothstep((y - NECK_Y + SMOOTH_BELOW) / SMOOTH_BELOW),
                        custom.smoothstep((NECK_Y + SMOOTH_ABOVE - y) / SMOOTH_ABOVE))

    def at(grid, turn, y):
        """A (height, angle) grid sampled at angles in degrees and heights."""
        row = np.clip((y - heights[0]) / .002, 0, len(heights) - 1.001)
        col = (turn % 360) / 3
        r0, c0 = np.floor(row).astype(int), np.floor(col).astype(int) % len(angles)
        fr, fc, c1 = row - r0, col - np.floor(col), (c0 + 1) % len(angles)
        return ((grid[r0, c0] * (1 - fc) + grid[r0, c1] * fc) * (1 - fr) +
                (grid[r0 + 1, c0] * (1 - fc) + grid[r0 + 1, c1] * fc) * fr)

    y = points[:, 1]
    weight = band(y) * column
    delta = at(change, turn, y) * weight
    outward = np.stack([offset[:, 0], np.zeros(len(points)), offset[:, 2]], axis=1) / np.maximum(radius, 1e-9)[:, None]
    moved = points + outward * delta[:, None]
    # Normals in the band come from the smoothed radius field itself. Rebuilding them from the faces gave every
    # vertex added on a straight ring edge that edge's flat normal (a row of teeth along the seam); keeping the
    # old ones left the seam lit as if its ridge were still there (a pale band above the seam).
    smooth = gaussian_filter1d(np.where(column_grid > 0, blurred, np.nan_to_num(blurred)), 3, axis=1, mode='wrap')
    rise = at(np.gradient(smooth, .002, axis=0), turn, y)
    sweep = at(np.gradient(smooth, np.radians(3), axis=1), turn, y) / np.maximum(at(smooth, turn, y), 1e-3)
    around = np.stack([outward[:, 2], np.zeros(len(points)), -outward[:, 0]], axis=1)
    surface = custom.unit(outward - rise[:, None] * [0, 1, 0] - sweep[:, None] * around)
    old = np.concatenate([body['NORMAL'], head['NORMAL']])
    mixed = custom.unit(old * (1 - weight[:, None]) + surface * weight[:, None])
    split = len(body['POSITION'])
    body['POSITION'], head['POSITION'] = moved[:split], moved[split:]
    body['NORMAL'], head['NORMAL'] = mixed[:split], mixed[split:]
    return {'band_m': [NECK_Y - SMOOTH_BELOW, NECK_Y + SMOOTH_ABOVE], 'height_sigma_m': SMOOTH_SIGMA,
            'vertices': int(np.count_nonzero(weight > 0)),
            'body_maximum_mm': float(np.abs(delta[:split]).max() * 1000),
            'head_maximum_mm': float(np.abs(delta[split:]).max() * 1000)}


def iris_circle(image):
    """Pupil centre (row, column) and iris radius in pixels of a retouched frontal eye image."""
    step = image.shape[1] / IRIS_SIZE[0]
    dark = np.argwhere(image.max(2) < .16)
    centre = dark[np.linalg.norm(dark - np.median(dark, axis=0), axis=1) < 60 * step].mean(0)
    row = image[int(round(centre[0]))]
    white = (row.mean(1) > .6) & (row.max(1) - row.min(1) < .25)
    left, right = np.flatnonzero(white[:int(centre[1])]), np.flatnonzero(white[int(centre[1]):])
    radius = float(np.mean([centre[1] - left.max(), right.min()]))
    assert .12 < radius / image.shape[1] < .22, radius
    return centre, radius


def fitted_iris_circle(image, visible):
    """Centre (row, column) and radius of the iris of a generated eye, from its unlit front view.

    The iris is measured where it ends against the white of the eye on both sides; the lids hide its top and bottom.
    """
    white = visible & (image.mean(2) >= .45)
    dark = np.argwhere(visible & (image.max(2) < .16))
    seed = np.median(dark, axis=0).astype(int)
    chords = []
    for row in range(image.shape[0]):
        left, right = np.flatnonzero(white[row, :seed[1]]), np.flatnonzero(white[row, seed[1]:])
        if len(left) and len(right) and seed[1] + right.min() - left.max() > .18 * image.shape[1]:
            chords.append([row, left.max() + .5, seed[1] + right.min() - .5])
    rows, left, right = np.array(chords).T
    column = np.median((left + right) / 2)
    widest = (right - left).max() / 2
    row = np.median(dark[np.abs(dark[:, 1] - column) < .3 * widest, 0])
    radius = float(np.median(np.hypot((right - left) / 2, rows - row)))
    assert .1 < radius / image.shape[1] < .22, radius
    return np.array([row, column]), radius


def clean_iris(side):
    """The retouched iris as a (radius, angle) image, with angles the eyelid hid filled from the nearest visible one."""
    retouched = np.asarray(Image.open(IRIS / f'retouched-{side}.png').convert('RGB'), dtype=np.float32) / 255
    visible = np.asarray(Image.open(IRIS / f'visible-{side}.png')) == 255
    step = retouched.shape[1] / IRIS_SIZE[0]
    centre, radius = iris_circle(retouched)
    radii, angles = np.arange(0, int(radius * 1.08)), np.radians(np.arange(360))
    y = centre[0] - np.outer(radii, np.sin(angles))
    x = centre[1] + np.outer(radii, np.cos(angles))
    polar = custom.texture_sample(retouched, np.stack([x.ravel() / (retouched.shape[1] - 1), y.ravel() / (retouched.shape[0] - 1)], 1)).reshape(len(radii), 360, 3)
    # The retouched frame is the front image scaled up with 13 padded rows on top.
    front_row = np.clip(np.round(y / step - 13).astype(int), 0, IRIS_SIZE[1] - 1)
    front_col = np.clip(np.round(x / step).astype(int), 0, IRIS_SIZE[0] - 1)
    seen = visible[front_row, front_col]
    for r in range(len(radii)):
        good = np.flatnonzero(seen[r])
        if not len(good) or len(good) == 360:
            continue
        gap = np.abs((np.arange(360)[:, None] - good[None] + 180) % 360 - 180)
        polar[r] = polar[r, good[np.argmin(gap, axis=1)]]
    return polar, centre / step - [13, 0], radius / step


def iris_record(centre, radius, side, **more):
    z = IRIS_Z[side] + (centre[1] - IRIS_SIZE[0] / 2) / IRIS_SCALE
    y = IRIS_Y + (IRIS_SIZE[1] / 2 - centre[0]) / IRIS_SCALE
    return {'iris_centre_front_px': centre[::-1].tolist(), 'iris_radius_front_px': radius, **more,
            'iris_centre_fitted_m': [-z * SCALE + OFFSET[0], y * SCALE + OFFSET[1]],
            'iris_radius_fitted_m': radius / IRIS_SCALE * SCALE}


def repair_irises(points, faces, uvs, image):
    """Paint the retouched irises onto the front of both eyeballs; the rest of the colour map is untouched.

    A face whose generated irises need no retouch only has them measured (tools/project-female-face-eyes.py).
    """
    pixels = image.copy()
    height, width = pixels.shape[:2]
    balls, _, ball_faces = eye_parts(points, faces, 200)
    record = {}
    for side, sign in (('L', 1), ('R', -1)):
        if not FACES[FACE].get('retouched_irises', True):
            front = np.asarray(Image.open(IRIS / f'front-{side}.png').convert('RGB'), dtype=np.float32) / 255
            visible = np.asarray(Image.open(IRIS / f'visible-{side}.png')) == 255
            record[side] = iris_record(*fitted_iris_circle(front, visible), side)
            continue
        polar, centre, radius = clean_iris(side)
        ball = balls[sign]
        painted = 0
        for triangle in faces[ball_faces[sign]]:
            corners = points[triangle]
            uv = uvs[triangle] * [width - 1, height - 1]
            lo = np.maximum(np.floor(uv.min(0)).astype(int) - 1, 0)
            hi = np.minimum(np.ceil(uv.max(0)).astype(int) + 1, [width - 1, height - 1])
            x, y = np.meshgrid(np.arange(lo[0], hi[0] + 1), np.arange(lo[1], hi[1] + 1))
            samples = np.column_stack([x.ravel(), y.ravel()])
            matrix = np.column_stack([uv[1] - uv[0], uv[2] - uv[0]])
            if abs(np.linalg.det(matrix)) < 1e-10:
                continue
            weights = (samples - uv[0]) @ np.linalg.inv(matrix).T
            bary = np.column_stack([1 - weights.sum(1), weights])
            # One texel of margin keeps bilinear filtering inside the repainted area.
            inside = np.all(bary >= -.6 / max(np.ptp(uv, axis=0).max(), 1), axis=1)
            world = bary[inside] @ corners
            col = (world[:, 2] - IRIS_Z[side]) * IRIS_SCALE + IRIS_SIZE[0] / 2
            row = IRIS_SIZE[1] / 2 - (world[:, 1] - IRIS_Y) * IRIS_SCALE
            reach = np.hypot(row - centre[0], col - centre[1])
            front = world[:, 0] > ball[:, 0].min() + np.ptp(ball[:, 0]) * .45
            keep = front & (reach < radius * 1.06)
            if not keep.any():
                continue
            turn = np.degrees(np.arctan2(centre[0] - row[keep], col[keep] - centre[1])) % 360
            scaled = reach[keep] / radius * (radius * (polar.shape[0] / (radius * 1.08)))
            r0 = np.clip(np.floor(scaled).astype(int), 0, polar.shape[0] - 2)
            a0 = np.floor(turn).astype(int) % 360
            fr, fa = (scaled - r0)[:, None], (turn - np.floor(turn))[:, None]
            a1 = (a0 + 1) % 360
            color = ((polar[r0, a0] * (1 - fa) + polar[r0, a1] * fa) * (1 - fr) +
                     (polar[r0 + 1, a0] * (1 - fa) + polar[r0 + 1, a1] * fa) * fr)
            fade = (1 - custom.smoothstep((reach[keep] / radius - .97) / .08))[:, None]
            px, py = samples[inside][keep].T
            pixels[py, px] = pixels[py, px] * (1 - fade) + color * fade
            painted += int(keep.sum())
        record[side] = iris_record(centre, radius, side, texels=painted)
    Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / 'face-eye-albedo.png')
    return record


def bake_body_neck(body, faces, source, source_faces, body_image, head_image, name):
    """custom.bake_body_neck with a wider band: the bald source reaches the collarbones."""
    pixels = body_image.copy()
    image_height, image_width = pixels.shape[:2]
    low_y, reach = NECK_Y - BLEND_BAND, BLEND_HALF_WIDTH + .03
    coordinates, positions = [], []
    for triangle in faces:
        points = body['POSITION'][triangle]
        if points[:, 1].max() < low_y or np.abs(points[:, 0]).min() > reach:
            continue
        uv = body['TEXCOORD_0'][triangle] * [image_width - 1, image_height - 1]
        low = np.maximum(np.floor(uv.min(0)).astype(int), 0)
        high = np.minimum(np.ceil(uv.max(0)).astype(int), [image_width - 1, image_height - 1])
        x, y = np.meshgrid(np.arange(low[0], high[0] + 1), np.arange(low[1], high[1] + 1))
        samples = np.column_stack([x.ravel(), y.ravel()])
        matrix = np.column_stack([uv[1] - uv[0], uv[2] - uv[0]])
        if abs(np.linalg.det(matrix)) < 1e-10:
            continue
        weights = (samples - uv[0]) @ np.linalg.inv(matrix).T
        bary = np.column_stack([1 - weights.sum(1), weights])
        inside = np.all(bary >= -1e-6, axis=1)
        world = bary[inside] @ points
        selected = (world[:, 1] >= low_y) & (np.abs(world[:, 0]) < reach)
        coordinates.append(samples[inside][selected])
        positions.append(world[selected])
    coordinates, positions = np.concatenate(coordinates), np.concatenate(positions)
    color = custom.texture_sample(head_image, custom.surface_sample(positions, source, source_faces, 'TEXCOORD_0'))
    fade = custom.smoothstep((positions[:, 1] - low_y) / BLEND_BAND)
    fade *= 1 - custom.smoothstep((np.abs(positions[:, 0]) - BLEND_HALF_WIDTH) / .03)
    x, y = coordinates.T
    pixels[y, x] = pixels[y, x] * (1 - fade[:, None]) + color * fade[:, None]
    painted = np.zeros(pixels.shape[:2], dtype=bool)
    painted[y, x] = True
    padding = np.column_stack(np.nonzero(binary_dilation(painted, iterations=3) & ~painted))
    filled = np.column_stack(np.nonzero(painted))
    nearest = cKDTree(filled).query(padding)[1]
    pixels[padding[:, 0], padding[:, 1]] = pixels[filled[nearest, 0], filled[nearest, 1]]
    Image.fromarray(np.round(np.clip(pixels, 0, 1) * 255).astype(np.uint8)).save(OUT / name)
    return int(len(np.unique(coordinates, axis=0)))


def components(points, faces):
    mesh = trimesh.Trimesh(points, faces, process=False)
    mesh.merge_vertices()
    groups = list(trimesh.graph.connected_components(mesh.face_adjacency, nodes=np.arange(len(faces))))
    return [(group, points[faces[group]].reshape(-1, 3)) for group in groups]


def eye_parts(points, faces, largest):
    """Loose pieces around the eyes in Tripo's frame (+X front, Z sideways): eyeballs and lashes per side."""
    balls, ball_faces, lashes = {}, {}, {1: [], -1: []}
    for group, corners in components(points, faces):
        low, high = corners.min(0), corners.max(0)
        if low[0] < .1 or low[1] < .04 or high[1] > .22 or np.abs(corners[:, 2]).max() > .22:
            continue
        side = 1 if corners[:, 2].mean() > 0 else -1
        if len(group) <= largest:
            lashes[side].append(group)
        elif len(group) < 1000:
            balls[side], ball_faces[side] = corners, group
    empty = np.zeros(0, dtype=int)
    return balls, {side: np.concatenate(groups) if groups else empty for side, groups in lashes.items()}, ball_faces


def lid_margins(points, faces, skip, ball):
    """Upper and lower rims of one eye opening seen from the front, as (z, y, x) rows in Tripo's frame."""
    keep = np.ones(len(faces), dtype=bool)
    keep[skip] = False
    eye = np.zeros(len(faces), dtype=bool)
    eye[ball] = True
    mesh = trimesh.Trimesh(points, faces[keep], process=False)
    corners = points[faces[ball]].reshape(-1, 3)
    zs = np.arange(corners[:, 2].min() - .03, corners[:, 2].max() + .03, .001)
    ys = np.arange(corners[:, 1].min() - .01, corners[:, 1].max() + .01, .0004)
    z, y = [grid.ravel() for grid in np.meshgrid(zs, ys, indexing='ij')]
    origins = np.column_stack([np.ones_like(z), y, z])
    hits, rays, triangles = mesh.ray.intersects_location(origins, np.tile([-1., 0, 0], (len(z), 1)), multiple_hits=False)
    depth = np.full(len(z), np.nan)
    depth[rays] = hits[:, 0]
    open_ = np.zeros(len(z), dtype=bool)
    open_[rays] = eye[keep][triangles]
    depth, open_ = depth.reshape(len(zs), len(ys)), open_.reshape(len(zs), len(ys))
    upper, lower = [], []
    for column in np.flatnonzero(open_.sum(1) > 2):
        seen = np.flatnonzero(open_[column])
        top, bottom = min(seen[-1] + 1, len(ys) - 1), max(seen[0] - 1, 0)
        upper.append([zs[column], ys[top], depth[column, top]])
        lower.append([zs[column], ys[bottom], depth[column, bottom]])
    return np.array(upper), np.array(lower)


def lid_point(rim, fraction):
    """A rim point and the rim's slope there, by fraction of the opening's width."""
    z = rim[0, 0] + np.clip(fraction, 0, 1) * (rim[-1, 0] - rim[0, 0])
    smooth = np.column_stack([rim[:, 0], gaussian_filter1d(rim[:, 1], 3), gaussian_filter1d(rim[:, 2], 3)])
    point = np.array([np.interp(z, smooth[:, 0], smooth[:, 2]), np.interp(z, smooth[:, 0], smooth[:, 1]), z])
    return point, float(np.interp(z, smooth[:, 0], np.gradient(smooth[:, 1], smooth[:, 0])))


def lashes_on_lids(points, faces, donor_lashes, donor_balls, donor_skip, head_points, head_faces, own_lashes, own_balls):
    """Each donor strand moved onto this head's lid rim: one rotation in the face plane, a uniform scale and a translation.

    A strand keeps its place along the opening's width and its scaled offset from the rim.
    """
    moved, record = points.copy(), {}
    donor_surface = np.ones(len(faces), dtype=bool)
    donor_surface[donor_skip] = False
    skin = trimesh.Trimesh(points, faces[donor_surface], process=False)
    for side in (1, -1):
        source = lid_margins(points, faces, donor_skip, donor_balls[side])
        target = lid_margins(head_points, head_faces, own_lashes, own_balls[side])
        scale = float(np.ptp(target[0][:, 0]) / np.ptp(source[0][:, 0]))
        strands = [group for group, _ in components(points, faces[donor_lashes[side]])]
        for strand in strands:
            ids = np.unique(faces[donor_lashes[side]][strand])
            root = points[ids[np.argmin(trimesh.proximity.closest_point(skin, points[ids])[1])]]
            fraction = (root[2] - source[0][0, 0]) / np.ptp(source[0][:, 0])
            rim = int(np.argmin([abs(lid_point(source[k], fraction)[0][1] - root[1]) for k in (0, 1)]))
            (start, slope), (end, new_slope) = lid_point(source[rim], fraction), lid_point(target[rim], fraction)
            angle = np.arctan(new_slope) - np.arctan(slope)
            cos, sin = np.cos(angle), np.sin(angle)
            offset = points[ids] - start
            turned = np.column_stack([offset[:, 0], offset[:, 1] * cos + offset[:, 2] * sin, offset[:, 2] * cos - offset[:, 1] * sin])
            moved[ids] = end + turned * scale
        record['left' if side > 0 else 'right'] = {'strands': len(strands), 'uniform_scale': scale}
    return moved, record


def transplant_lashes(head_points, head_faces):
    """Strand lashes of the first head, moved per eye by one uniform scale and a translation.

    Returns the bald head's own lash faces to drop, and the strand geometry in the bald head's frame.
    """
    donor, donor_raw = read_glb(LASH_SOURCE)
    attrs, faces = custom.geometry(donor, donor_raw, donor['meshes'][0]['primitives'][0])
    points = attrs['POSITION'].astype(np.float64)
    donor_balls, donor_lashes, _ = eye_parts(points, faces, 16)
    own_balls, own_lashes, own_ball_faces = eye_parts(head_points, head_faces, 200)
    if FACES[FACE].get('lashes_on_lids'):
        _, _, donor_ball_faces = eye_parts(points, faces, 16)
        loose = np.concatenate([group for group, corners in components(points, faces) if len(group) <= 16])
        fitted, lid_record = lashes_on_lids(points, faces, donor_lashes, donor_ball_faces, loose,
                                            head_points, head_faces, np.concatenate(list(own_lashes.values())), own_ball_faces)
    scale = float(np.mean([np.ptp(own_balls[side][:, 2]) / np.ptp(donor_balls[side][:, 2]) for side in (1, -1)]))
    surface = np.ones(len(head_faces), dtype=bool)
    surface[np.concatenate(list(own_lashes.values()))] = False
    skin = trimesh.Trimesh(head_points, head_faces[surface], process=False)
    moved, picked, record = points.copy(), [], {'uniform_scale': scale}
    for side in (1, -1):
        anchor = lambda ball: np.array([ball[:, 0].max(), ball[:, 1].min() + np.ptp(ball[:, 1]) / 2,
                                        ball[:, 2].min() + np.ptp(ball[:, 2]) / 2])
        source, target = anchor(donor_balls[side]), anchor(own_balls[side])
        ids = np.unique(faces[donor_lashes[side]])
        moved[ids] = (points[ids] - source) * scale + target
        if FACES[FACE].get('lashes_on_lids'):
            moved[ids] = fitted[ids]
        picked.append(donor_lashes[side])
        distance = trimesh.proximity.closest_point(skin, moved[ids])[1]
        record['left' if side > 0 else 'right'] = {
            'strand_triangles': int(len(donor_lashes[side])), 'replaced_own_triangles': int(len(own_lashes[side])),
            **(lid_record['left' if side > 0 else 'right'] if FACES[FACE].get('lashes_on_lids')
               else {'translation_source_units': (target - source * scale).tolist()}),
            'nearest_skin_distance_mm': {'min': float(distance.min() * SCALE * 1000),
                                         'median': float(np.median(distance) * SCALE * 1000),
                                         'max': float(distance.max() * SCALE * 1000)}}
    used, lash_faces = np.unique(faces[np.concatenate(picked)], return_inverse=True)
    lash = {'POSITION': moved[used], 'NORMAL': attrs['NORMAL'][used].astype(np.float64),
            'TEXCOORD_0': attrs['TEXCOORD_0'][used].astype(np.float64)}
    return surface, lash, lash_faces.reshape(-1, 3), (donor, donor_raw), record


def main():
    base, base_raw = read_glb(BASE)
    source, source_raw = read_glb(SOURCE)
    original_body, body_faces = custom.geometry(base, base_raw, base['meshes'][0]['primitives'][0])
    source_attrs, source_faces = custom.geometry(source, source_raw, source['meshes'][0]['primitives'][0])
    source_attrs = {key: source_attrs[key].astype(np.float64) for key in ['POSITION', 'NORMAL', 'TEXCOORD_0']}
    surface, lash, lash_faces, (donor, donor_raw), lash_record = transplant_lashes(source_attrs['POSITION'], source_faces)
    own_lashes = FACES[FACE].get('own_lashes', False)
    if own_lashes:
        lash_record = {'method': 'The generated strand lashes are kept as part of the head'}
    else:
        source_faces = source_faces[surface]
    head_color = source['materials'][0]['pbrMetallicRoughness']['baseColorTexture']
    # UV1.x marks the eyeballs, so the game's eye colour tint cannot reach the eyelids in front of them.
    eyeball = np.zeros((len(source_attrs['POSITION']), 2))
    for group in eye_parts(source_attrs['POSITION'], source_faces, 200)[2].values():
        eyeball[np.unique(source_faces[group]), 0] = 1
    source_attrs['TEXCOORD_1'] = eyeball
    irises = repair_irises(source_attrs['POSITION'], source_faces, source_attrs['TEXCOORD_0'],
                                material_image(source, source_raw, head_color))
    # Tripo's face looks along +X; the rig looks along +Z.
    turn = lambda v: np.stack([-v[:, 2], v[:, 1], v[:, 0]], axis=1)
    for part in (source_attrs, lash):
        part['POSITION'] = turn(part['POSITION']) * SCALE + OFFSET
        part['NORMAL'] = turn(part['NORMAL'])
    source_attrs['reference'] = source_attrs['POSITION'].copy()
    head, head_faces = custom.clip(source_attrs, source_faces, NECK_Y, True)
    head['dense'] = custom.surface_sample(head['POSITION'], original_body, body_faces, 'dense')
    head_joint = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']].index('Head')
    blend = custom.smoothstep((head['POSITION'][:, 1] - 1.59) / .02)
    head['dense'] *= 1 - blend[:, None]
    head['dense'][:, head_joint] += blend
    body, kept_faces = custom.clip(original_body, body_faces, NECK_Y, False)
    body, kept_faces, head, head_faces, seam_count, body_displacement = angular_seam(body, kept_faces, head, head_faces)
    assert float(np.linalg.norm(head['POSITION'] - head['reference'], axis=1).max()) < 1e-7
    eased = ease_seam_weights(head) if FACES[FACE].get('ease_seam_weights') else 0
    smoothing = smooth_seam(body, kept_faces, head, head_faces)
    reference = head.pop('reference')
    untouched = reference[:, 1] > NECK_Y + SMOOTH_ABOVE
    affine_error = float(np.linalg.norm(head['POSITION'] - reference, axis=1)[untouched].max())
    assert affine_error < 1e-7
    body_material = base['meshes'][0]['primitives'][0]['material']
    head_maps = source['materials'][0]
    recolored = bake_body_neck(
        body, kept_faces, source_attrs, source_faces,
        np.asarray(Image.open(BODY_SKIN / 'body-albedo.png'), dtype=np.float32) / 255,
        np.asarray(Image.open(OUT / 'face-eye-albedo.png'), dtype=np.float32) / 255, 'body-neck-albedo.png')
    bake_body_neck(
        body, kept_faces, source_attrs, source_faces,
        np.asarray(Image.open(BODY_SKIN / 'body-orm.png'), dtype=np.float32) / 255,
        material_image(source, source_raw, head_maps['pbrMetallicRoughness']['metallicRoughnessTexture']), 'body-neck-orm.png')

    doc, raw = copy.deepcopy(base), bytearray(base_raw)
    face_material = custom.add_material(doc, raw, source, source_raw)
    doc['materials'][face_material]['name'] = f'Female {FACE} face'
    image_data = (OUT / 'face-eye-albedo.png').read_bytes()
    raw.extend(b'\0' * (-len(raw) % 4))
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
    raw.extend(image_data)
    doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': 'Face with retouched irises'})
    doc['textures'].append({'source': len(doc['images']) - 1})
    doc['materials'][face_material]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    fitted = doc['materials'][body_material]
    for name, slot in [('body-neck-albedo.png', fitted['pbrMetallicRoughness']['baseColorTexture']),
                       ('body-neck-orm.png', fitted['pbrMetallicRoughness']['metallicRoughnessTexture'])]:
        image_data = (OUT / name).read_bytes()
        raw.extend(b'\0' * (-len(raw) % 4))
        doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
        raw.extend(image_data)
        doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': 'image/png', 'name': name})
        doc['textures'].append({'source': len(doc['images']) - 1})
        slot['index'] = len(doc['textures']) - 1
    # The strands are a few dark texels each in the donor's 4096 atlas; one flat colour stands in for it.
    donor_color = material_image(donor, donor_raw, donor['materials'][0]['pbrMetallicRoughness']['baseColorTexture'])
    samples = custom.texture_sample(donor_color, lash['TEXCOORD_0'])
    # Strand corners also catch skin texels, so only the darker half counts.
    dark = samples.sum(1) <= np.median(samples.sum(1))
    lash_color = samples[dark].mean(0) ** 2.2
    lash_record['flat_linear_color'] = lash_color.tolist()
    doc['materials'].append({'name': f'Female {FACE} eyelashes', 'doubleSided': True,
                             'pbrMetallicRoughness': {'baseColorFactor': [*lash_color.tolist(), 1],
                                                      'metallicFactor': 0, 'roughnessFactor': .9}})
    lash_material = len(doc['materials']) - 1
    del lash['TEXCOORD_0']
    lash['dense'] = np.zeros((len(lash['POSITION']), head['dense'].shape[1]))
    lash['dense'][:, head_joint] = 1
    primitives = [custom.add_geometry(doc, raw, body, kept_faces, body_material),
                  custom.add_geometry(doc, raw, head, head_faces, face_material)]
    doc['meshes'] = [{'name': 'female_body', 'primitives': [primitives[0]]},
                     {'name': f'face_female_{FACE}', 'primitives': [primitives[1]]}]
    if not own_lashes:
        doc['meshes'].append({'name': f'eyelashes_female_{FACE}',
                              'primitives': [custom.add_geometry(doc, raw, lash, lash_faces, lash_material)]})
        doc['nodes'].append({'name': f'eyelashes_female_{FACE}', 'mesh': 2, 'skin': 0,
                             'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': f'eyelashes_female_{FACE}'}})
        doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    doc['nodes'][0]['mesh'] = 0
    doc['nodes'].append({'name': f'face_female_{FACE}', 'mesh': 1, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': f'face_female_{FACE}'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    assembled = OUT / f'base_{FACE}.glb'
    write_glb(assembled, doc, custom.compact(doc, raw))
    check, check_raw = read_glb(assembled)
    assert check['nodes'][:len(base['nodes'])] == base['nodes']
    assert np.array_equal(accessor(base, base_raw, base['skins'][0]['inverseBindMatrices']),
                          accessor(check, check_raw, check['skins'][0]['inverseBindMatrices']))
    report = {'date': FACES[FACE]['date'], 'source_path': str(SOURCE.relative_to(ROOT)), 'source_sha256': custom.digest(SOURCE),
              'base_sha256': custom.digest(BASE), 'rotation': 'Tripo +X front to rig +Z front',
              'scale': SCALE, 'offset_m': OFFSET.tolist(), 'neck_cut_y_m': NECK_Y,
              'head_affine_scope': 'Rotation, uniform scale and translation above the seam smoothing band',
              'maximum_head_affine_error_m': affine_error, 'seam_smoothing': smoothing,
              'source_triangles': len(source_faces), 'head_triangles': len(head_faces),
              'retained_body_triangles': len(kept_faces), 'neck_boundary_vertices': seam_count,
              'body_neck_fit_maximum_mm': body_displacement, **({'seam_weight_eased_vertices': eased} if eased else {}), 'body_neck_recolored_pixels': recolored,
              'rig_id': 'human_female_01_local_v1',
              'irises': {**({'sources': str((IRIS / 'sources.json').relative_to(ROOT)),
                             'method': 'Retouched frontal iris images painted onto the front of each eyeball; other texels unchanged'}
                            if FACES[FACE].get('retouched_irises', True) else
                            {'method': 'Generated irises kept; circles measured on the unlit front views in iris_cleanup_v1'}),
                         **irises},
              'eyelashes': lash_record if own_lashes else {
                  'source_path': str(LASH_SOURCE.relative_to(ROOT)), 'source_sha256': custom.digest(LASH_SOURCE),
                  'method': 'Strand lashes of the first head replace the bald head\'s lash plates'
                  + (', each strand moved onto the lid rim' if FACES[FACE].get('lashes_on_lids') else ''), **lash_record},
              'files': {name: custom.digest(OUT / name) for name in [f'base_{FACE}.glb', 'face-eye-albedo.png', 'body-neck-albedo.png', 'body-neck-orm.png']}}
    (OUT / 'fitting.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--face', choices=list(FACES), default='ranger')
    configure(parser.parse_args().face)
    main()
