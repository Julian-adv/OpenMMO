"""Model paired articulated plate boots in the equipped heel pose, then return them to bind space."""

import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.interpolate import PchipInterpolator

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.female_boot_pose import heel_pose
from outfits.knight_boot_profile import cuff, leg, rim_offset, xyz
from outfits.knight_plate_surfaces import BRASS, GUNMETAL, LEATHER, STEEL, SurfaceMesh, atlas, unit

ROOT = Path(__file__).resolve().parents[1]
ITEM = ROOT / "assets/items/female_knight_boots"
OUT = ITEM / "reference_v2/fitted_v1"
BODY = ROOT / "assets/modular_human_female_01/rig_v1/female-base.glb"
RED, CLOTH, HEEL, SHOE_STEEL = 6, 7, 8, 9
FINISHES = [
    ([123, 16, 22], 103, 8),
    ([24, 27, 30], 215, 0),
    ([158, 131, 96], 84, 235, 0.45),
    ([133, 131, 126], 86, 246, 0.32),
]
FLOOR = -0.054492
pi = np.pi


def samples(n):
    return np.linspace(0, 1, n)


def add_leg(mesh, side, joints):
    def sheet(fn, nu, nv, angle, mat, bone, thickness=0.0016):
        outward = [side * np.sin(angle), 0, np.cos(angle)]
        skin = bone if callable(bone) else lambda u, v: ([joints[bone], 0, 0, 0], [1, 0, 0, 0])
        return mesh.sheet(fn, samples(nu), samples(nv), outward, mat, skin, thickness)

    def knee_weights(u, v):
        y = 0.485 + 0.195 * v
        t = np.clip((y - 0.532) / 0.097, 0, 1)
        t = t * t * (3 - 2 * t)
        return [joints["UpLeg"], joints["Leg"], 0, 0], np.stack([t, 1 - t, 0 * t, 0 * t], axis=-1).reshape(
            -1, 4
        )

    def knit(u, v):
        a = (u - 0.5) * 2 * pi
        y = 0.485 + 0.195 * v
        folds = 0.0017 * np.cos(v * pi * 15 + 0.7 * np.cos(a)) * (1 - np.cos(2 * pi * v)) * 0.5
        return leg(side, a, y, folds - 0.0005)

    sheet(knit, 19, 16, 0, CLOTH, knee_weights, 0)

    def shell(a, y, rear=False):
        ridge = 0.0048 * np.maximum(0, np.cos(a)) ** 10
        return leg(side, a, y, 0.004 + ridge - (0.0016 if rear else 0))

    for thigh in (True, False):
        bone = "UpLeg" if thigh else "Leg"

        def top(a):
            return cuff(a) if thigh else 0.532 + 0.016 * np.cos(a) - 0.006 * np.cos(2 * a)

        def hem(a):
            return (
                0.646 - 0.012 * np.cos(a)
                if thigh
                else 0.157 - 0.022 * np.cos(a) - 0.031 * np.maximum(0, np.cos(a - 1.47)) ** 6
            )

        for rear in (False, True):
            center = pi if rear else 0
            span = 1.04 if rear else 2.17

            def plate(u, v):
                a = center + (2 * u - 1) * span
                y = hem(a) + (top(a) - hem(a)) * v
                return shell(a, y, rear)

            panel = sheet(plate, 14 if rear else 18, 7 if thigh else 10, center, STEEL, bone)
            for u in (0.045, 0.955):
                for v in (0.06, 0.36, 0.70, 0.94) if thigh else (0.045, 0.23, 0.53, 0.90):
                    mesh.stud(panel, u, v, 0.0032)

        # Narrow overlapping lames follow the scooped cuff and the flared ankle.
        for start, width in ((0, 0.016), (0.018, 0.012)) if thigh else ((0, 0.014),):

            def rim(u, v):
                a = 2 * pi * (u - 0.5)
                boundary = top(a) - start if thigh else hem(a) + start
                y = boundary + (-1 if thigh else 1) * width * v
                return leg(side, a, y, rim_offset(a, v))

            anchor = sheet(rim, 29, 2, 0, STEEL, bone)
            for u in (0.10, 0.31, 0.69, 0.90):
                mesh.stud(anchor, u, 0.52, 0.0021)

        height = 0.678 if thigh else 0.477

        def strap(u, v):
            a = 0.87 + (2 * pi - 1.74) * u
            y = height + 0.005 * np.cos(a) + 0.015 * (v - 0.5)
            return leg(side, a, y, 0.008)

        band = sheet(strap, 21, 2, pi, LEATHER, bone, 0.0018)
        for u in (0.018, 0.982):
            mesh.stud(band, u, 0.5, 0.0038)

        def buckle(u, v):
            a = 1.90 + 0.46 * (u - 0.5)
            return leg(side, a, height + 0.005 * np.cos(a) + 0.020 * (v - 0.5), 0.0105)

        for x0, x1, y0, y1 in [
            (0, 1, 0, 0.13),
            (0, 1, 0.87, 1),
            (0, 0.12, 0.10, 0.90),
            (0.88, 1, 0.10, 0.90),
            (0.48, 0.57, 0.12, 0.78),
        ]:
            sheet(lambda u, v: buckle(x0 + (x1 - x0) * u, y0 + (y1 - y0) * v), 3, 2, 1.9, BRASS, bone, 0.0016)

    # Pitch the assembled poleyn about its lower edge towards the thigh.
    pitch = np.deg2rad(16)
    sin_pitch, cos_pitch = np.sin(pitch), np.cos(pitch)
    pivot_y, pivot_z = 0.548, -0.015

    def tilt_guard(points):
        x, y, z = np.moveaxis(points, -1, 0)
        dy, dz = y - pivot_y, z - pivot_z
        return xyz(x, pivot_y + cos_pitch * dy + sin_pitch * dz, pivot_z - sin_pitch * dy + cos_pitch * dz)

    # Sliding lames above and below the central knee cup leave the back open.
    for level in (-1, 1):
        for i in range(2):

            def lame(u, v):
                a = (2 * u - 1) * 1.70
                edge = 0.026 + 0.010 * np.cos(a) + i * 0.021
                y = 0.586 + level * (edge + (0.039 if i == 0 else 0.032) * v)
                p = leg(side, a, y, 0.007 + 0.011 * (1 - v) + 0.004 * np.cos(a))
                return p + xyz(0, -0.006, -0.004) if level > 0 and i == 0 else p

            sheet(lame, 17, 3, 0, STEEL, "UpLeg" if level > 0 else "Leg")

    def poleyn(u, v):
        a = (2 * u - 1) * 1.73
        t = 2 * v - 1
        y = 0.586 + t * (0.041 + 0.016 * np.cos(a)) + 0.005 * np.cos(a)
        bulge = (0.012 + 0.013 * np.cos(a)) * np.maximum(0, 1 - np.abs(t) ** 1.35)
        return tilt_guard(leg(side, a, y, 0.018 + bulge))

    shield = sheet(poleyn, 19, 7, 0, STEEL, "KneeGuard")
    for u in (0.13, 0.87):
        mesh.stud(shield, u, 0.50, 0.0036)

    for flank in (-1, 1):

        def wing(u, v):
            t = 2 * v - 1
            lead = 0.90 + 0.16 * t * t
            trail = 2.00 + 0.46 * t * t
            a = flank * (lead + (trail - lead) * u)
            y = 0.584 + t * (0.051 + 0.010 * np.sin(pi * u))
            p = leg(side, a, y, 0.014 + 0.014 * (1 - t * t) * np.sin(pi * u) + 0.008 * (1 - u))
            return tilt_guard(p + xyz(side * flank * 0.010, 0, 0.008))

        wing_anchor = sheet(wing, 9, 7, flank * pi / 2, STEEL, "KneeGuard")
        for u, v in [(0.40, 0.08), (0.40, 0.92), (0.14, 0.50), (0.88, 0.50)]:
            mesh.stud(wing_anchor, u, v, 0.0030)

    def ankle(u, v):
        a = (u - 0.5) * 2 * pi
        return leg(side, a, 0.128 + 0.053 * v, -0.014 + 0.008 * v + 0.0013 * np.sin(5 * pi * v))

    def ankle_skin(u, v):
        t = np.clip((v - 0.10) / 0.73, 0, 1)
        return [joints["Leg"], joints["Foot"], 0, 0], np.stack([t, 1 - t, 0 * t, 0 * t], axis=-1).reshape(
            -1, 4
        )

    sheet(ankle, 17, 7, 0, CLOTH, ankle_skin, 0)


