"""Level the relaxed idle torso and average mirrored arm rotations."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import numpy as np
from scipy.spatial.transform import Rotation, Slerp

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
DEFAULT = ROOT / 'assets/modular_human_male_01/animations/animations.glb'
REVISION = '2026-10-09-v1'
MIRROR = np.array([1, -1, -1, 1])


def values(doc, binary, index):
    accessor = doc['accessors'][index]
    assert accessor['componentType'] == 5126 and 'sparse' not in accessor
    width = {'SCALAR': 1, 'VEC4': 4}[accessor['type']]
    view = doc['bufferViews'][accessor['bufferView']]
    assert view.get('buffer', 0) == 0
    offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
    return np.ndarray((accessor['count'], width), dtype='<f4', buffer=binary,
                      offset=offset, strides=(view.get('byteStride', width * 4), 4))


def continuous(quaternions):
    result = quaternions / np.linalg.norm(quaternions, axis=1, keepdims=True)
    for i in range(1, len(result)):
        if result[i] @ result[i - 1] < 0:
            result[i] *= -1
    return result


def symmetrize(source, output):
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    doc, original = read_glb(source)
    extras = doc['scenes'][doc.get('scene', 0)].setdefault('extras', {})
    if extras.get('idle_upper_body_symmetry_revision') == REVISION:
        if source != output:
            output.write_bytes(source.read_bytes())
        return None
    binary = bytearray(original)
    parents = {child: i for i, node in enumerate(doc['nodes'])
               for child in node.get('children', [])}
    uses = Counter(sampler['output'] for clip in doc['animations']
                   for sampler in clip['samplers'])
    changed = []
    modified_bytes = set()
    idle = next(clip for clip in doc['animations'] if clip['name'] == 'idle1')
    rotations = {channel['target']['node']: idle['samplers'][channel['sampler']]
                 for channel in idle['channels'] if channel['target']['path'] == 'rotation'}
    named = {doc['nodes'][node]['name']: node for node in rotations}

    def sample(node, times):
        if node not in rotations:
            return Rotation.from_quat(np.tile(doc['nodes'][node].get('rotation', [0, 0, 0, 1]), (len(times), 1)))
        sampler = rotations[node]
        assert sampler.get('interpolation', 'LINEAR') == 'LINEAR'
        keys = values(doc, original, sampler['input'])[:, 0]
        return Slerp(keys, Rotation.from_quat(values(doc, original, sampler['output'])))(times)

    def replace(name, quaternions):
        sampler = rotations[named[name]]
        index = sampler['output']
        assert uses[index] == 1, 'Modified rotation output is shared'
        target = values(doc, binary, index)
        before = values(doc, original, index)
        target[:] = continuous(quaternions)
        assert np.isfinite(target).all()
        accessor = doc['accessors'][index]
        view = doc['bufferViews'][accessor['bufferView']]
        start = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
        for i in range(len(target)):
            offset = start + i * view.get('byteStride', 16)
            modified_bytes.update(range(offset, offset + 16))
        for field, operation in [('min', np.min), ('max', np.max)]:
            if field in accessor:
                accessor[field] = operation(target, axis=0).tolist()
        angles = (Rotation.from_quat(before).inv() * Rotation.from_quat(target)).magnitude()
        changed.append(dict(bone=name, keys=len(target), maximum_change_degrees=float(np.degrees(angles).max())))

    for suffix in ['Shoulder', 'Arm', 'ForeArm', 'Hand']:
        left, right = [rotations[named[side + suffix]] for side in ['Left', 'Right']]
        assert left.get('interpolation', 'LINEAR') == right.get('interpolation', 'LINEAR') == 'LINEAR'
        assert np.array_equal(values(doc, original, left['input']), values(doc, original, right['input']))
        lq = continuous(values(doc, original, left['output']).astype(float))
        rq = continuous(values(doc, original, right['output']).astype(float) * MIRROR)
        rq *= np.where(np.sum(lq * rq, axis=1, keepdims=True) < 0, -1, 1)
        mean = continuous(lq + rq)
        replace('Left' + suffix, mean)
        replace('Right' + suffix, mean * MIRROR)

    spine = named['Spine2']
    times = values(doc, original, rotations[spine]['input'])[:, 0]
    chain, node = [], spine
    while node in parents:
        node = parents[node]
        chain.append(node)
    parent = Rotation.identity(len(times))
    for node in reversed(chain):
        parent = parent * sample(node, times)
    world = (parent * sample(spine, times)).as_quat()
    world[:, 1:3] = 0
    assert np.linalg.norm(world, axis=1).min() > .9
    replace('Spine2', (parent.inv() * Rotation.from_quat(continuous(world))).as_quat())
    assert len(changed) == 9
    assert all(a == b or i in modified_bytes for i, (a, b) in enumerate(zip(original, binary)))
    extras['idle_upper_body_symmetry_revision'] = REVISION
    write_glb(output, doc, bytes(binary))
    return dict(revision=REVISION, source_sha256=source_hash,
                output_sha256=hashlib.sha256(output.read_bytes()).hexdigest(), clip='idle1', changed_tracks=changed,
                method='Average mirrored clavicle, upper arm, forearm and hand rotations; remove world torso yaw/roll at Spine2 while retaining its X-axis twist.',
                rest_rig_unchanged=True, other_clips_and_tracks_unchanged=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=DEFAULT)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    result = symmetrize(args.input, args.output or args.input)
    if result:
        if args.report:
            args.report.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result))
