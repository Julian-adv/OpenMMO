import json
from pathlib import Path

import numpy as np
from scipy.spatial.transform import Rotation

from lib.glb import read_glb, write_glb
from lib.glb_accessors import add_accessor

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/monsters/thief_king/rig_v1/thief_king.glb'
OUTPUT = ROOT / 'assets/monsters/thief_king/game_v1/thief_king.glb'
CLIPS = Path('/tmp/thief-king-bake/clips.json')
FIT = ROOT / 'assets/monsters/thief_king/grip_v1/fit.json'


def build():
    doc, source = read_glb(SOURCE)
    binary = bytearray(source)
    data = json.loads(CLIPS.read_text())
    fit = json.loads(FIT.read_text())
    nodes = {node['name']: i for i, node in enumerate(doc['nodes'])}
    doc['animations'] = []
    for clip in data['clips']:
        channels, samplers = [], []
        grip_tracks = {}
        for finger, angles in fit['fingers_degrees_x'].items():
            for i, angle in enumerate(angles, 1):
                name = f'RightHand{finger}{i}'
                node = doc['nodes'][nodes[name]]
                rotation = Rotation.from_quat(node['rotation']) * Rotation.from_euler('x', angle, degrees=True)
                if finger == 'Thumb':
                    rotation *= Rotation.from_euler('y', fit['thumb_degrees_y'][i - 1], degrees=True)
                    rotation *= Rotation.from_euler('z', fit['thumb_degrees_z'][i - 1], degrees=True)
                grip_tracks[name + '.quaternion'] = {'name': name + '.quaternion', 'times': [0, clip['duration']], 'values': rotation.as_quat().tolist() * 2, 'size': 4}
        tracks = [track for track in clip['tracks'] if track['name'] not in grip_tracks] + list(grip_tracks.values())
        for track in tracks:
            name, prop = track['name'].split('.')
            times = np.asarray(track['times'], dtype=np.float32).reshape(-1, 1)
            values = np.asarray(track['values'], dtype=np.float32).reshape(len(times), track['size'])
            assert np.all(np.diff(times[:, 0]) > 0), track['name']
            input_index = add_accessor(doc, binary, times, 'SCALAR')
            doc['accessors'][input_index].update(min=[float(times.min())], max=[float(times.max())])
            output_index = add_accessor(doc, binary, values, 'VEC4' if track['size'] == 4 else 'VEC3')
            samplers.append({'input': input_index, 'output': output_index, 'interpolation': 'LINEAR'})
            channels.append({'sampler': len(samplers) - 1, 'target': {'node': nodes[name], 'path': 'rotation' if prop == 'quaternion' else 'translation'}})
        doc['animations'].append({'name': clip['name'], 'channels': channels, 'samplers': samplers})
    doc['buffers'][0]['byteLength'] = len(binary)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    write_glb(OUTPUT, doc, binary)
    print('Wrote', OUTPUT, 'with', [clip['name'] for clip in data['clips']])


if __name__ == '__main__':
    build()
