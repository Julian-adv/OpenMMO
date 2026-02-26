"""
fix_mixamo_transforms.py
========================
Mixamo 캐릭터의 transform을 정규화하는 Blender 스크립트.

사용법:
  1. Blender 상단 메뉴 → Scripting 탭
  2. Text Editor에서 Open → 이 파일 선택
  3. 아래 ── 설정 ── 섹션에서 TARGET_ARMATURE 지정 (필요한 경우)
  4. Run Script 버튼 (▶) 또는 Alt+P

수행 작업:
  1. Armature rotation (X축 90°) → 0 으로 Apply
  2. Armature/Mesh scale (0.01/100 등) → 1.0 으로 Apply
     (Mesh가 없는 animation-only Armature도 지원)
  3. Rest pose에서 발이 지면(z=0)에 닿도록 위치 보정
  * 각 Apply 후 animation keyframe 데이터를 자동으로 보정함
"""

import bpy
from mathutils import Vector

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# ── 설정 ──────────────────────────────────────────────────
# 처리할 Armature 이름. None 이면 씬의 첫 번째 Armature 자동 선택.
TARGET_ARMATURE = None   # 예: "Armature.001"
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━


def iter_fcurves(action):
    """Blender 5.x Layered Action / 4.x Legacy 모두 지원"""
    if action.is_action_layered:
        for layer in action.layers:
            for strip in layer.strips:
                for slot in action.slots:
                    cb = strip.channelbag(slot)
                    if cb:
                        yield from cb.fcurves
    else:
        yield from action.fcurves


