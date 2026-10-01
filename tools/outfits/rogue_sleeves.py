"""Shirt sleeves with folded cuffs below the shared long-glove sections."""
import json

import numpy as np

from outfits.rogue_layers import atlas_uv, grid_faces


def add_sleeves(fit, doc, binary, material_index):
    io = fit.io
    reference = json.loads((fit.ROOT / fit.SELECTION['reference']['interfaces']).read_text())
    records = []
    for side, sign in [('Left', 1), ('Right', -1)]:
        ref = next(item for item in reference['interfaces'] if item['name'] == 'glove_long_' + side)
        center, axis = np.array(ref['center']), io.unit(np.array(ref['normal']))
        across = io.unit(np.cross(axis, [0, 0, 1]))
        forward = np.cross(axis, across)
        basis = np.array([across, forward])
        shoulder = fit.BONES[side + 'Arm']
        length = (shoulder - center) @ axis
        surface = fit.body_surface(('upper_arms', 'forearms'), sign)
        triangles = surface[0][surface[1]]
        heights = np.array([length - .015, length - .060, .23, .19, .15, .11, .075, .04, .008, 0, -.012, -.0175])
        assert np.all(np.diff(heights) < 0)
        count = 24
        theta = np.linspace(0, 2 * np.pi, count + 1)
        radial = np.column_stack([np.cos(theta), np.sin(theta)])
        points, uv = [], []
        for row, height in enumerate(heights):
            origin = center + height * axis
            low, high = io.section_bounds(triangles, origin, axis, basis)
            mid, radius = (low + high) / 2, (high - low) / 2
            clearance = np.interp(height, [-.0175, .008, .075, .19, length], [.006, .008, .014, .017, .009])
            folds = .002 * np.sin(theta * 6 + row * .8) * np.sin(np.pi * row / (len(heights) - 1))
            ring = origin + (mid + radial * (radius + clearance + folds[:, None])) @ basis
            inward = np.maximum(0, -(radial @ basis)[:, 0] * sign)
            ring[:, 0] -= sign * .010 * inward * io.smoothstep((height - .13) / .10)
            points.extend(ring)
            uv.extend(np.column_stack([theta / (2 * np.pi), np.full(count + 1, row / (len(heights) - 1))]))
        outer_rings = len(heights)
        last = np.array(points[-count - 1:])
        folds = [(0, .003), (.008, .008), (.027, .010), (.031, .006), (.027, .003)]
        for index, (height, thickness) in enumerate(folds):
            points.extend(last + axis * height + thickness * (radial @ basis))
            uv.extend(np.column_stack([theta / (2 * np.pi), np.full(count + 1, .90 - index * .04)]))
        total_rings = outer_rings + len(folds)
        faces = grid_faces(total_rings, count + 1)[:, [0, 2, 1]].tolist()
        cap = len(points)
        points.append(np.mean(points[:count], axis=0))
        uv.append([.5, .05])
        faces.extend([[cap, col, col + 1] for col in range(count)])
        points, uv, faces = np.array(points), np.array(uv), np.array(faces)
        _, welded = np.unique(np.round(points, 6), axis=0, return_inverse=True)
        edges = np.sort(np.concatenate([welded[faces[:, [0, 1]]], welded[faces[:, [1, 2]]], welded[faces[:, [2, 0]]]]), axis=1)
        _, incidence = np.unique(edges, axis=0, return_counts=True)
        assert np.count_nonzero(incidence == 1) == count
        assert incidence.max() == 2
        cuff = points[(outer_rings - 1) * (count + 1):outer_rings * (count + 1)]
        plane_error = float(np.max(abs((cuff - center) @ axis + .0175)))
        assert plane_error < 1e-7
        normals = io.smooth_normals(points, faces)
        for row in range(total_rings):
            a, b = row * (count + 1), (row + 1) * (count + 1) - 1
            normals[a] = normals[b] = io.unit(normals[a] + normals[b])
        weight_heights = np.concatenate([np.repeat(heights, count + 1), np.full(len(folds) * (count + 1), heights[-1]), [heights[0]]])
        elbow_height = (fit.BONES[side + 'ForeArm'] - center) @ axis
        forearm = 1 - io.smoothstep((weight_heights - elbow_height + .065) / .13)
        inward = np.tile(np.maximum(0, -(radial @ basis)[:, 0] * sign), total_rings)
        inward = np.append(inward, 1.)
        shoulder_weight = np.maximum(.65 * io.smoothstep((weight_heights - length + .07) / .10),
                                     .65 * inward * io.smoothstep((weight_heights - .13) / .10))
        joints = np.tile([fit.NAMES.index(side + bone) for bone in ['Shoulder', 'Arm', 'ForeArm']] + [0], (len(points), 1))
        weights = np.column_stack([shoulder_weight, 1 - shoulder_weight - forearm, forearm, np.zeros(len(points))])
        attrs = io.add_skin_attributes(doc, binary, points, normals, joints, weights)
        attrs['TEXCOORD_0'] = io.add_accessor(doc, binary, atlas_uv(uv, 'linen'), 'VEC2')
        attrs['TANGENT'] = io.add_accessor(doc, binary, io.tangents(points, normals, uv, faces), 'VEC4')
        doc['meshes'].append(dict(name='shirt_sleeve_' + side.lower(), primitives=[dict(
            attributes=attrs, material=material_index,
            indices=io.add_accessor(doc, binary, faces.reshape(-1, 1), 'SCALAR', 5125))]))
        records.append(dict(side=side, interface=ref['name'], center=center.tolist(), axis=axis.tolist(),
                            cuff_plane_offset_m=-.0175, folded_cuff_width_m=.031, folded_cuff_projection_m=.010, triangles=len(faces),
                            cuff_plane_maximum_error_m=plane_error,
                            boundary_edges=count, nonmanifold_edges=0,
                            boundary_location='fold upper lip tucked against sleeve; shoulder capped, sleeve wall continuous',
                            skinning='smooth axial Shoulder/Arm/ForeArm blend; cuff lining shares outer weights',
                            circumferential_segments=count, outer_rings=outer_rings))
    return records
