import copy
import hashlib
import importlib.util
import json
import shutil
from pathlib import Path

import numpy as np

from lib.glb import read_glb, view_bytes, write_glb
from lib.glb_accessors import accessor


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'assets/modular_human_female_01/faces/tripo_custom_v1'
SPEC = importlib.util.spec_from_file_location('female_fitter', ROOT / 'tools/fit-female-custom-face.py')
FIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIT)


def texture_signature(material, doc, raw):
    result = copy.deepcopy(material)
    result.pop('name', None)

    def visit(value):
        for key, child in value.items():
            if key.endswith('Texture') and isinstance(child, dict) and 'index' in child:
                texture = doc['textures'][child['index']]
                image = doc['images'][texture['source']]
                child['index'] = hashlib.sha256(view_bytes(doc, raw, image['bufferView'])).hexdigest()
                child['sampler'] = doc.get('samplers', [])[texture['sampler']] if 'sampler' in texture else None
            elif isinstance(child, dict):
                visit(child)
    visit(result)
    return result


def main():
    assert FIT.digest(FIT.BASE) == '3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5'
    assert FIT.digest(FIT.SOURCE) == '8dbcbcb21d06cd2ed33020a9eef617e1b4396fa4e43292c95748ddbbe3152dce'
    base, base_raw = read_glb(FIT.BASE)
    source, source_raw = read_glb(FIT.SOURCE)
    original_body, body_faces = FIT.geometry(base, base_raw, base['meshes'][0]['primitives'][0])
    source_attrs, source_faces = FIT.geometry(source, source_raw, source['meshes'][0]['primitives'][0])
    source_attrs['source_position'] = source_attrs['POSITION'].astype(np.float64)
    scale = 1.8 / float(source_attrs['POSITION'][:, 1].max())
    offset, height = np.array([-.0008, 0, .014]), 1.552
    source_attrs['POSITION'] = source_attrs['source_position'] * scale + offset
    head, head_faces = FIT.clip(source_attrs, source_faces, height, True)
    head['dense'] = FIT.surface_sample(head['POSITION'], original_body, body_faces, 'dense')
    head_joint = [base['nodes'][i]['name'] for i in base['skins'][0]['joints']].index('Head')
    blend = FIT.smoothstep((head['POSITION'][:, 1] - 1.59) / .02)
    head['dense'] *= 1 - blend[:, None]
    head['dense'][:, head_joint] += blend
    body, kept_faces = FIT.clip(original_body, body_faces, height, False)
    body, kept_faces, head, head_faces, seam_count, displacement = FIT.direct_seam(body, kept_faces, head, head_faces, height)
    reference = head.pop('source_position') * scale + offset
    affine_error = float(np.linalg.norm(head['POSITION'] - reference, axis=1).max())
    assert affine_error < 1e-12
    assert len(head_faces) == 7743 and len(kept_faces) == 15701

    doc, raw = copy.deepcopy(base), bytearray(base_raw)
    body_material = base['meshes'][0]['primitives'][0]['material']
    face_material = FIT.add_material(doc, raw, source, source_raw)
    baseline, baseline_raw = read_glb(OUT / 'reference_v2/before-reference.glb')
    original_material = baseline['materials'][baseline['meshes'][0]['primitives'][0]['material']]
    texture = baseline['textures'][original_material['pbrMetallicRoughness']['baseColorTexture']['index']]
    image = baseline['images'][texture['source']]
    image_data = view_bytes(baseline, baseline_raw, image['bufferView'])
    assert image_data == (OUT / 'body-neck-albedo.png').read_bytes()
    raw.extend(b'\0' * (-len(raw) % 4))
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': len(raw), 'byteLength': len(image_data)})
    raw.extend(image_data)
    doc['images'].append({'bufferView': len(doc['bufferViews']) - 1, 'mimeType': image['mimeType'], 'name': 'Body neck color match'})
    doc['textures'].append({'source': len(doc['images']) - 1})
    doc['materials'][body_material]['pbrMetallicRoughness']['baseColorTexture']['index'] = len(doc['textures']) - 1
    primitives = [FIT.add_geometry(doc, raw, attrs, faces, material) for attrs, faces, material in
                  [(body, kept_faces, body_material), (head, head_faces, face_material)]]
    doc['meshes'] = [{'name': name, 'primitives': [primitive]} for name, primitive in
                     zip(['female_body', 'face_female_custom'], primitives)]
    doc['nodes'][0]['mesh'] = 0
    doc['nodes'].append({'name': 'face_female_custom', 'mesh': 1, 'skin': 0,
                         'extras': {'rig_id': 'human_female_01_local_v1', 'part_id': 'face_female_custom'}})
    doc['scenes'][0]['nodes'].append(len(doc['nodes']) - 1)
    packed = FIT.compact(doc, raw)
    for key, index in doc['meshes'][0]['primitives'][0]['attributes'].items():
        previous = baseline['meshes'][0]['primitives'][0]['attributes'][key]
        np.testing.assert_array_equal(accessor(doc, packed, index), accessor(baseline, baseline_raw, previous))
    np.testing.assert_array_equal(accessor(doc, packed, doc['meshes'][0]['primitives'][0]['indices']),
                                  accessor(baseline, baseline_raw, baseline['meshes'][0]['primitives'][0]['indices']))
    restored_material = doc['materials'][doc['meshes'][1]['primitives'][0]['material']]
    assert texture_signature(restored_material, doc, packed) == texture_signature(source['materials'][0], source, source_raw)
    exported = accessor(doc, packed, doc['meshes'][1]['primitives'][0]['attributes']['POSITION'])
    exported_error = float(np.linalg.norm(exported - reference, axis=1).max())
    assert exported_error < 1.2e-7

    archive = OUT / 'reference_v2/rejected-eye-edits-v2'
    archive.mkdir(exist_ok=True)
    for name in ['base_custom.glb', 'base_custom_preview.glb', 'face_custom.glb', 'custom-face-fitting.blend', 'fitting.json', 'animation-snapshots.json']:
        if not (archive / name).exists():
            shutil.copyfile(OUT / name, archive / name)
    write_glb(OUT / 'base_custom.glb', doc, packed)
    face_doc = copy.deepcopy(doc)
    face_doc.pop('animations', None)
    face_doc['nodes'][0].pop('mesh')
    face_doc['nodes'][0].pop('skin')
    face_doc['meshes'] = face_doc['meshes'][1:]
    for node in face_doc['nodes']:
        if 'mesh' in node:
            node['mesh'] -= 1
    write_glb(OUT / 'face_custom.glb', face_doc, FIT.compact(face_doc, packed))
    report = {
        'date': '2026-11-07', 'neck_fit_only': True,
        'source_path': str(FIT.SOURCE.relative_to(ROOT)), 'source_sha256': FIT.digest(FIT.SOURCE),
        'base_sha256': FIT.digest(FIT.BASE), 'scale': scale, 'offset_m': offset.tolist(),
        'neck_cut_y_m': height, 'neck_seams': 1, 'neck_bridge_triangles': 0,
        'rig_id': 'human_female_01_local_v1', 'new_head_scale_and_translation_only': True,
        'head_affine_scope': 'Entire source head, including eyes and eyelashes, scalp and neck',
        'maximum_head_affine_error_m': affine_error, 'maximum_exported_head_affine_error_m': exported_error,
        'source_triangles': len(source_faces), 'head_triangles': len(head_faces),
        'retained_body_triangles': len(kept_faces), 'assembled_triangles': len(kept_faces) + len(head_faces),
        'neck_boundary_vertices': seam_count, 'body_neck_fit_maximum_mm': displacement,
        'body_geometry_and_neck_color_match_initial_fit': True,
        'source_texture_bytes_preserved': True, 'source_head_material_preserved': True,
        'eye_and_eyelash_edits_applied': False,
        'canonical_bone_nodes_bind_matrices_and_animation_tracks_preserved': True,
        'meshes': 2, 'materials': 2, 'preview_texture_limit': 2048,
        'files': {name: FIT.digest(OUT / name) for name in ['base_custom.glb', 'face_custom.glb', 'body-neck-albedo.png']},
        'status': 'Restored original head with single neck seam; eye and eyelash experiments are unused',
    }
    for path in [OUT / 'fitting.json', ROOT / 'doc/assets/modular-female-custom-face-fitting-v1.json']:
        path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