# Depth, half-width, underside height, upper height, and thickness of the sole.
last = np.array(
    [
        [-0.134, 0.027, 0.105, 0.151, 0.0045],
        [-0.126, 0.033, 0.102, 0.166, 0.005],
        [-0.115, 0.036, 0.096, 0.177, 0.005],
        [-0.106, 0.037, 0.090, 0.187, 0.005],
        [-0.100, 0.038, 0.084, 0.194, 0.005],
        [-0.092, 0.039, 0.079, 0.197, 0.005],
        [-0.085, 0.040, 0.074, 0.195, 0.005],
        [-0.069, 0.040, 0.055, 0.177, 0.005],
        [-0.058, 0.039, 0.042, 0.168, 0.005],
        [-0.047, 0.038, 0.029, 0.169, 0.005],
        [-0.035, 0.037, 0.001, 0.169, 0.005],
        [-0.023, 0.035, -0.031, 0.166, 0.006],
        [-0.013, 0.035, -0.047, 0.157, 0.009],
        [-0.002, 0.036, -0.052, 0.133, 0.008],
        [0.010, 0.038, FLOOR, 0.079, 0.007],
        [0.015, 0.040, FLOOR, 0.066, 0.006],
        [0.026, 0.043, FLOOR + 0.0003, 0.031, 0.005],
        [0.043, 0.046, FLOOR + 0.0006, 0.008, 0.004],
        [0.059, 0.043, FLOOR + 0.0011, -0.008, 0.0035],
        [0.072, 0.035, FLOOR + 0.0015, -0.012, 0.0035],
    ]
)
shoe_section = PchipInterpolator(last[:, 0], last[:, 1:], axis=0)


