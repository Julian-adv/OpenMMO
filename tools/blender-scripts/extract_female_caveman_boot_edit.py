from pathlib import Path
import bpy
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / 'assets/items/female_caveman_boots'
PATHS = [DIRECTORY / 'tripo_v1/fitted_v3/female-caveman-boots-fitted.blend',
         DIRECTORY / 'manual_edit_v1/user-edited.blend']
NAMES = ['boots_female_caveman_right', 'boots_female_caveman_left']


def snapshot():
    result = {}
    for name in NAMES:
        obj = bpy.data.objects[name]
        mesh = obj.data
        vertices = np.empty(len(mesh.vertices)*3, dtype=np.float32)
        mesh.vertices.foreach_get('co', vertices)
        loops = np.empty(len(mesh.loops), dtype=np.int32)
        mesh.loops.foreach_get('vertex_index', loops)
        uvs = np.empty(len(mesh.uv_layers.active.data)*2, dtype=np.float32)
        mesh.uv_layers.active.data.foreach_get('uv', uvs)
        weights = [[(group.group, group.weight) for group in vertex.groups] for vertex in mesh.vertices]
        keys = {}
        for key in mesh.shape_keys.key_blocks:
            values = np.empty(len(key.data)*3, dtype=np.float32)
            key.data.foreach_get('co', values)
            keys[key.name] = values.reshape(-1, 3)
        result[name] = {'vertices': vertices.reshape(-1, 3), 'loops': loops, 'uvs': uvs,
                        'weights': weights, 'groups': [group.name for group in obj.vertex_groups],
                        'matrix': np.asarray(obj.matrix_world), 'keys': keys}
    return result


snapshots = []
for path in PATHS:
    bpy.ops.wm.open_mainfile(filepath=str(path))
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
    snapshots.append(snapshot())
old, edited = snapshots
for name in NAMES:
    a, b = old[name], edited[name]
    assert np.array_equal(a['loops'], b['loops']), f'{name}: topology changed'
    assert np.array_equal(a['uvs'], b['uvs']), f'{name}: UVs changed'
    assert a['groups'] == b['groups'] and a['weights'] == b['weights'], f'{name}: weights changed'
    assert np.allclose(a['matrix'], b['matrix'], atol=1e-7), f'{name}: object transform changed'
    assert a['keys'].keys() == b['keys'].keys()
    delta = b['vertices']-a['vertices']
    for key in a['keys']:
        assert np.allclose(b['keys'][key]-a['keys'][key], delta, atol=1e-6), f'{name}: shape key changed independently'
assert np.allclose(old[NAMES[1]]['vertices'], edited[NAMES[1]]['vertices'], atol=1e-7), 'Left boot was edited'
np.savez_compressed(DIRECTORY / 'manual_edit_v1/mesh-edit.npz',
                    original_right=old[NAMES[0]]['vertices'], edited_right=edited[NAMES[0]]['vertices'],
                    original_left=old[NAMES[1]]['vertices'])
print('Extracted user mesh edit')
