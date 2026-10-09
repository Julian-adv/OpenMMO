"""Fit the ranger bracer, conform the hand and mirror the gauntlet."""
from io import BytesIO
import importlib.util
import json
from pathlib import Path

import numpy as np
from PIL import Image

from lib.glb import view_bytes
from outfits.rogue_layers import clip_garment, wrist_section

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'assets/modular_human_male_01/ranger/tripo_gloves_v1'
spec = importlib.util.spec_from_file_location('glove', ROOT / 'tools/fit-tripo-glove.py')
glove = importlib.util.module_from_spec(spec)
spec.loader.exec_module(glove)
fit, io = glove.fit, glove.io
FINGERS = ['Pinky', 'Ring', 'Middle', 'Index', 'Thumb']
ROOTS = np.array([[-.181, .155], [-.105, .151], [-.023, .151], [.066, .164], [.109, .263]])
TIPS = np.array([[-.181, .075], [-.105, .032], [-.022, .006], [.066, .037], [.211, .191]])


def frame():
    wrist, elbow = [fit.BONES[name] for name in ['RightHand', 'RightForeArm']]
    axis = io.unit(wrist - elbow)
    width = fit.BONES['RightHandIndex1'] - fit.BONES['RightHandPinky1']
    width = io.unit(width - axis * (width @ axis))
    return wrist, axis, np.array([width, io.unit(np.cross(axis, width))])


def fit_bracer(points, source_triangles):
    wrist, axis, basis = frame()
    theta = np.arange(96) * 2 * np.pi / 96
    radial = np.column_stack([np.cos(theta), np.sin(theta)])
    source_basis = np.array([[1, 0, 0], [0, 0, 1]])
    surface = fit.body_surface(('hands', 'forearms'), -1)
    heights = np.linspace(.345, .94, 48)
    source_centers, source_radii, centers, radii = [], [], [], []
    for height in heights:
        center, radius = wrist_section(source_triangles, np.array([0, height, 0]), np.array([0, 1, 0]),
                                      source_basis, radial)
        destination = wrist + axis * ((.40 - height) * .375)
        body_center, body_radius = wrist_section(surface[0][surface[1]], destination, axis, basis, radial)
        source_centers.append(center)
        source_radii.append(radius)
        centers.append(body_center)
        radii.append(body_radius)
    source_centers, source_radii, centers, radii = map(np.asarray, [source_centers, source_radii, centers, radii])
    height = points[:, 1]
    source_center = np.column_stack([np.interp(height, heights, source_centers[:, i]) for i in range(2)])
    cross = points @ source_basis.T - source_center
    angle = np.arctan2(cross[:, 1], cross[:, 0])
    length = np.linalg.norm(cross, axis=1)

    def sample(table):
        values = np.array([np.interp(angle, theta, row, period=2 * np.pi) for row in table])
        return np.array([np.interp(h, heights, values[:, i]) for i, h in enumerate(height)])

    center = np.column_stack([np.interp(height, heights, centers[:, i]) for i in range(2)])
    relative = length / sample(source_radii)
    radius = sample(radii) + .007 + (relative - 1) * .025
    result = wrist + ((.40 - height) * .375)[:, None] * axis
    result += (center + np.column_stack([np.cos(angle), np.sin(angle)]) * radius[:, None]) @ basis
    joints, weights = fit.transfer(result, surface, {'RightForeArm', 'RightHand'})
    rigid = height > .46
    joints[rigid] = [fit.NAMES.index('RightForeArm'), 0, 0, 0]
    weights[rigid] = [1, 0, 0, 0]
    return result, joints, weights, relative


