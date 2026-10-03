"""Bake a small, repeatable shoulder retraction into relaxed idle clips."""
import argparse
import math
import re
import struct
from pathlib import Path

from lib.glb import read_glb, write_glb

ROOT = Path(__file__).resolve().parents[1]
DEFAULT = ROOT / 'assets/modular_human_male_01/rigged_hand_tuned/animations.glb'


def adjust(source, output, degrees):
    doc, original = read_glb(source)
    scene = doc['scenes'][doc.get('scene', 0)]
    extras = scene.setdefault('extras', {})
    previous = extras.get('idle_shoulder_retraction_degrees', 0)
    if previous == degrees:
        if source != output:
            output.write_bytes(source.read_bytes())
        return
    binary = bytearray(original)
    targets = []
    uses = {}
    for clip in doc['animations']:
        for channel in clip['channels']:
            sampler = clip['samplers'][channel['sampler']]
            index = sampler['output']
            uses[index] = uses.get(index, 0) + 1
            bone = doc['nodes'][channel['target']['node']]['name']
            if (re.fullmatch(r'idle[1-5]', clip['name'])
                    and bone in ('LeftShoulder', 'RightShoulder')
                    and channel['target']['path'] == 'rotation'):
                assert sampler.get('interpolation', 'LINEAR') in ('LINEAR', 'STEP')
                targets.append((clip['name'], bone, index))
    idle_names = {clip['name'] for clip in doc['animations']
                  if re.fullmatch(r'idle[1-5]', clip['name'])}
    assert idle_names and len(targets) == len(idle_names) * 2
    changed_bytes = set()
    for name, bone, index in targets:
        assert uses[index] == 1, 'Shoulder output is shared with another channel'
        accessor = doc['accessors'][index]
        assert accessor['type'] == 'VEC4' and accessor['componentType'] == 5126
        assert 'sparse' not in accessor
        view = doc['bufferViews'][accessor['bufferView']]
        assert view.get('buffer', 0) == 0
        start = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
        stride = view.get('byteStride', 16)
        sign = 1 if bone == 'LeftShoulder' else -1
        angle = math.radians(sign * (degrees - previous)) / 2
        sine, cosine = math.sin(angle), math.cos(angle)
        values = []
        for i in range(accessor['count']):
            offset = start + i * stride
            x, y, z, w = struct.unpack_from('<4f', original, offset)
            value = (cosine*x + sine*z, cosine*y + sine*w,
                     cosine*z - sine*x, cosine*w - sine*y)
            assert all(math.isfinite(v) for v in value)
            struct.pack_into('<4f', binary, offset, *value)
            changed_bytes.update(range(offset, offset + 16))
            values.append(value)
        for field, operation in [('min', min), ('max', max)]:
            if field in accessor:
                accessor[field] = [operation(v[axis] for v in values) for axis in range(4)]
        print(f'{name}: {bone}, {accessor["count"]} keys, {sign * degrees:+g} degrees')
    assert all(a == b or i in changed_bytes
               for i, (a, b) in enumerate(zip(original, binary)))
    extras['idle_shoulder_retraction_degrees'] = degrees
    write_glb(output, doc, bytes(binary))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=DEFAULT)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--degrees', type=float, default=6)
    args = parser.parse_args()
    if not 0 <= args.degrees <= 12:
        parser.error('--degrees must be between 0 and 12')
    adjust(args.input, args.output or args.input, args.degrees)
