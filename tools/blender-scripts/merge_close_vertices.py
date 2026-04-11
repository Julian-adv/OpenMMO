import bpy

MESH_NAME = 'tripo_node_e6b902c3'
THRESHOLD = 0.0001

bpy.ops.object.select_all(action='DESELECT')

mesh_obj = bpy.data.objects[MESH_NAME]
bpy.context.view_layer.objects.active = mesh_obj
mesh_obj.select_set(True)

bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.select_all(action='SELECT')
bpy.ops.mesh.remove_doubles(threshold=THRESHOLD)
bpy.ops.object.mode_set(mode='OBJECT')