def texture_alignment():
    wrist, axis, basis = frame()
    source, target = [], []
    for height, along, center, span, width in [(.40, 0, -.035, .12, .032), (.35, .035, -.026, .138, .038),
                                             (.27, .075, -.014, .156, .043)]:
        for sign in [-1, 0, 1]:
            source.append([center + span * sign, -height])
            target.append(wrist + axis * along + basis[0] * width * sign)
    fingers = {}
    for i, finger in enumerate(FINGERS):
        root, middle, end = [fit.BONES[f'RightHand{finger}{j}'] for j in [1, 2, 3]]
        opening = middle * .85 + end * .15
        src_root, src_tip = ROOTS[i] * [1, -1], TIPS[i] * [1, -1]
        source.extend([src_root, src_tip])
        target.extend([root, opening])
        fingers[finger] = dict(source_root=src_root.tolist(), source_opening=src_tip.tolist(),
                              target_root=root.tolist(), target_opening=opening.tolist())
    return dict(source_landmarks=np.asarray(source).tolist(), target_landmarks=np.asarray(target).tolist(),
                body_depth_axis=basis[1].tolist(), fingers=fingers, source_finger_half_width=.028)


def main():
    sources = json.loads((ROOT / 'doc/assets/modular-ranger-tripo-gloves-sources.json').read_text())
    for entry in [sources['source'], sources['base'], sources['interfaces']]:
        assert fit.digest(ROOT / entry['path']) == entry['sha256']
    doc, raw = fit.read_glb(OUTPUT / 'source.glb')
    primitive = doc['meshes'][0]['primitives'][0]
    points = io.accessor(doc, raw, primitive['attributes']['POSITION']).astype(float)
    uv = io.accessor(doc, raw, primitive['attributes']['TEXCOORD_0'])
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    images = [view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    glove.ALONG, glove.DEPTH = np.array([0, -1, 0]), np.array([0, 0, 1])
    shell, shell_faces, shell_joints, shell_weights, regions = glove.hand_shell()
    alignment = texture_alignment()
    shell_uv, baked = glove.bake_texture(shell, shell_faces, points, faces, uv, images[0], alignment)
    charts = Image.open(BytesIO(baked))
    atlas = Image.new('RGB', (2048, 2048))
    atlas.paste(Image.open(BytesIO(images[0])).resize((1024, 1024), Image.Resampling.LANCZOS), (0, 0))
    atlas.paste(charts.crop((2048, 0, 3072, 2048)), (1024, 0))
    shell_uv[:, 0] = shell_uv[:, 0] * 2 - .5
    left_shell, left_faces, left_joints, left_weights, left_regions = glove.hand_shell('Left')
    reflected_shell = left_shell * [-1, 1, 1]
    wrist, axis, basis = frame()
    projection = np.array([basis[0], axis])
    projected = (shell - wrist) @ projection.T
    low, high = projected.min(0) - .002, projected.max(0) + .002
    left_uv = ((reflected_shell - wrist) @ projection.T - low) / (high - low)
    reflected_faces = left_faces[:, ::-1]
    normal = np.cross(reflected_shell[reflected_faces[:, 1]] - reflected_shell[reflected_faces[:, 0]],
                      reflected_shell[reflected_faces[:, 2]] - reflected_shell[reflected_faces[:, 0]])
    chart = (normal @ basis[1] < 0).astype(int)
    left_uv[:, 0] = .5 + left_uv[:, 0] * .5
    left_uv[:, 1] = left_uv[:, 1] * .5 + np.repeat(chart, 3) * .5
    encoded = BytesIO()
    atlas.save(encoded, format='JPEG', quality=92)
    cuff, cuff_faces, cuff_uv = clip_garment(points, faces, uv, .345 - points[:, 1])
    unique, inverse = np.unique(cuff, axis=0, return_inverse=True)
    cuff_positions, cuff_joints, cuff_weights, relative = fit_bracer(unique, points[faces])
    filled_lining = np.all(relative[inverse[cuff_faces]] < .85, axis=1)
    removed_lining_faces = int(filled_lining.sum())
    cuff_faces = cuff_faces[~filled_lining]
    positions = np.vstack([shell, cuff_positions[inverse]])
    triangles = np.vstack([shell_faces, cuff_faces + len(shell)])
    texcoords = np.vstack([shell_uv, cuff_uv * .5])
    joints = np.vstack([shell_joints, cuff_joints[inverse]])
    weights = np.vstack([shell_weights, cuff_weights[inverse]])
    binary = bytearray(raw)
    binary.extend(b'\0' * (-len(binary) % 4))
    doc['bufferViews'].append(dict(buffer=0, byteOffset=len(binary), byteLength=len(encoded.getvalue())))
    binary.extend(encoded.getvalue())
    doc['images'].append(dict(bufferView=len(doc['bufferViews']) - 1, mimeType='image/jpeg'))
    doc['textures'].append(dict(source=len(doc['images']) - 1))
    doc['materials'][primitive['material']]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    doc['meshes'] = []
    for side in ['Right', 'Left']:
        p, f, j = positions.copy(), triangles.copy(), joints.copy()
        if side == 'Left':
            mapping = np.array([fit.NAMES.index(name.replace('Right', 'Left', 1)) if name.startswith('Right') else i
                                for i, name in enumerate(fit.NAMES)])
            p = np.vstack([left_shell, cuff_positions[inverse] * [-1, 1, 1]])
            f = np.vstack([left_faces, cuff_faces[:, ::-1] + len(left_shell)])
            j = np.vstack([left_joints, mapping[cuff_joints[inverse]]])
            skin = np.vstack([left_weights, cuff_weights[inverse]])
            coords = np.vstack([left_uv, cuff_uv * .5])
        else:
            skin, coords = weights, texcoords
        attributes = io.add_skin_attributes(doc, binary, p, io.smooth_normals(p, f), j, skin)
        attributes['TEXCOORD_0'] = io.add_accessor(doc, binary, coords, 'VEC2')
        doc['meshes'].append(dict(name='ranger_glove_' + side.lower(), primitives=[dict(attributes=attributes,
            material=primitive['material'], indices=io.add_accessor(doc, binary, f.reshape(-1, 1), 'SCALAR', 5125))]))
    fit.with_rig(doc, binary, 'gloves_ranger', 'tripo_ranger_gloves_v1')
    binary = io.compact(doc, binary)
    target = OUTPUT / 'gloves_ranger.glb'
    fit.write_glb(target, doc, binary)
    assert images == [view_bytes(doc, binary, image['bufferView']) for image in doc['images'][:len(images)]]
    report = dict(date='2026-10-07', source=sources['source'], base=sources['base'], interfaces=sources['interfaces'],
                  source_triangles=len(faces), triangles_per_glove={'Right': len(triangles), 'Left': len(left_faces) + len(cuff_faces)},
                  mirrored_triangles=len(triangles) + len(left_faces) + len(cuff_faces),
                  source_bracer_triangles=len(cuff_faces), canonical_hand_shell_triangles=len(shell_faces),
                  removed_filled_inner_lining_triangles=removed_lining_faces,
                  method='Source outer bracer sections map to actual skin radius plus 7mm, with source ridge/wall differences scaled by 25mm; canonical hand shells offset 3mm with five finger openings; original leather texture projected to dorsal/palmar charts',
                  original_embedded_texture_retained=True, baked_atlas_dimensions=[2048, 2048],
                  hand_chart_dimensions=[1024, 1024], alignment=alignment, finger_regions=regions,
                  finger_regions_by_side={'Right': regions, 'Left': left_regions},
                  left_hand_correction='Mirrored design refitted to actual left hand surface and original left finger weights; canonical left/right finger rest poses differ',
                  corrections=[dict(region='Left index/thumb web',
                      problem='Directly mirrored right-hand shell left 258 uncovered points in the 175-pose left-hand web check',
                      fix='Refit the mirrored hand design to the actual left skin and its original weights; retain mirrored source bracer and leather charts',
                      before_glb_sha256='647f33da2b2dc6b37a1fd22737b33aa1185587494513bb1b6853d34e8e9c1731')],
                  bracer_source_cut_y=.345, cuff_height_from_wrist_m=.225,
                  mirroring='X reflection, reversed triangle winding, and corresponding Left hand/finger/forearm bones',
                  rejected_methods=[dict(method='Whole source hand warped with 3D section landmarks',
                      reason='Finger lining folded and hand/palm skin protruded; replace hand topology while retaining source bracer and baking original texture')],
                  validation=fit.validate(target))
    (ROOT / 'doc/assets/modular-ranger-tripo-gloves-fitting-v1.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['validation']))


if __name__ == '__main__':
    main()
