"""Remove concealed shirt lining from the preserved ranger top v3."""
import importlib.util
import io as image_io
import json
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('ranger_fit', ROOT / 'tools/fit-tripo-ranger-top.py')
ranger = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ranger)
fit, io = ranger.fit, ranger.io


def leather_coverage(points, triangles):
    edge1, edge2 = triangles[:, 1] - triangles[:, 0], triangles[:, 2] - triangles[:, 0]
    covered = []
    for point in points:
        direction = np.array([point[0], 0, point[2] + .005])
        direction /= np.linalg.norm(direction)
        h = np.cross(direction, edge2)
        denominator = np.sum(edge1 * h, axis=1)
        safe = np.where(abs(denominator) > 1e-8, denominator, np.nan)
        offset = point - triangles[:, 0]
        u = np.sum(offset * h, axis=1) / safe
        q = np.cross(offset, edge1)
        v = np.sum(q * direction, axis=1) / safe
        distance = np.sum(edge2 * q, axis=1) / safe
        valid = (u >= -1e-5) & (v >= -1e-5) & (u + v <= 1 + 1e-5) & (abs(distance) < .035)
        covered.append(bool(valid.any()))
    return np.asarray(covered)


def main(source, output, report_path):
    assert fit.digest(source) == '2450d323ec749f95506900c7e25d289ab7b76495c111c8c72fa7098c356b83be'
    doc, raw = fit.read_glb(source)
    primitive = doc['meshes'][0]['primitives'][0]
    attrs = primitive['attributes']
    original = {name: io.accessor(doc, raw, index).copy() for name, index in attrs.items()}
    formats = {name: (doc['accessors'][index]['type'], doc['accessors'][index]['componentType'])
               for name, index in attrs.items()}
    faces = io.accessor(doc, raw, primitive['indices']).reshape(-1, 3)
    vertices, uv = original['POSITION'], original['TEXCOORD_0']
    images = [ranger.view_bytes(doc, raw, image['bufferView']) for image in doc['images']]
    image = np.asarray(Image.open(image_io.BytesIO(images[0])).convert('RGB'), dtype=float) / 255
    barycentric = np.array([[1/3, 1/3, 1/3], [.8, .1, .1], [.1, .8, .1], [.1, .1, .8],
                            [.5, .4, .1], [.1, .5, .4], [.4, .1, .5]])
    samples = np.einsum('kj,fji->fki', barycentric, uv[faces])
    x = np.clip((samples[:, :, 0] * image.shape[1]).astype(int), 0, image.shape[1] - 1)
    y = np.clip((samples[:, :, 1] * image.shape[0]).astype(int), 0, image.shape[0] - 1)
    rgb = image[y, x]
    green = ((rgb[:, :, 0] / np.maximum(rgb[:, :, 1], .001) < 1.19) &
             (rgb[:, :, 1] / np.maximum(rgb[:, :, 2], .001) > 1.05)).mean(1)
    cloth, leather = green > .7, green < .15
    triangles = vertices[faces]
    low_lining = (triangles[:, :, 1].max(1) < 1.265) & (abs(triangles[:, :, 0]).max(1) < .215)
    torso = (triangles[:, :, 1].max(1) < 1.46) & (abs(triangles[:, :, 0]).max(1) < .205)
    remove = cloth & low_lining
    for index in np.where(cloth & torso & ~remove)[0]:
        probes = barycentric[:4] @ triangles[index]
        remove[index] = leather_coverage(probes, triangles[leather]).all()
    mixed_lining = (green >= .2) & ~cloth & low_lining
    for index in np.where(mixed_lining)[0]:
        probes = barycentric[:4] @ triangles[index]
        remove[index] = leather_coverage(probes, triangles[leather]).all()
    assert remove.any() and not (remove & leather).any()
    used, inverse = np.unique(faces[~remove], return_inverse=True)
    binary = bytearray(raw)
    primitive['attributes'] = {name: io.add_accessor(doc, binary, values[used], *formats[name])
                               for name, values in original.items()}
    primitive['indices'] = io.add_accessor(doc, binary, inverse.reshape(-1, 1), 'SCALAR', 5125)
    for node in doc['nodes']:
        if node.get('extras', {}).get('part_id') == 'top_ranger':
            node['extras']['fitting_status'] = 'candidate_tripo_v4'
    binary = io.compact(doc, binary)
    attrs = doc['meshes'][0]['primitives'][0]['attributes']
    for name, values in original.items():
        assert np.array_equal(values[used], io.accessor(doc, binary, attrs[name]))
    assert images == [ranger.view_bytes(doc, binary, image['bufferView']) for image in doc['images']]
    output.parent.mkdir(parents=True, exist_ok=True)
    fit.write_glb(output, doc, binary)
    report = dict(date='2026-10-05', revision=4, method='Delete concealed olive cloth faces; retain leather and exposed shirt',
                  source=dict(path=str(source.relative_to(ROOT)), sha256=fit.digest(source)),
                  removed_triangles=int(remove.sum()), remaining_triangles=int((~remove).sum()),
                  removed_face_indices=np.where(remove)[0].tolist(),
                  classification=dict(uv_samples_per_face=7, cloth_red_green_ratio_max=1.19,
                                      cloth_green_blue_ratio_min=1.05, cloth_required_sample_fraction=.7,
                                      low_lining_maximum_y_m=1.265, low_lining_maximum_abs_x_m=.215,
                                      mixed_low_lining_sample_fraction_min=.2,
                                      upper_lining_maximum_y_m=1.46, upper_lining_maximum_abs_x_m=.205,
                                      upper_lining_required_leather_probes=4, leather_probe_distance_m=.035),
                  preserved=['Leather-classified exterior faces', 'Exposed sleeves and collar', 'Surviving positions, normals, UV and skin attributes',
                             'Embedded textures', 'Skeleton and bind matrices'],
                  validation=fit.validate(output))
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: report[key] for key in ['removed_triangles', 'remaining_triangles', 'validation']}, indent=2))
    return report


if __name__ == '__main__':
    raise SystemExit('Use tools/build-ranger-top.py to build the final asset.')