def smooth_counter_normals(mesh, shell, facing):
    indices = mesh.faces[shell]
    triangles = mesh.attrs["POSITION"][shell][indices - indices.min()]
    centers = triangles.mean(axis=1)
    area_normals = np.cross(triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0])
    # Fair the reflection over 17 mm while keeping the fitted vertices and rolled edges.
    for patch in (shell, facing):
        offsets = mesh.attrs["POSITION"][patch][:, None] - centers
        distances_sq = np.sum(offsets**2, axis=-1)
        weights = np.exp(-distances_sq / (2 * 0.017**2))
        mesh.attrs["NORMAL"][patch] = unit(weights @ area_normals)


def add_shoe(mesh, side, foot):
    rigid = lambda u, v: ([foot, 0, 0, 0], [1, 0, 0, 0])

    def cx(z):
        return 0.243 + 0.18 * (z + 0.045)

    def surface(a, z, offset=0):
        width, ground, top, thickness = np.moveaxis(shoe_section(z), -1, 0)
        base = ground + thickness
        c = np.cos(a)
        return xyz(side * (cx(z) + (width + offset) * np.sin(a)), base + (top - base) * c + offset * c, z)

    def sheet(fn, n, m, out, mat, thick=0.0012):
        return mesh.sheet(fn, samples(n), samples(m), out, mat, rigid, thick)

    def sole_z(v):
        return last[0, 0] + (last[-1, 0] - last[0, 0]) * v

    def sole(u, z):
        width, ground, _, _ = np.moveaxis(shoe_section(z), -1, 0)
        return xyz(side * (cx(z) + (2 * u - 1) * width), ground, z)

    seat_shoulder, seat_front = -0.085, -0.065

    for lo, hi, rows, bottom_mat, edge_mat in [
        (last[0, 0], seat_shoulder, 10, RED, HEEL),
        (seat_shoulder, last[-1, 0], 26, RED, CLOTH),
    ]:
        sheet(lambda u, v: sole(u, lo + (hi - lo) * v), 2, rows, [0, -1, 0], bottom_mat, 0)
        for edge in (-1, 1):

            def welt(u, v):
                z = lo + (hi - lo) * u
                width, ground, _, thickness = np.moveaxis(shoe_section(z), -1, 0)
                return xyz(side * (cx(z) + edge * width), ground + thickness * v, z)

            sheet(welt, rows, 2, [side * edge, 0, 0], edge_mat, 0)

    sheet(lambda u, v: surface((u - 0.5) * pi, sole_z(v), -0.0026), 15, 32, [0, 1, 0], CLOTH, 0)

    heel_depth, heel_rise = -0.015, 0.006

    def cap(index, depth, rise, u, v):
        end, width, ground, top, thickness = last[index]
        base = ground + thickness
        a = (u - 0.5) * pi
        c = np.cos(a)
        phi = 0.5 * pi * v
        z = end + depth * c * np.sin(phi)
        _, dg, dt, dh = shoe_section.derivative()(end)
        slope = (dg + dh) * (1 - c) + dt * c
        y = base + c * (
            (top - base + 0.002) * np.cos(phi)
            + rise * (1 - np.cos(phi))
            + slope * depth * np.sin(phi) * np.cos(phi)
        )
        return xyz(side * (cx(z) + (width + 0.002 * np.cos(phi)) * np.sin(a)), y, z)

    dome_intervals, quarter_intervals = 8, 6
    intervals = dome_intervals + quarter_intervals

    # Instep depth, flank depth and flank clearance above the welt.
    rims = np.array([
        [-0.006, -0.084, 0.032], [0.001, -0.076, 0.018],
        [0.007, -0.066, 0.010], [0.014, -0.055, 0.004],
        [0.022, -0.043, 0.002], [0.031, -0.029, 0.0015],
        [0.041, -0.012, 0.001], [0.050, 0.006, 0.001],
        [0.058, 0.025, 0.0007], [0.065, 0.043, 0.0005],
        [0.069, 0.059, 0.0003], [0.072, 0.072, 0],
    ])
    quarter_turn = 0.55

    def rim_depth(angle, crest, flank, clearance, extension, turn):
        _, ground, _, welt = np.moveaxis(shoe_section(flank), -1, 0)
        edge_height = ground + welt + clearance
        crest_height = shoe_section(crest)[..., 2]
        # Continue the same cutting plane down the flank to the welt.
        lo = np.zeros_like(angle) - extension
        hi = np.ones_like(angle)
        for _ in range(22):
            t = (lo + hi) / 2
            z = flank + (crest - flank) * t
            _, base, crown, thickness = np.moveaxis(shoe_section(z), -1, 0)
            height = base + thickness + (crown - base - thickness) * np.cos(angle)
            past_flank = np.maximum(0, flank - z)
            turn_drop = turn * past_flank**2 / (past_flank + 0.002)
            below = height < edge_height + (crest_height - edge_height) * t - turn_drop
            hi, lo = np.where(below, t, hi), np.where(below, lo, t)
        return flank + (crest - flank) * (lo + hi) / 2

    def quarter_edge(a):
        opening = -0.064 - 0.039 * np.cos(a)
        return np.minimum(opening, rim_depth(a, *rims[0], 0.50, quarter_turn) + 0.0025)

    def quarter(u, v):
        a = (u - 0.5) * pi
        row = intervals * v - dome_intervals
        t = np.maximum(row / quarter_intervals, 0)
        z = last[0, 0] + (quarter_edge(a) - last[0, 0]) * t
        dome = cap(0, heel_depth, heel_rise, u, np.maximum(-row / dome_intervals, 0))
        return np.where((row < 0)[..., None], dome, surface(a, z, 0.0020 + 0.0023 * t))

    quarter_patch = len(mesh.faces)
    counter = sheet(quarter, 17, intervals + 1, [0, 1, 0], SHOE_STEEL)

    def facing(u, v):
        a = (u - 0.5) * pi
        z = quarter_edge(a) - 0.0018 * (1 - v)
        return surface(a, z, 0.0044 + 0.0006 * v)

    facing_patch = len(mesh.faces)
    facing_anchor = sheet(facing, 17, 3, [0, 1, 0], GUNMETAL)
    smooth_counter_normals(mesh, quarter_patch, facing_patch)
    for u in (0.11, 0.89):
        mesh.stud(counter, u, (dome_intervals + quarter_intervals * 0.26) / intervals, 0.0025)
        mesh.stud(facing_anchor, u, 0.60, 0.0028)

    for i, (upper, lower) in enumerate(zip(rims, rims[1:])):

        def sabaton(u, v):
            a = (u - 0.5) * pi
            c = np.cos(a)
            t = np.asarray(v)[..., None]
            crest, flank, clearance = np.moveaxis(upper + t * (lower - upper), -1, 0)
            offset = 0.002
            if i < len(rims) - 2:
                crest += 0.0026 * v
                flank += 0.0034 * v
                offset += (0.0038 - 0.0023 * c * c) * v * v
            _, ground, top, welt = np.moveaxis(shoe_section(flank), -1, 0)
            edge_angle = np.arccos(np.clip(clearance / (top - ground - welt), 0, 1))
            wrap = np.clip(3 - i - v, 0, 1)
            angle = a * (wrap + (1 - wrap) * edge_angle / (pi / 2))
            z = rim_depth(angle, crest, flank, clearance, 0.50 * wrap, quarter_turn * np.clip(1 - i - v, 0, 1))
            return surface(angle, z, offset)

        plate = sheet(sabaton, 21, 4, [0, 1, 0], SHOE_STEEL, 0.0015)
        if i == 0:
            for u in (0.075, 0.925):
                mesh.stud(plate, u, 0.72, 0.0017)

    for index, depth, rise in [(0, heel_depth, heel_rise), (-1, 0.023, 0.0015)]:
        end, width, ground, _, thickness = last[index]

        if index == -1:
            sheet(lambda u, v: cap(index, depth, rise, u, v), 19, 9, [0, 0.5, 1], SHOE_STEEL, 0)

        def tip_sole(u, v):
            a = (u - 0.5) * pi
            c = np.cos(a)
            z = end + depth * c * v
            return xyz(side * (cx(z) + width * np.sin(a)), ground + rise * c * v, z)

        sheet(tip_sole, 19, 2, [0, -1, 0], HEEL if index == 0 else RED, 0)

        def tip_welt(u, v):
            return tip_sole(u, 1) + xyz(0, thickness * v, 0)

        sheet(tip_welt, 19, 3, [0, 0, np.sign(depth)], HEEL if index == 0 else CLOTH, 0)

    for flank in (-1, 1):

        def side_strap(u, v, offset=0.0075):
            z = -0.082 + 0.061 * u
            y = 0.085 + 0.050 * u + 0.0095 * (v - 0.5)
            _, ground, top, thick = np.moveaxis(shoe_section(z), -1, 0)
            a = flank * np.arccos(np.clip((y - ground - thick) / (top - ground - thick), -0.99, 0.99))
            return surface(a, z, offset)

        strap = sheet(side_strap, 14, 3, [side * flank, 0.25, 0], LEATHER)
        for u in (0.05, 0.95):
            mesh.stud(strap, u, 0.5, 0.0024)
        if flank == 1:

            def buckle(u, v):
                theta = 2 * pi * u
                c, s = np.cos(theta), np.sin(theta)
                z = np.sign(c) * np.abs(c) ** 0.48
                y = np.sign(s) * np.abs(s) ** 0.48
                scale = 0.76 + 0.24 * v
                return side_strap(
                    0.15 + 0.074 * z * scale, 0.50 + 0.76 * y * scale, 0.0092 + 0.0009 * np.sin(pi * v)
                )

            sheet(buckle, 25, 3, [side, 0.25, 0], BRASS, 0.0012)
            sheet(
                lambda u, v: side_strap(0.14 + 0.020 * u, 0.50 + 0.60 * v, 0.0098),
                2,
                3,
                [side, 0.25, 0],
                BRASS,
            )

    # Heel sections flare into the sloped seat. The inner edge eases tangentially into the arch.
    levels = [0, 0.035, 0.12, 0.45, 0.70, 0.83, 0.92, 0.975, 1]
    heel = PchipInterpolator(
        levels,
        [
            [0.0058, -0.1082, -0.0968],
            [0.0058, -0.1082, -0.0968],
            [0.0049, -0.1082, -0.0975],
            [0.0054, -0.1090, -0.0971],
            [0.0069, -0.1180, -0.0950],
            [0.011, -0.1310, -0.0900],
            [0.017, -0.1410, -0.082],
            [0.024, -0.1475, -0.074],
            [0.029, last[0, 0] + heel_depth, seat_front],
        ],
        axis=0,
    )

    def heelwall(u, v):
        a = (u - 0.5) * 2 * pi
        width, rear, front = np.moveaxis(heel(v), -1, 0)
        z = (front + rear) / 2 + (front - rear) * 0.5 * np.cos(a)
        seat = (
            np.where(
                z < last[0, 0],
                last[0, 2] + (z - last[0, 0]) * heel_rise / heel_depth,
                shoe_section(z)[..., 1],
            )
            + 0.00045
        )
        end, end_width = last[0, :2]
        seat_rear = end + heel_depth
        seat_z = seat_rear + (seat_front - seat_rear) * 0.5 * (1 + np.cos(a))
        seat_width = shoe_section(np.maximum(seat_z, end))[..., 0]
        rear_curve = np.clip((seat_z - end) / heel_depth, 0, 1)
        seat_width = np.where(seat_z < end, end_width * np.sqrt(1 - rear_curve**2), seat_width)
        breast = np.clip((seat_z - seat_shoulder) / (seat_front - seat_shoulder), 0, 1)
        seat_width *= np.sqrt(1 - breast**2)
        flare = np.clip((v - 0.65) / 0.35, 0, 1)
        flare = flare**2 * (3 - 2 * flare)
        x = width * np.sin(a) * (1 - flare) + np.sign(np.sin(a)) * seat_width * flare
        h = v + v**5 * (1 - v)
        return xyz(side * (cx(z) + x), FLOOR + (seat - FLOOR) * h, z)

    mesh.sheet(
        heelwall, samples(17), np.unique(np.r_[samples(12), levels, 0.945, 0.990]), [0, 0, 1], HEEL, rigid, 0
    )

    def heel_tip(u, v):
        p = heelwall(u, 0)
        center = xyz(side * cx(-0.1025), FLOOR, -0.1025)
        return center + (p - center) * np.asarray(v)[..., None]

    sheet(heel_tip, 19, 2, [0, -1, 0], CLOTH, 0)
    sheet(
        lambda u, v: (
            heelwall(u, 0.034 * v)
            + xyz(side * 0.0003 * np.sin((u - 0.5) * 2 * pi), 0, 0.0003 * np.cos((u - 0.5) * 2 * pi))
        ),
        19,
        2,
        [0, 0, 1],
        CLOTH,
        0,
    )


