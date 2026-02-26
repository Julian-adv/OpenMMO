"""
add_action_to_nla.py
====================
임포트한 애니메이션 Armature에서 액션을 꺼내
메인 Armature의 NLA에 트랙으로 추가하는 스크립트.

사용법:
  1. 새 애니메이션 .fbx/.glb 임포트
  2. 아래 ── 설정 ── 에서 ACTION_NAME 지정
  3. Run Script (▶ 또는 Alt+P)
  4. 임포트한 Armature.001 등은 삭제해도 됨
"""

import bpy

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# ── 설정 ──────────────────────────────────────────────────
# NLA에 추가할 액션 이름 (None 이면 가장 최근 임포트된 액션 자동 선택)
ACTION_NAME = None        # 예: "idle3"

# 대상 Armature 이름 (None 이면 씬의 첫 번째 Armature 자동 선택)
TARGET_ARMATURE = None    # 예: "Armature"
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━


def add_action_to_nla():
    # ── 대상 Armature 탐색 ────────────────────────────────
    if TARGET_ARMATURE:
        arm = bpy.data.objects.get(TARGET_ARMATURE)
        if not arm:
            print(f"ERROR: '{TARGET_ARMATURE}' 를 찾을 수 없습니다.")
            return
    else:
        arm = next((o for o in bpy.data.objects if o.type == 'ARMATURE'), None)
        if not arm:
            print("ERROR: Armature를 찾을 수 없습니다.")
            return

    # ── 추가할 액션 탐색 ──────────────────────────────────
    if ACTION_NAME:
        action = bpy.data.actions.get(ACTION_NAME)
        if not action:
            print(f"ERROR: 액션 '{ACTION_NAME}' 를 찾을 수 없습니다.")
            print(f"현재 액션 목록: {[a.name for a in bpy.data.actions]}")
            return
    else:
        # NLA에 아직 없는 액션 중 가장 마지막으로 추가된 것 선택
        existing = {
            strip.action
            for track in (arm.animation_data.nla_tracks if arm.animation_data else [])
            for strip in track.strips
            if strip.action
        }
        candidates = [a for a in bpy.data.actions if a not in existing]
        if not candidates:
            print("추가할 새 액션이 없습니다.")
            print(f"현재 액션 목록: {[a.name for a in bpy.data.actions]}")
            return
        action = candidates[-1]
        print(f"자동 선택된 액션: '{action.name}'")

    # ── animation_data 확인/생성 ──────────────────────────
    if arm.animation_data is None:
        arm.animation_data_create()

    anim = arm.animation_data

    # ── 이미 NLA에 있는지 확인 ────────────────────────────
    for track in anim.nla_tracks:
        for strip in track.strips:
            if strip.action == action:
                print(f"'{action.name}' 은 이미 NLA 트랙 '{track.name}' 에 있습니다.")
                return

    # ── active action 해제 (1프레임 stash 방지) ───────────
    anim.action = None

    # ── 새 NLA 트랙 + 스트립 추가 ────────────────────────
    track = anim.nla_tracks.new()
    track.name = action.name
    strip = track.strips.new(action.name, int(action.frame_range[0]), action)

    print(f"완료: NLA 트랙 '{track.name}' 추가됨")
    print(f"  스트립: frames {strip.frame_start:.0f} - {strip.frame_end:.0f}")

    # ── 현재 NLA 전체 출력 ────────────────────────────────
    print(f"\n[{arm.name}] 전체 NLA 트랙:")
    for t in anim.nla_tracks:
        for s in t.strips:
            name = s.action.name if s.action else '(no action)'
            print(f"  [{t.name}]  '{s.name}'  frames {s.frame_start:.0f}-{s.frame_end:.0f}")


add_action_to_nla()
