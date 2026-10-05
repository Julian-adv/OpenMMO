"""Add local waist clearance to the preserved ranger top v2."""
import importlib.util
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('ranger_fit', ROOT / 'tools/fit-tripo-ranger-top.py')
ranger = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ranger)
fit, io = ranger.fit, ranger.io


def main(source, output, report_path):
    assert fit.digest(source) == 'c5ac6263f934708779693efe9e587d3b381d04c7e25f617e519c1c60eed7a5a7'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = io.accessor(doc, raw, attrs['POSITION']).astype(float)
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    unchanged = {name: io.accessor(doc, raw, index).copy() for name, index in attrs.items()
                 if name not in ['POSITION', 'NORMAL', 'JOINTS_0', 'WEIGHTS_0']}
    images = [ranger.view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    positions = original.copy()
    trunk = 1 - io.smoothstep((abs(positions[:, 0]) - .20) / .03)
    amount = (1 - io.smoothstep((positions[:, 1] - 1.15) / .09)) * trunk
    radial = positions[:, [0, 2]] - [0, -.005]
    length = np.linalg.norm(radial, axis=1)
    shift = radial / np.maximum(length[:, None], 1e-6) * (.009 * amount[:, None])
    positions[:, [0, 2]] += shift
    binary = bytearray(raw)
    attrs['POSITION'] = io.add_accessor(doc, binary, positions, 'VEC3')
    attrs['NORMAL'] = io.add_accessor(doc, binary, io.smooth_normals(positions, faces), 'VEC3')
    joints = io.accessor(doc, raw, attrs['JOINTS_0']).copy()
    weights = io.accessor(doc, raw, attrs['WEIGHTS_0']).copy()
    dense = np.zeros((len(positions), len(fit.NAMES)))
    np.add.at(dense, (np.arange(len(positions))[:, None], joints), weights)
    hip_amount = (1 - io.smoothstep((original[:, 1] - 1.16) / .10)) * trunk
    dense *= 1 - hip_amount[:, None]
    dense[:, fit.NAMES.index('Hips')] += hip_amount
    changed_joints, changed_weights = ranger.compact_weights(dense)
    changed = hip_amount > 0
    joints[changed], weights[changed] = changed_joints[changed], changed_weights[changed]
    attrs['JOINTS_0'] = io.add_accessor(doc, binary, joints, 'VEC4', 5123)
    attrs['WEIGHTS_0'] = io.add_accessor(doc, binary, weights, 'VEC4')
    assert np.allclose(weights.sum(1), 1, atol=1e-6)
    for node in doc['nodes']:
        if node.get('extras', {}).get('part_id') == 'top_ranger':
            node['extras']['fitting_status'] = 'candidate_tripo_v3'
    binary = io.compact(doc, binary)
    attrs = doc['meshes'][0]['primitives'][0]['attributes']
    for name, values in unchanged.items():
        assert np.array_equal(values, io.accessor(doc, binary, attrs[name]))
    assert images == [ranger.view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    output.parent.mkdir(parents=True, exist_ok=True)
    fit.write_glb(output, doc, binary)
    displacement = np.linalg.norm(positions - original, axis=1)
    report = dict(date='2026-10-05', revision=3,
                  method='Outward waist clearance, smoothly tapered into the existing torso fit',
                  source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
                  waist_radial_expansion_m=.009, full_expansion_below_y_m=1.15,
                  zero_expansion_above_y_m=1.24, radial_center_xz_m=[0, -.005],
                  maximum_vertex_displacement_m=float(displacement.max()),
                  changed_vertices=int((displacement > 1e-7).sum()),
                  waist_skinning=dict(bone='Hips', full_below_y_m=1.16, unchanged_above_y_m=1.26,
                                     changed_vertices=int(changed.sum()), reason='Match cloth waist motion without arm influence at the vest hem'),
                  preserved=['UV', 'indices', 'upper torso and sleeve weights', 'embedded textures', 'skeleton', 'bind matrices'],
                  validation=fit.validate(output))
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return report


if __name__ == '__main__':
    raise SystemExit('Use tools/build-ranger-top.py to build the final asset.')
