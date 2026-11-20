"""Model the knight's lower harness from the supplied front three-quarter artwork."""

import json
from pathlib import Path

import numpy as np
from scipy.interpolate import PchipInterpolator
from scipy.spatial.transform import Rotation, Slerp

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.knight_boot_profile import cuff, leg, rim_offset
from outfits.knight_plate_surfaces import SurfaceMesh, atlas, STEEL, GUNMETAL, BRASS, LEATHER, LINING

ROOT = Path(__file__).resolve().parents[1]
ITEM = ROOT / "assets/items/female_knight_bottom"
OUT = ITEM / "reference_v1/fitted_v1"
lin = np.linspace


def xyz(x, y, z):
    return np.stack(np.broadcast_arrays(x, y, z), axis=-1)


def contour(values):
    return PchipInterpolator([0.79, 0.87, 0.95, 1.01, 1.07, 1.12, 1.16], values)


# Outside line of the tassets, from thigh to natural waist, in metres.
outer = PchipInterpolator(
    [0.79, 0.89, 0.95, 1.00, 1.045, 1.085, 1.12, 1.15, 1.175],
    [0.216, 0.213, 0.211, 0.204, 0.189, 0.166, 0.144, 0.116, 0.094],
)
# The hips tilt in the fitting stance; the lower edges settle over the two thighs.
midline = PchipInterpolator(
    [0.79, 0.89, 0.96, 1.02, 1.075, 1.12, 1.175], [0.013, 0.012, 0.006, -0.004, -0.006, -0.003, 0.0]
)
inner = contour([0.130, 0.113, 0.091, 0.076, 0.059, 0.061, 0.063])
front = contour([0.094, 0.099, 0.101, 0.108, 0.108, 0.100, 0.091])
rear = contour([-0.126, -0.137, -0.164, -0.156, -0.121, -0.085, -0.064])


def shell(side, theta, y, offset=0, leading=None):
    s, c = np.sin(theta), np.cos(theta)
    lip = inner(y) if leading is None else np.where(c > 0, leading, inner(y))
    x = lip + (outer(y) - lip) * s
    z = 0.5 * ((front(y) + rear(y)) + (front(y) - rear(y)) * c)
    return xyz(side * (x + offset * s) + midline(y) * s**2, y, z + offset * c)


def rigid(joint):
    return lambda u, v: ([joint, 0, 0, 0], [1, 0, 0, 0])


