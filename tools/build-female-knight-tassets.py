"""Model the knight's lower harness from the supplied front three-quarter artwork."""

import json
from pathlib import Path

import numpy as np
from scipy.interpolate import PchipInterpolator
from scipy.spatial.transform import Rotation, Slerp

from lib.glb import read_glb, write_glb
from lib.glb_accessors import accessor, add_accessor
from outfits.knight_plate_surfaces import SurfaceMesh, atlas, STEEL, GUNMETAL, BRASS, LEATHER, LINING

ROOT = Path(__file__).resolve().parents[1]
ITEM = ROOT / "assets/items/female_knight_bottom"
OUT = ITEM / "reference_v1/fitted_v1"
OLD = ITEM / "tripo_v1/fitted_v42"
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


def thigh_bind_transforms():
    # Return each rigid thigh plate from the fitting stance to its bind pose.
    base, data = read_glb(ROOT / "assets/modular_human_female_01/rig_v1/female-base.glb")
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


def build(mesh, bones):
    hips = bones.index("Hips")
    thigh_bind = thigh_bind_transforms()
    front_rails = {}

    def weighted(side, y):
        leg = bones.index(("Left" if side > 0 else "Right") + "UpLeg")
        t = np.clip((1.098 - y) / 0.172, 0, 1).ravel()
        t = t * t * (3 - 2 * t)
        return [hips, leg, 0, 0], np.stack([1 - t, t, 0 * t, 0 * t], axis=-1)

    def suspension(side, back):
        top, end = (1.146, 0.748) if back else (1.128, 0.843)

        def strap(u, v):
            y = top + (end - top) * v + 0.007 * (2 * u - 1) ** 2 * (v**12 - 0.6 * (1 - v) ** 14)
            x = (0.068 + 0.085 * v if back else 0.064 + 0.038 * v) + (u - 0.5) * (0.020 - 0.003 * v)
            if back:
                z = rear(y) - 0.019
                z = np.where(y < 0.946, np.interp(y, [0.74, 0.87, 0.946], [-0.112, -0.137, -0.182]), z)
            else:
                z = np.interp(
                    y, [0.84, 0.895, 0.96, 1.02, 1.080, 1.14], [0.104, 0.109, 0.115, 0.122, 0.140, 0.123]
                )
            z += (-1 if back else 1) * 0.0009 * (1 - (2 * u - 1) ** 2)
            return xyz(side * x, y, z)

        skin = lambda u, v: weighted(side, top + (end - top) * v)
        stations = np.array([0, 0.04, 0.13, 0.24, 0.90, 0.955, 0.98, 1])
        if not back:
            # The front lip follows the outside edge of the suspender in the fitting stance.
            rail = strap(0.78, stations)
            rotation, pivot = thigh_bind[("Left" if side > 0 else "Right") + "UpLeg"]
            weight = skin(0.78, stations)[1][:, 1, None]
            posed = rail + weight * ((rail - pivot) @ rotation + pivot - rail)
            front_rails[side] = PchipInterpolator(posed[::-1, 1], side * posed[::-1, 0])

        anchor = mesh.sheet(
            strap,
            lin(0, 1, 5),
            stations,
            [0, 0, -1 if back else 1],
            LEATHER,
            skin,
            0.0018,
        )
        for v in [0.048, 0.13, 0.215, 0.93]:
            mesh.stud(anchor, 0.5, v, 0.0018)

        # The brass chape follows the rounded end of the leather at the boot.
        def chape(u, v):
            theta = -0.12 + (np.pi + 0.24) * u
            x = 0.5 + (0.51 + 0.065 * v) * np.cos(theta)
            y = 0.945 + (0.048 + 0.006 * v) * np.sin(theta)
            p = strap(x, y)
            p[..., 2] += (-1 if back else 1) * 0.0018
            return p

        mesh.sheet(
            chape,
            lin(0, 1, 14),
            [0, 1],
            [0, 0, -1 if back else 1],
            BRASS,
            rigid(bones.index(("Left" if side > 0 else "Right") + "UpLeg")),
            0.001,
        )
        # Thin brass keeper folded across the long strap.
        for v0 in [0.24, 0.91]:

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
                lambda u, v: skin(u, v0 + 0.015 * v),
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

    for side in [-1, 1]:

        def cross_belt(u, v):
            t = -0.432 + (np.pi / 2 + 0.432) * u
            x = np.sin(t)
            y = 1.095 + 0.061 * x + 0.021 * (v - 0.5)
            radius = outer(y) + 0.0025
            z = 0.014 + (0.119 + 0.003 * side) * np.cos(t) + 0.017 * np.sin(t) ** 2 * np.cos(t)
            angle = (t - 0.48) * (np.pi / 2) / (np.pi / 2 - 0.48)
            return blend_hip(xyz(side * x * radius, y, z), waist_surface(side, angle, v), (t - 0.30) / 0.43)

        band = mesh.sheet(
            cross_belt, lin(0, 1, 25), lin(0, 1, 4), [side * 0.4, 0, 1], LEATHER, rigid(hips), 0.002
        )
        for u in [0.055, 0.215, 0.51, 0.79]:
            mesh.stud(band, u, 0.5, 0.0021)
        for u in [0.10, 0.145, 0.61, 0.66]:
            mesh.stud(band, u, 0.48, 0.00135, LINING)
        suspension(side, False)
        suspension(side, True)

    def back_belt(u, v):
        theta = np.pi * u
        y = 1.156 - 0.017 * np.sin(theta) + 0.022 * (v - 0.5)
        center = xyz((outer(y) + 0.0025) * np.cos(theta), y, 0.014 - 0.105 * np.sin(theta))
        arc = np.minimum(theta, np.pi - theta)
        hip = waist_surface(np.where(theta < np.pi / 2, 1, -1), np.pi / 2 + arc * 1.4, v)
        return blend_hip(center, hip, (1.25 - arc) / 0.45)

    mesh.sheet(back_belt, lin(0, 1, 27), lin(0, 1, 4), [0, 0, -1], LEATHER, rigid(hips), 0.002)

    def belt_boss(u, v):
        return np.array([0, 1.095, 0.137]), np.array([0, 0, 1]), *rigid(hips)(u, v)

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
                    if not cap:
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
    return retained, panels


def main():
    doc, binary = read_glb(OLD / "bottom_female_knight.glb")
    names = [doc["nodes"][j]["name"] for j in doc["skins"][0]["joints"]]
    mesh = SurfaceMesh()
    retained, panels = build(mesh, names)
    bind = accessor(doc, binary, doc["skins"][0]["inverseBindMatrices"])
    data = bytearray()
    doc.update(bufferViews=[], accessors=[])
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
    doc["nodes"][0]["extras"]["fitting_version"] = 44
    OUT.mkdir(exist_ok=True, parents=True)
    write_glb(OUT / "bottom_female_knight.glb", doc, data)
    (OUT / "body-occlusion.json").write_bytes((OLD / "body-occlusion.json").read_bytes())
    (OUT / "fitting.json").write_text(
        json.dumps(dict(harness_triangles=retained, panels=panels), indent=2) + "\n"
    )
    print(mesh.triangles, mesh.vertices)


if __name__ == "__main__":
    main()