def build():
    body, raw = read_glb(BODY)
    doc, template = read_glb(ITEM / "fitted_v5/boots_female_knight.glb")
    deform, pose = heel_pose(body, raw)
    body_hash = hashlib.sha256(BODY.read_bytes()).hexdigest()
    doc["scenes"][0]["extras"]["heel_pose"] = dict(pose, body_sha256=body_hash)
    names = [doc["nodes"][n]["name"] for n in doc["skins"][0]["joints"]]
    bind = accessor(doc, template, doc["skins"][0]["inverseBindMatrices"])
    skin_joints = doc["skins"][0]["joints"]
    shin_indices = [names.index(side + "Leg") for side in ("Left", "Right")]
    for shin_index in shin_indices:
        shin = doc["nodes"][skin_joints[shin_index]]
        side = shin["name"][:-3]
        name = side + "KneeGuard"
        node = dict(name=name, translation=shin["translation"], rotation=shin["rotation"])
        doc["nodes"][skin_joints[names.index(side + "UpLeg")]]["children"].append(len(doc["nodes"]))
        skin_joints.append(len(doc["nodes"]))
        doc["nodes"].append(node)
        names.append(name)
    bind = np.concatenate((bind, bind[shin_indices]))
    deform = np.concatenate((deform, deform[shin_indices]))
    data = bytearray()
    doc.update(bufferViews=[], accessors=[])
    count = 0
    for i, (side, prefix) in enumerate([(-1, "Right"), (1, "Left")]):
        mesh = SurfaceMesh(6 + len(FINISHES))
        joints = {s: names.index(prefix + s) for s in ("UpLeg", "Leg", "Foot", "KneeGuard")}
        add_leg(mesh, side, joints)
        add_shoe(mesh, side, joints["Foot"])
        values = {k: np.concatenate(v) for k, v in mesh.attrs.items()}
        skin = np.einsum("nk,nkij->nij", values["WEIGHTS_0"], deform[values["JOINTS_0"].astype(int)])
        homogeneous = np.c_[values["POSITION"], np.ones(mesh.vertices)]
        values["POSITION"] = np.linalg.solve(skin, homogeneous[..., None])[:, :3, 0]
        values["NORMAL"] = unit(np.einsum("nji,nj->ni", skin[:, :3, :3], values["NORMAL"]))
        widths = {
            "POSITION": "VEC3",
            "NORMAL": "VEC3",
            "TEXCOORD_0": "VEC2",
            "JOINTS_0": "VEC4",
            "WEIGHTS_0": "VEC4",
        }
        attrs = {
            k: add_accessor(doc, data, v, widths[k], 5123 if k == "JOINTS_0" else 5126)
            for k, v in values.items()
        }
        assert mesh.vertices < 65536
        idx = add_accessor(doc, data, np.concatenate(mesh.faces).ravel(), "SCALAR", 5123)
        doc["meshes"][i]["primitives"] = [dict(attributes=attrs, indices=idx, material=0)]
        count += mesh.triangles

    doc["skins"][0]["inverseBindMatrices"] = add_accessor(doc, data, bind, "MAT4")
    doc["images"], doc["textures"] = [], []
    for title, png in zip(["Plate and leather", "Roughness and metalness"], atlas(FINISHES)):
        data.extend(bytes(-len(data) % 4))
        doc["images"].append(dict(name=title, mimeType="image/png", bufferView=len(doc["bufferViews"])))
        doc["textures"].append(dict(source=len(doc["textures"])))
        doc["bufferViews"].append(dict(buffer=0, byteOffset=len(data), byteLength=len(png)))
        data.extend(png)
    doc["materials"] = [
        dict(
            name="Plate, leather and oxblood sole",
            doubleSided=True,
            pbrMetallicRoughness=dict(baseColorTexture=dict(index=0), metallicRoughnessTexture=dict(index=1)),
        )
    ]
    doc["buffers"] = [dict(byteLength=len(data))]
    OUT.mkdir(exist_ok=True, parents=True)
    write_glb(OUT / "boots_female_knight.glb", doc, data)
    body_mesh = body["meshes"][0]["primitives"][0]
    positions = accessor(body, raw, body_mesh["attributes"]["POSITION"])
    faces = accessor(body, raw, body_mesh["indices"]).reshape(-1, 3)
    attributes = body_mesh["attributes"]
    joints = accessor(body, raw, attributes["JOINTS_0"])
    weights = accessor(body, raw, attributes["WEIGHTS_0"])
    dominant = joints[np.arange(len(joints)), weights.argmax(axis=1)]
    body_joints = [body["nodes"][i]["name"] for i in body["skins"][0]["joints"]]
    angles = np.linspace(-pi, pi, 128, endpoint=False)
    hide = []
    clip_planes = []
    for side in (-1, 1):
        ring = leg(side, angles, cuff(angles))
        center = ring.mean(axis=0)
        slopes = np.linalg.lstsq((ring - center)[:, [0, 2]], ring[:, 1] - center[1], rcond=None)[0]
        normal = unit(np.array([-slopes[0], 1, -slopes[1]]))
        point = center + normal * (np.min((ring - center) @ normal) - 0.002)
        below = ((positions - point) @ normal < 0)[faces]
        prefix = "Left" if side > 0 else "Right"
        leg_joints = [body_joints.index(prefix + name) for name in ("UpLeg", "Leg", "Foot", "ToeBase")]
        on_leg = np.isin(dominant, leg_joints)[faces].all(axis=1)
        hide.extend(np.flatnonzero(on_leg & below.all(axis=1)).tolist())
        crossing = np.flatnonzero(on_leg & below.any(axis=1) & ~below.all(axis=1))
        clip_planes.append(dict(point=point.tolist(), normal=normal.tolist(), triangle_indices=crossing.tolist()))
    mask = dict(
        mesh_index=0,
        primitive_index=0,
        primitive_triangles=len(faces),
        hide_triangle_indices=sorted(hide),
        clip_planes=clip_planes,
        source_sha256=body_hash,
        description="Clip each thigh on a mirrored plane tucked inside the sloping metal cuff.",
    )
    (OUT / "body-occlusion.json").write_text(json.dumps(mask, indent=2) + "\n")
    print(f"{count} triangles, {len(hide)} masked body triangles")


if __name__ == "__main__":
    build()