def thigh_bind_transforms(base, data):
    # Return each rigid thigh plate from the fitting stance to its bind pose.
    clip = next(clip for clip in base["animations"] if clip["name"] == "idle1")
    nodes = base["nodes"]
    by_name = {node["name"]: i for i, node in enumerate(nodes)}
    skin = base["skins"][0]
    bind = np.linalg.inv(
        accessor(base, data, skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
    )
    hip = Rotation.from_matrix(bind[skin["joints"].index(by_name["Hips"]), :3, :3])
    transforms = {}
    for name in ["LeftUpLeg", "RightUpLeg"]:
        index = by_name[name]
        channel = next(c for c in clip["channels"] if c["target"] == {"node": index, "path": "rotation"})
        sampler = clip["samplers"][channel["sampler"]]
        times = accessor(base, data, sampler["input"]).ravel()
        keys = Rotation.from_quat(accessor(base, data, sampler["output"]))
        stance = Slerp(times, keys)(times[-1] * 0.18)
        rotation = hip * Rotation.from_quat(nodes[index]["rotation"]) * stance.inv() * hip.inv()
        transforms[name] = rotation.as_matrix(), bind[skin["joints"].index(index), :3, 3]
    return transforms


def add_leather_underwear(mesh, base, data, mask, bones):
    primitive = base["meshes"][mask["mesh_index"]]["primitives"][mask["primitive_index"]]
    faces = accessor(base, data, primitive["indices"]).reshape(-1, 3)[mask["hide_triangle_indices"]]
    vertices, indices = np.unique(faces.ravel(), return_inverse=True)
    attrs = {key: accessor(base, data, primitive["attributes"][key])[vertices] for key in mesh.attrs}
    joints = np.array([bones.index(base["nodes"][node]["name"]) for node in base["skins"][0]["joints"]])
    mesh.append(attrs["POSITION"], attrs["NORMAL"], attrs["TEXCOORD_0"], indices.reshape(-1, 3), LEATHER,
                joints[attrs["JOINTS_0"]], attrs["WEIGHTS_0"])


def fit_waist(mesh, start, bones, thigh_bind, front_guard):
    def ramp(t):
        t = np.clip(t, 0, 1)
        return t * t * (3 - 2 * t)

    def taper(p):
        x, y, z = p.T
        front = 0.22 * ramp((y - 1.025) / 0.081)
        back = 0.27 * ramp((y - 1.069) / 0.084)
        depth = z - 0.010
        factor = np.where(depth > 0, front, back)
        return xyz(x, y, z - depth * factor * (1 - ramp((abs(x) - 0.035) / 0.086)))

    # Fit the belt and its attachments together in the fitting stance.
    transforms = np.tile(np.eye(4), (len(bones), 1, 1))
    for name, (rotation, pivot) in thigh_bind.items():
        transform = transforms[bones.index(name)]
        transform[:3, :3] = rotation.T
        transform[:3, 3] = pivot - rotation.T @ pivot
    for i in range(start, len(mesh.attrs["POSITION"])):
        p = mesh.attrs["POSITION"][i]
        if i in front_guard:
            # A single shear preserves the curved face of the front guard.
            p[:, 2] -= 0.122 * (p[:, 1] - 0.900)
            normal = mesh.attrs["NORMAL"][i]
            normal[:, 1] += 0.122 * normal[:, 2]
            normal /= np.linalg.norm(normal, axis=1, keepdims=True)
            continue
        if p[:, 1].max() < 1.0:
            continue
        joints = mesh.attrs["JOINTS_0"][i].astype(int)
        weights = mesh.attrs["WEIGHTS_0"][i]
        pose = np.einsum("nk,nkij->nij", weights, transforms[joints])
        linear = pose[:, :3, :3]
        posed = (linear @ p[..., None])[..., 0] + pose[:, :3, 3]
        # Preserve the surface lighting under the same smooth change of depth.
        axes = np.eye(3) * 0.00005
        jacobian = np.stack([(taper(posed + axis) - taper(posed - axis)) / 0.0001 for axis in axes], axis=1)
        posed_normal = np.linalg.solve(linear.transpose(0, 2, 1), mesh.attrs["NORMAL"][i][..., None])
        normal = (linear.transpose(0, 2, 1) @ np.linalg.solve(jacobian, posed_normal))[..., 0]
        mesh.attrs["NORMAL"][i] = normal / np.linalg.norm(normal, axis=1, keepdims=True)
        mesh.attrs["POSITION"][i] = p + np.linalg.solve(linear, (taper(posed) - posed)[..., None])[..., 0]


def build(mesh, bones, thigh_bind):
    harness_start = len(mesh.attrs["POSITION"])
    hips = bones.index("Hips")
    front_rails = {}

    def weighted(side, y):
        leg = bones.index(("Left" if side > 0 else "Right") + "UpLeg")
        t = np.clip((1.098 - y) / 0.172, 0, 1).ravel()
        t = t * t * (3 - 2 * t)
        return [hips, leg, 0, 0], np.stack([1 - t, t, 0 * t, 0 * t], axis=-1)

    def suspension(side, back):
        top, end = (1.146, 0.805) if back else (1.128, 0.843)

        def harness_strap(u, v):
            y = top + (end - top) * v + 0.007 * (2 * u - 1) ** 2 * (v**12 - 0.6 * (1 - v) ** 14)
            x = (0.068 + 0.085 * v if back else 0.064 + 0.038 * v) + (u - 0.5) * (0.020 - 0.003 * v)
            if back:
                z = rear(y) - 0.019
            else:
                z = np.interp(
                    y, [0.84, 0.895, 0.96, 1.02, 1.080, 1.14], [0.104, 0.109, 0.115, 0.122, 0.140, 0.123]
                )
            z += (-1 if back else 1) * 0.0009 * (1 - (2 * u - 1) ** 2)
            return xyz(side * x, y, z)

        thigh_name = ("Left" if side > 0 else "Right") + "UpLeg"
        thigh = bones.index(thigh_name)
        rotation, pivot = thigh_bind[thigh_name]

        def skin(u, v):
            return weighted(side, top + (end - top) * v)

        stations = np.array([0, 0.04, 0.13, 0.24, 0.90, 0.955, 0.98, 1])
        if not back:
            # The front lip follows the outside edge of the suspender in the fitting stance.
            rail = harness_strap(0.78, stations)
            weight = skin(0.78, stations)[1][:, 1, None]
            posed = rail + weight * ((rail - pivot) @ rotation + pivot - rail)
            front_rails[side] = PchipInterpolator(posed[::-1, 1], side * posed[::-1, 0])

        def belt_attachment(u):
            if back:
                arc = 0.315 - (u - 0.5) * 0.060
                along = arc if side > 0 else 1 - arc
                band = back_belt
                center = 0.315 if side > 0 else 0.685
            else:
                along = 0.51 + (u - 0.5) * 0.084

                def band(a, b):
                    return cross_belt(side, a, b)

                center = 0.51
            top_y = band(center, 0.65)[..., 1] - 0.002 * (2 * u - 1)**2
            lower, upper = band(along, 0), band(along, 1)
            across = (top_y - lower[..., 1]) / (upper[..., 1] - lower[..., 1])
            p = band(along, across)
            p[..., 2] += -0.003 if back else 0.003
            return p

        def rear_plate(u, y):
            angle = np.pi - 0.18 - (u - 0.5) * 0.018 / (outer(y) - inner(y))
            layer = np.interp(y, [0.94, 1.0, 1.045, 1.08, 1.11], [0, 1, 2, 3, 4])
            return shell(side, angle, y, 0.0034 + 0.0011 * layer)

        def rear_strap(u, v, upper_piece):
            y = top + (end - top) * v
            plate = rear_plate(u, np.clip(y, 0.918, 1.126))
            if upper_piece:
                plate[..., 2] -= 0.0015
                anchor = belt_attachment(u)
                return blend_hip(plate, anchor, (y - 1.113) / (top - 1.113))
            plate = (plate - pivot) @ rotation.T + pivot

            angle = np.pi - 0.31 - (u - 0.5) * 0.19
            lip = cuff(angle)
            tip_height = lip - 0.016 + 0.007 * (2 * u - 1)**2
            tip = leg(side, angle, tip_height, rim_offset(angle, 0.5) + 0.001)
            edge = (rear_plate(u, 0.918) - pivot) @ rotation.T + pivot
            t = np.clip((0.918 - y) / (0.918 - end), 0, 1)
            span = edge + (tip - edge) * np.asarray(t)[..., None]
            cuff_point = leg(side, angle, np.minimum(lip, span[..., 1]), rim_offset(angle, 0.5) + 0.001)
            cuff_point[..., 1] = span[..., 1]
            span = blend_hip(span, cuff_point, (v - 0.90) / 0.055)
            return np.where(np.asarray(y < 0.918)[..., None], span, plate)

        def front_strap(u, v):
            p = harness_strap(u, v)
            p += (belt_attachment(u) - harness_strap(u, 0)) * np.clip(1 - v / 0.28, 0, 1)[..., None]
            angle = 0.12 + (u - 0.5) * 0.19
            tip_height = cuff(angle) - 0.024 + 0.007 * (2 * u - 1)**2
            offset = rim_offset(angle, 0.5) + 0.001
            blend = np.clip((v - 0.53) / 0.39, 0, 1)
            blend = blend * blend * (3 - 2 * blend)
            y = p[..., 1] + (tip_height - harness_strap(u, 1)[..., 1]) * v * blend
            target = leg(side, angle, y, offset)
            return p + (target - p) * np.asarray(blend)[..., None]

        if back:
            def upper_strap(u, v):
                return rear_strap(u, v, True)

            def lower_strap(u, v):
                return rear_strap(u, v, False)

            lower = mesh.sheet(
                lower_strap, lin(0, 1, 5),
                [0.202, 0.24, 0.28, 0.31, 0.39, 0.47, 0.54, 0.60, 0.64, 0.67, 0.73, 0.82, 0.90, 0.94, 0.955, 0.98, 1],
                [0, 0, -1], LEATHER, rigid(thigh), 0.0018,
            )
            upper = mesh.sheet(
                upper_strap, lin(0, 1, 5),
                [0, 0.035, 0.07, 0.10, 0.13, 0.18, 0.215, 0.24, 0.272],
                [0, 0, -1], LEATHER, rigid(hips), 0.0018,
            )
            # A flexible tongue joins the sliding ends when the thigh swings outward.
            def tongue(u, v):
                a = upper_strap(u, 0.248)
                b = lower_strap(u, 0.206)
                return a + (b - a) * np.asarray(v)[..., None]

            def tongue_skin(u, v):
                return [hips, thigh, 0, 0], np.stack([1 - v, v, 0 * v, 0 * v], axis=-1).reshape(-1, 4)

            mesh.sheet(tongue, lin(0, 1, 4), [0, 1], [0, 0, -1], LEATHER, tongue_skin, 0.001)
        else:
            upper_strap = lower_strap = front_strap
            upper = lower = mesh.sheet(
                front_strap, lin(0, 1, 5), np.sort(np.r_[stations, 0.57, 0.73, 0.83]),
                [0, 0, 1], LEATHER, skin, 0.0018,
            )
        for v in [0.048, 0.13, 0.215]:
            mesh.stud(upper, 0.5, v, 0.0018)
        mesh.stud(lower, 0.5, 0.973, 0.0031)

        # The brass chape follows the rounded end of the leather at the boot.
        def chape(u, v):
            theta = -0.12 + (np.pi + 0.24) * u
            x = 0.5 + (0.51 + 0.065 * v) * np.cos(theta)
            y = 0.945 + (0.048 + 0.006 * v) * np.sin(theta)
            p = lower_strap(x, y)
            p[..., 2] += (-1 if back else 1) * 0.0018
            return p

        mesh.sheet(
            chape,
            lin(0, 1, 14),
            [0, 1],
            [0, 0, -1 if back else 1],
            BRASS,
            rigid(thigh),
            0.001,
        )
        # Thin brass keeper folded across the long strap.
        for v0, strap, joint in [(0.24, upper_strap, hips), (0.94 if back else 0.91, lower_strap, thigh)]:

            def keeper(u, v):
                p = strap(-0.085 + 1.17 * u, v0 + 0.015 * v)
                p[..., 2] += (-1 if back else 1) * 0.0016
                return p

            mesh.sheet(
                keeper,
                lin(0, 1, 5),
                [0, 1],
                [0, 0, -1 if back else 1],
                BRASS,
                rigid(joint) if back else lambda u, v: skin(u, v0 + 0.015 * v),
                0.001,
            )

    def waist_surface(side, angle, v):
        y = 1.120 + 0.044 * np.sin(angle) + 0.021 * (v - 0.5)
        plate = shell(side, angle, y, 0.0082)
        # Roll the upper rim onto the bare waist above the overlap with the steel.
        a = np.pi / 2 + 0.69 * (angle - np.pi / 2)
        waist_radius = np.interp(y, [1.12, 1.14, 1.16, 1.18], [0.125, 0.116, 0.105, 0.094])
        center_z = np.interp(y, [1.12, 1.16, 1.18], [0.010, 0.017, 0.019])
        depth = np.where(np.cos(a) > 0, 0.097 - center_z, center_z + 0.052)
        waist = xyz(side * waist_radius * np.sin(a), y, center_z + depth * np.cos(a))
        return blend_hip(plate, waist, (v - 0.52) / 0.48)

    def blend_hip(center, hip, t):
        t = np.clip(t, 0, 1)
        return center + (hip - center) * (t * t * (3 - 2 * t))[..., None]

    def back_belt(u, v):
        theta = np.pi * u
        arc = np.minimum(theta, np.pi - theta)
        y = 1.164 - 0.025 * np.sin(theta) + 0.021 * (v - 0.5)
        radius_x = np.interp(y, [1.12, 1.14, 1.16, 1.18], [0.139, 0.126, 0.113, 0.101])
        radius_z = np.interp(y, [1.12, 1.14, 1.16, 1.18], [0.119, 0.100, 0.082, 0.076])
        center = xyz(radius_x * np.cos(theta), y, 0.012 - radius_z * np.sin(theta))
        hip = waist_surface(np.where(theta < np.pi / 2, 1, -1), np.pi / 2 + arc, v)
        return blend_hip(hip, center, arc / 0.72)

    def cross_belt(side, u, v):
        t = -0.432 + (np.pi / 2 + 0.432) * u
        y = 1.095 + 0.069 * np.sin(t) + 0.021 * (v - 0.5)
        radius = outer(y) + 0.0025
        z = 0.014 + (0.119 + 0.003 * side) * np.cos(t)
        z += 0.006 * np.exp(-(t / 0.40)**4)
        z += 0.013 * np.exp(-((t - 0.87) / 0.28)**2) * (1 - v)
        center = xyz(side * np.sin(t) * radius, y, z)
        hip = waist_surface(side, t, v)
        return blend_hip(hip, center, (np.pi / 2 - t) / 0.75)

    for side in [-1, 1]:
        band = mesh.sheet(
            lambda u, v: cross_belt(side, u, v), lin(0, 1, 25), lin(0, 1, 4), [side * 0.4, 0, 1], LEATHER, rigid(hips), 0.002
        )
        for u in [0.055, 0.215, 0.51, 0.79]:
            mesh.stud(band, u, 0.5, 0.0021)
        for u in [0.10, 0.145, 0.61, 0.66]:
            mesh.stud(band, u, 0.48, 0.00135, LINING)
        suspension(side, False)
        suspension(side, True)

    mesh.sheet(back_belt, lin(0, 1, 27), lin(0, 1, 4), [0, 0, -1], LEATHER, rigid(hips), 0.002)

    def belt_boss(u, v):
        return cross_belt(1, 0.432 / (np.pi / 2 + 0.432), 0.5) + [0, 0, 0.001], np.array([0, 0, 1]), *rigid(hips)(u, v)

    mesh.stud(belt_boss, 0, 0, 0.0037)

    saddle_y = PchipInterpolator(
        [0, 0.22, 0.36, 0.51, 0.68, 0.84, 1], [0.969, 0.940, 0.928, 0.914, 0.905, 0.934, 0.978]
    )
    saddle_z = PchipInterpolator(
        [0, 0.22, 0.36, 0.51, 0.68, 0.84, 1], [0.092, 0.077, 0.069, 0.045, -0.032, -0.108, -0.150]
    )

    def saddle(u, v):
        a = 2 * u - 1
        width = 0.022 + 0.014 * (2 * v - 1) ** 2
        return xyz(a * width, saddle_y(v) + 0.018 + 0.006 * a * a, saddle_z(v))

    mesh.sheet(saddle, lin(0, 1, 6), lin(0, 1, 20), [0, -1, 0], LINING, rigid(hips), 0.002)
    for side in [-1, 1]:
        for t in [0.245, 0.605]:

            def hanger(u, v):
                angle = np.pi * (t + (u - 0.5) * 0.053 + 0.036 * v)
                rise = np.sin(angle)
                top = 1.118 + 0.044 * rise
                end = 1.053 + 0.035 * rise
                y = (1 - v) * top + v * end + 0.003 * (2 * u - 1) ** 2 * v**8
                lip = front_rails[side](np.clip(y, 0.88, 1.125))
                return shell(side, angle, y, 0.0072 + 0.0012 * v, leading=lip)

            anchor = mesh.sheet(
                hanger, lin(0, 1, 4), lin(0, 1, 10), [side, 0, 0], LEATHER, rigid(hips), 0.0016
            )
            for v in [0.12, 0.46, 0.89]:
                mesh.stud(anchor, 0.5, v, 0.0025)
    retained = mesh.triangles
    panels = {}

    def record(name, first, bone):
        panels[name] = dict(first_triangle=first, triangles=mesh.triangles - first, bone=bone)

    chin = PchipInterpolator(
        [0.905, 0.918, 0.940, 0.966, 1.00, 1.035, 1.09], [0.043, 0.065, 0.083, 0.099, 0.112, 0.120, 0.115]
    )
    for back in [False, True]:
        start = mesh.triangles
        guard_start = len(mesh.attrs["POSITION"])
        for cap in [False, True]:

            def guard(u, v):
                a = 2 * u - 1
                if cap:
                    y = (1 - v) * (1.114 - 0.027 * a * a) + v * (1.038 + 0.027 * a * a)
                    width = 0.073 - 0.003 * v
                else:
                    y = (1 - v) * (1.079 - 0.013 * a * a) + v * (0.926 + 0.008 * a * a)
                    width = 0.073 - 0.048 * np.maximum(v, 0) ** 1.26
                if back:
                    y = 0.949 + (y - 0.908) * 0.90
                    z = np.interp(
                        y,
                        [0.94, 0.975, 1.02, 1.065, 1.11, 1.15],
                        [-0.155, -0.164, -0.156, -0.129, -0.095, -0.077],
                    )
                    z -= 0.011 * (1 - a * a) + (0.016 if cap else 0)
                else:
                    z = chin(y) - 0.010 * a * a + 0.008 + 0.004 * (1 - a * a)
                    if cap:
                        pitch = np.deg2rad(4.5)
                        dy, dz = y - 1.038, z - 0.125
                        y = 1.038 + dy * np.cos(pitch) - dz * np.sin(pitch)
                        z = 0.125 + dy * np.sin(pitch) + dz * np.cos(pitch)
                    else:
                        z = chin(y) - 0.007 * a * a + 0.0023 * (1 - np.sqrt(a * a + 0.03))
                        z += 0.0018 * np.exp(-(((np.abs(a) - 0.85) / 0.10) ** 2))
                if not cap:
                    pivot = 0.949 + (1.079 - 0.908) * 0.90 if back else 1.079
                    y = pivot + (y - pivot) * 1.16
                return xyz(a * width, y, z)

            anchor = mesh.sheet(
                guard,
                lin(0, 1, 15),
                lin(0, 1, 6),
                [0, 0, -1 if back else 1],
                GUNMETAL,
                rigid(hips),
                0.0021,
            )
            for u, v in (
                [(0.055, 0.34), (0.24, 0.86), (0.5, 0.92), (0.76, 0.86), (0.945, 0.34)]
                if cap
                else [(u, v) for u in [0.085, 0.915] for v in [0.38, 0.58, 0.78, 0.93]]
            ):
                mesh.stud(anchor, u, v, 0.0030 if cap else 0.0026)
            if not cap:
                mesh.stud(anchor, 0.50, 0.966, 0.00255)
        if not back:
            front_guard = range(guard_start, len(mesh.attrs["POSITION"]))
        record("back" if back else "front", start, "Hips")

    for side, prefix in [(-1, "Right"), (1, "Left")]:
        # Heights of the inner lip and outside ridge of each overlapping lame.
        lips = [
            (1.120, 1.164),
            (1.087, 1.119),
            (1.050, 1.088),
            (1.003, 1.047),
            (0.959, 0.986),
            (0.919, 0.888),
        ]

        def boundary(i, u):
            s = np.sin(np.pi * u)
            inside, ridge = lips[i]
            y = inside + (ridge - inside) * s
            f = np.minimum(u, 1 - u)
            if i in (3, 4):
                notch = np.interp(
                    f, [0, 0.040, 0.108, 0.175, 0.30, 0.50], [0.004, -0.004, 0.003, 0.037, 0.012, 0]
                )
                y += notch * (1 if i == 4 else 0.22)
            return y

        for i in range(5):
            start = mesh.triangles
            bone = "Hips" if i < 2 else prefix + "UpLeg"

            def lame(u, v):
                y = (1 - v) * (boundary(i, u) + (0.032 if i == 2 else 0.013 if i else 0)) + v * boundary(
                    i + 1, u
                )
                offset = 0.0011 * (4 - i) + 0.0010 * np.sin(np.pi * v * 0.78)
                angle = np.pi * u
                lip = front_rails[side](np.clip(y, 0.88, 1.125))
                points = shell(side, angle, y, offset, leading=lip)
                if bone in thigh_bind:
                    rotation, pivot = thigh_bind[bone]
                    points = (points - pivot) @ rotation.T + pivot
                return points

            segments = np.unique(np.r_[lin(0, 1, 21), 0.04, 0.108, 0.175, 0.30, 0.70, 0.825, 0.892, 0.96])
            anchor = mesh.sheet(
                lame, segments, lin(0, 1, 4), [side, 0, 0], STEEL, rigid(bones.index(bone)), 0.0019
            )
            for u in [0.028, 0.45, 0.972]:
                mesh.stud(anchor, u, 0.84 if i == 4 else 0.79, 0.00325)
            record(prefix.lower() + "_" + str(i + 1), start, bone)
    fit_waist(mesh, harness_start, bones, thigh_bind, front_guard)
    return retained, panels


def main():
    mask = json.loads((OUT / "body-occlusion.json").read_text())
    base, data = read_glb(ROOT / mask["body"])
    skin = base["skins"][0]
    bind = accessor(base, data, skin["inverseBindMatrices"])
    names = [base["nodes"][j]["name"] for j in skin["joints"]]
    mesh = SurfaceMesh()
    add_leather_underwear(mesh, base, data, mask, names)
    retained, panels = build(mesh, names, thigh_bind_transforms(base, data))
    part = "bottom_female_knight"
    nodes = list(base["nodes"])
    nodes[0] = dict(name=part, mesh=0, skin=0, extras={
        "rig_id": "human_female_01_local_v1", "part_id": part, "fitting_version": 44,
    })
    doc = dict(
        asset=dict(version="2.0", generator="build-female-knight-tassets.py"),
        accessors=[], bufferViews=[], meshes=[dict(name=part)], nodes=nodes,
        scene=base["scene"], scenes=base["scenes"], skins=[dict(skin)],
    )
    data = bytearray()
    attrs = {
        k: add_accessor(
            doc,
            data,
            np.concatenate(v),
            {
                "POSITION": "VEC3",
                "NORMAL": "VEC3",
                "TEXCOORD_0": "VEC2",
                "JOINTS_0": "VEC4",
                "WEIGHTS_0": "VEC4",
            }[k],
            5123 if k == "JOINTS_0" else 5126,
        )
        for k, v in mesh.attrs.items()
    }
    assert mesh.vertices < 65536
    indices = add_accessor(doc, data, np.concatenate(mesh.faces).ravel(), "SCALAR", 5123)
    doc["meshes"][0]["primitives"] = [dict(attributes=attrs, indices=indices, material=0)]
    doc["skins"][0]["inverseBindMatrices"] = add_accessor(doc, data, bind, "MAT4")
    doc["images"], doc["textures"] = [], []
    for name, png in zip(["Tasset finish", "Tasset roughness and metalness"], atlas()):
        data.extend(bytes(-len(data) % 4))
        doc["images"].append(dict(name=name, mimeType="image/png", bufferView=len(doc["bufferViews"])))
        doc["textures"].append(dict(source=len(doc["textures"])))
        doc["bufferViews"].append(dict(buffer=0, byteOffset=len(data), byteLength=len(png)))
        data.extend(png)
    doc["materials"] = [
        dict(
            name="Steel and saddle leather",
            doubleSided=True,
            pbrMetallicRoughness={"baseColorTexture": {"index": 0}, "metallicRoughnessTexture": {"index": 1}},
        )
    ]
    doc["buffers"] = [dict(byteLength=len(data))]
    OUT.mkdir(exist_ok=True, parents=True)
    write_glb(OUT / "bottom_female_knight.glb", doc, data)
    (OUT / "fitting.json").write_text(
        json.dumps(dict(harness_triangles=retained, panels=panels), indent=2) + "\n"
    )
    print(mesh.triangles, mesh.vertices)


if __name__ == "__main__":
    main()