def select_only(obj):
    bpy.ops.object.select_all(action='DESELECT')
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def fix_mixamo_transforms():
    # ── 오브젝트 탐색 ──────────────────────────────────────────
    if TARGET_ARMATURE:
        arm = bpy.data.objects.get(TARGET_ARMATURE)
        if arm is None:
            print(f"ERROR: '{TARGET_ARMATURE}' 오브젝트를 찾을 수 없습니다.")
            print(f"씬의 Armature 목록: {[o.name for o in bpy.data.objects if o.type == 'ARMATURE']}")
            return
    else:
        armatures = [o for o in bpy.data.objects if o.type == 'ARMATURE']
        if not armatures:
            print("ERROR: Armature 오브젝트를 찾을 수 없습니다.")
            return
        arm = armatures[0]

    # Armature에 직접 파렌팅된 Mesh 탐색 (없으면 None)
    mesh = next(
        (o for o in bpy.data.objects
         if o.type == 'MESH' and o.parent == arm),
        None
    )
    # 파렌팅된 Mesh가 없으면 씬 전체에서 탐색 (단, TARGET_ARMATURE 미지정 시에만)
    if mesh is None and TARGET_ARMATURE is None:
        meshes = [o for o in bpy.data.objects if o.type == 'MESH']
        mesh = meshes[0] if meshes else None

    if mesh:
        print(f"대상: Armature='{arm.name}', Mesh='{mesh.name}'")
        print(f"시작 상태: arm.rotation={[f'{v:.3f}' for v in arm.rotation_euler]}, "
              f"arm.scale={arm.scale[:]}, mesh.scale={mesh.scale[:]}")
    else:
        print(f"대상: Armature='{arm.name}' (Mesh 없음 - animation-only)")
        print(f"시작 상태: arm.rotation={[f'{v:.3f}' for v in arm.rotation_euler]}, "
              f"arm.scale={arm.scale[:]}")

    # ── STEP 1: Scale 비율 미리 기록 ───────────────────────────
    scale_factor = arm.scale.x
    print(f"\n[1] Scale factor 캡처: {scale_factor}")

    # ── STEP 2: Apply Rotation ─────────────────────────────────
    needs_rotation = any(abs(v) > 1e-4 for v in arm.rotation_euler)
    if needs_rotation:
        select_only(arm)
        bpy.ops.object.transform_apply(location=False, rotation=True, scale=False)
        print(f"[2] Rotation applied → {[f'{v:.4f}' for v in arm.rotation_euler]}")
    else:
        print("[2] Rotation이 이미 0, 건너뜀")

    # ── STEP 3: Apply Scale ────────────────────────────────────
    mesh_needs_scale = mesh is not None and abs(mesh.scale.x - 1.0) > 1e-4
    needs_scale = abs(arm.scale.x - 1.0) > 1e-4 or mesh_needs_scale
    if needs_scale:
        select_only(arm)
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
        if mesh is not None:
            select_only(mesh)
            bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
            print(f"[3] Scale applied → arm={arm.scale[:]}, mesh={mesh.scale[:]}")
        else:
            print(f"[3] Scale applied → arm={arm.scale[:]} (Mesh 없음, 건너뜀)")
    else:
        print("[3] Scale이 이미 1, 건너뜀")
        scale_factor = 1.0

    # ── STEP 4: Location F-curve를 scale_factor 배로 보정 ──────
    if needs_scale and abs(scale_factor - 1.0) > 1e-4:
        action = arm.animation_data.action if arm.animation_data else None
        if action:
            count = 0
            for fc in iter_fcurves(action):
                if 'location' in fc.data_path and 'pose.bones' in fc.data_path:
                    for kp in fc.keyframe_points:
                        kp.co[1]          *= scale_factor
                        kp.handle_left[1] *= scale_factor
                        kp.handle_right[1]*= scale_factor
                    fc.update()
                    count += 1
            bpy.context.view_layer.update()
            print(f"[4] Location F-curves ×{scale_factor} 보정: {count}개")
        else:
            print("[4] Animation data 없음, 건너뜀")
    else:
        print("[4] Scale 보정 불필요, 건너뜀")

    # ── STEP 5: Rest pose에서 발 위치 확인 ────────────────────
    arm.data.pose_position = 'REST'
    bpy.context.view_layer.update()

    foot_z_min = None
    toe_keywords = ['Toe_End', 'ToeBase', 'toe_end', 'toebase']
    for bone in arm.data.bones:
        if any(k in bone.name for k in toe_keywords):
            z = (arm.matrix_world @ bone.tail_local).z
            if foot_z_min is None or z < foot_z_min:
                foot_z_min = z

    if foot_z_min is None:
        print("[5] Toe bone을 찾지 못함, 발 위치 보정을 건너뜀")
        arm.data.pose_position = 'POSE'
        bpy.context.view_layer.update()
        print("\n완료 (발 보정 제외).")
        return

    offset = -foot_z_min
    print(f"[5] Rest pose 발 z={foot_z_min:.4f} → {offset:.4f}m 위로 이동")

    # ── STEP 6: Armature 위로 올리고 Apply Location ────────────
    if abs(offset) > 1e-4:
        arm.data.pose_position = 'POSE'
        arm.location.z += offset
        bpy.context.view_layer.update()

        select_only(arm)
        bpy.ops.object.transform_apply(location=True, rotation=False, scale=False)
        print(f"[6] Location applied → {arm.location[:]}")

        # ── STEP 7: Root bone location keyframe 보정 ──────────
        action = arm.animation_data.action if arm.animation_data else None
        if action:
            R     = arm.matrix_world.to_3x3()
            R_inv = R.inverted()
            root_bone = next((b for b in arm.data.bones if b.parent is None), None)

            if root_bone:
                B         = root_bone.matrix_local.to_3x3()
                B_inv     = B.inverted()
                world_corr      = Vector((0, 0, -offset))
                arm_local_corr  = R_inv @ world_corr
                bone_local_corr = B_inv @ arm_local_corr
                print(f"[7] Root bone '{root_bone.name}' 보정값: "
                      f"{[f'{v:.4f}' for v in bone_local_corr]}")

                root_path = f'pose.bones["{root_bone.name}"].location'
                fixed = 0
                for fc in iter_fcurves(action):
                    if fc.data_path == root_path:
                        corr = bone_local_corr[fc.array_index]
                        if abs(corr) > 1e-5:
                            for kp in fc.keyframe_points:
                                kp.co[1]          += corr
                                kp.handle_left[1] += corr
                                kp.handle_right[1]+= corr
                            fc.update()
                            fixed += 1
                print(f"[7] Keyframe 보정 완료: {fixed}개 채널")
            else:
                print("[7] Root bone을 찾지 못함")
        else:
            print("[7] Animation data 없음")
    else:
        print("[5/6] 발이 이미 z=0 근처, 위치 보정 불필요")

    # ── 검증 ────────────────────────────────────────────────────
    bpy.context.view_layer.update()
    print("\n─── 검증 ───")

    arm.data.pose_position = 'REST'
    bpy.context.view_layer.update()
    for bone in arm.data.bones:
        if 'Toe_End' in bone.name:
            z = (arm.matrix_world @ bone.tail_local).z
            print(f"  [REST]  {bone.name}: z={z:.4f}  (기대: ≈0.0)")

    arm.data.pose_position = 'POSE'
    bpy.context.view_layer.update()
    root_bone = next((b for b in arm.data.bones if b.parent is None), None)
    if root_bone:
        root_pb = arm.pose.bones[root_bone.name]
        world_z = (arm.matrix_world @ root_pb.head).z
        print(f"  [POSE]  {root_bone.name}: world z={world_z:.4f}")

    print("\n완료.")


fix_mixamo_transforms()
