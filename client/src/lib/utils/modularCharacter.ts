import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'

interface SwordTransform {
  position: [number, number, number]
  quaternion: [number, number, number, number]
}

interface SwordPose extends SwordTransform {
  keyframes?: (SwordTransform & { phase: number })[]
}

export interface ModularHandProfile {
  rig_id: string
  iron_sword: SwordPose
  iron_sword_by_clip?: Record<string, SwordPose>
  finger_pose_sources: Record<string, Record<string, string>>
  finger_relaxation?: Record<
    string,
    Record<
      string,
      {
        rest_quaternion: [number, number, number, number]
        amount: number
      }
    >
  >
}

export function modularRigId(root: THREE.Object3D): string {
  const ids = new Set<string>()
  root.traverse((node) => {
    if (typeof node.userData.rig_id === 'string') ids.add(node.userData.rig_id)
  })
  if (ids.size !== 1) throw new Error('파츠의 리그 ID가 없거나 서로 다릅니다.')
  return [...ids][0]
}

export function skinnedParts(root: THREE.Object3D): THREE.SkinnedMesh[] {
  const result: THREE.SkinnedMesh[] = []
  root.traverse((node) => {
    if (node instanceof THREE.SkinnedMesh) result.push(node)
  })
  return result
}

export interface ModularOutfit {
  hair: 'hair_crop' | 'hair_sidepart' | 'none'
  top: 'linen' | 'leather' | 'none'
  gloves: boolean
  boots: boolean
}

export function showModularOutfit(
  body: THREE.SkinnedMesh[],
  parts: ReadonlyMap<string, THREE.SkinnedMesh[]>,
  outfit: ModularOutfit
) {
  const region = (mesh: THREE.Object3D): string | undefined => {
    for (let node: THREE.Object3D | null = mesh; node; node = node.parent)
      if (typeof node.userData.region === 'string') return node.userData.region
  }
  const hidden = new Set(['legs'])
  if (outfit.top !== 'none') {
    for (const region of ['torso', 'upper_arms', 'forearms']) hidden.add(region)
  }
  if (outfit.gloves) hidden.add('hands')
  if (outfit.boots) hidden.add('feet')
  for (const mesh of body) mesh.visible = !hidden.has(region(mesh) ?? '')
  const selected = new Set(['pants_cloth', outfit.hair])
  if (outfit.top !== 'none') selected.add('top_linen')
  if (outfit.top === 'leather') selected.add('top_leather')
  if (outfit.gloves) selected.add('gloves_leather')
  if (outfit.boots) selected.add('boots_leather')
  for (const [id, meshes] of parts)
    for (const mesh of meshes)
      mesh.visible =
        selected.has(id) &&
        !(
          id === 'pants_cloth' &&
          outfit.boots &&
          mesh.userData.region === 'cuffs'
        ) &&
        !(
          id === 'pants_cloth' &&
          !outfit.boots &&
          mesh.userData.region === 'tucked_cuffs'
        ) &&
        !(
          id === 'pants_cloth' &&
          outfit.top === 'leather' &&
          mesh.userData.region === 'waist'
        ) &&
        !(
          id === 'top_linen' &&
          outfit.top === 'leather' &&
          mesh.userData.region === 'torso'
        )
  return selected
}

function matrixMatches(a: THREE.Matrix4, b: THREE.Matrix4): boolean {
  return a.elements.every((value, i) => Math.abs(value - b.elements[i]) < 1e-5)
}

export function modularAnimationClips(
  body: THREE.Object3D,
  pack: { scene: THREE.Object3D; animations: THREE.AnimationClip[] },
  variant: 'corrected' | 'comparison'
): THREE.AnimationClip[] {
  if (
    modularRigId(body) !== modularRigId(pack.scene) ||
    pack.scene.userData.animation_stage !== 'modular-baked-v1' ||
    pack.scene.userData.variant !== variant
  )
    throw new Error('전용 애니메이션의 리그 또는 보정 단계가 다릅니다.')
  const bones = skinnedParts(body)[0]?.skeleton.bones
  if (!bones?.length) throw new Error('몸체의 스킨 골격이 없습니다.')
  body.updateMatrixWorld(true)
  pack.scene.updateMatrixWorld(true)
  for (const bone of bones) {
    const source = pack.scene.getObjectByName(bone.name)
    if (
      !source ||
      source.parent?.name !== bone.parent?.name ||
      !matrixMatches(source.matrix, bone.matrix) ||
      !matrixMatches(source.matrixWorld, bone.matrixWorld)
    )
      throw new Error('전용 애니메이션의 기준 골격이 몸체와 다릅니다.')
  }
  const boneNames = new Set(bones.map((bone) => bone.name))
  const clipNames = new Set<string>()
  for (const clip of pack.animations) {
    const tracks = new Set<string>()
    if (!clip.name || clipNames.has(clip.name) || !clip.validate())
      throw new Error('전용 애니메이션 클립이 올바르지 않습니다.')
    clipNames.add(clip.name)
    for (const track of clip.tracks) {
      const match = /^(\w+)\.(position|quaternion|scale)$/.exec(track.name)
      if (!match || !boneNames.has(match[1]) || tracks.has(track.name))
        throw new Error('전용 애니메이션에 알 수 없는 본 트랙이 있습니다.')
      tracks.add(track.name)
    }
  }
  return pack.animations
}

export function bindModularPart(
  body: THREE.Object3D,
  source: THREE.Object3D
): THREE.SkinnedMesh[] {
  if (modularRigId(body) !== modularRigId(source)) {
    throw new Error('몸체와 파츠의 리그 ID가 다릅니다.')
  }
  const bodyMeshes = skinnedParts(body)
  const skeleton = bodyMeshes[0]?.skeleton
  const sourceMeshes = skinnedParts(source)
  if (!skeleton || sourceMeshes.length === 0) {
    throw new Error('몸체 또는 파츠에 스킨 골격이 없습니다.')
  }
  for (const mesh of [...bodyMeshes, ...sourceMeshes]) {
    const candidate = mesh.skeleton
    if (
      candidate.bones.length !== skeleton.bones.length ||
      candidate.bones.some(
        (bone, i) =>
          bone.name !== skeleton.bones[i].name ||
          bone.parent?.name !== skeleton.bones[i].parent?.name ||
          !matrixMatches(candidate.boneInverses[i], skeleton.boneInverses[i])
      )
    ) {
      throw new Error('파츠의 본 순서·계층·기준 행렬이 몸체와 다릅니다.')
    }
  }
  const part = clone(source)
  part.updateMatrixWorld(true)
  const meshes = skinnedParts(part)
  const discarded = new Set([
    ...meshes.map((mesh) => mesh.skeleton),
    ...bodyMeshes
      .map((mesh) => mesh.skeleton)
      .filter((candidate) => candidate !== skeleton),
  ])
  for (const mesh of bodyMeshes) mesh.bind(skeleton, mesh.bindMatrix.clone())
  const inverseRoot = part.matrixWorld.clone().invert()
  for (const mesh of meshes) {
    mesh.matrix.multiplyMatrices(inverseRoot, mesh.matrixWorld)
    mesh.matrix.decompose(mesh.position, mesh.quaternion, mesh.scale)
    body.add(mesh)
    mesh.bind(skeleton, mesh.bindMatrix.clone())
    mesh.castShadow = mesh.receiveShadow = true
  }
  for (const unused of discarded) unused.dispose()
  return meshes
}

export function parseModularHandProfile(value: unknown): ModularHandProfile {
  const profile = value as ModularHandProfile | null
  const finiteTuple = (values: unknown, size: number) =>
    Array.isArray(values) &&
    values.length === size &&
    values.every((v) => typeof v === 'number' && Number.isFinite(v))
  if (
    !profile ||
    typeof profile.rig_id !== 'string' ||
    !profile.rig_id ||
    !finiteTuple(profile.iron_sword?.position, 3) ||
    !finiteTuple(profile.iron_sword?.quaternion, 4) ||
    !profile.finger_pose_sources ||
    typeof profile.finger_pose_sources !== 'object' ||
    Array.isArray(profile.finger_pose_sources)
  ) {
    throw new Error('손 보정 프로파일 형식이 올바르지 않습니다.')
  }
  const norm = Math.hypot(...profile.iron_sword.quaternion)
  if (Math.abs(norm - 1) > 1e-4)
    throw new Error('검 장착 회전이 정규화되지 않았습니다.')
  if (profile.iron_sword_by_clip !== undefined) {
    if (
      !profile.iron_sword_by_clip ||
      typeof profile.iron_sword_by_clip !== 'object' ||
      Array.isArray(profile.iron_sword_by_clip) ||
      Object.values(profile.iron_sword_by_clip).some(
        (pose) =>
          !pose ||
          !finiteTuple(pose.position, 3) ||
          !finiteTuple(pose.quaternion, 4) ||
          Math.abs(Math.hypot(...pose.quaternion) - 1) > 1e-4
      )
    ) {
      throw new Error('동작별 검 장착 설정이 올바르지 않습니다.')
    }
  }
  for (const pose of [
    profile.iron_sword,
    ...Object.values(profile.iron_sword_by_clip ?? {}),
  ]) {
    if (pose.keyframes === undefined) continue
    const frames = pose.keyframes
    if (
      !Array.isArray(frames) ||
      frames.length < 2 ||
      frames.some(
        (frame, i) =>
          !frame ||
          !Number.isFinite(frame.phase) ||
          frame.phase < 0 ||
          frame.phase > 1 ||
          (i > 0 && frame.phase <= frames[i - 1].phase) ||
          !finiteTuple(frame.position, 3) ||
          !finiteTuple(frame.quaternion, 4) ||
          Math.abs(Math.hypot(...frame.quaternion) - 1) > 1e-4
      ) ||
      frames[0].phase !== 0 ||
      frames[frames.length - 1].phase !== 1
    )
      throw new Error('검 장착 키프레임이 올바르지 않습니다.')
  }
  const finger = /^(Left|Right)Hand(Thumb|Index|Middle|Ring|Pinky)[123]$/
  for (const mapping of Object.values(profile.finger_pose_sources)) {
    if (
      !mapping ||
      typeof mapping !== 'object' ||
      Array.isArray(mapping) ||
      Object.entries(mapping).some(
        ([target, source]) =>
          !finger.test(target) ||
          typeof source !== 'string' ||
          !finger.test(source)
      )
    ) {
      throw new Error('손가락 회전 트랙 매핑이 올바르지 않습니다.')
    }
  }
  if (profile.finger_relaxation !== undefined) {
    if (
      !profile.finger_relaxation ||
      typeof profile.finger_relaxation !== 'object' ||
      Array.isArray(profile.finger_relaxation)
    ) {
      throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
    }
    for (const mapping of Object.values(profile.finger_relaxation)) {
      if (!mapping || typeof mapping !== 'object' || Array.isArray(mapping)) {
        throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
      }
      for (const [bone, adjustment] of Object.entries(mapping)) {
        if (
          !finger.test(bone) ||
          !adjustment ||
          !finiteTuple(adjustment.rest_quaternion, 4) ||
          Math.abs(Math.hypot(...adjustment.rest_quaternion) - 1) > 1e-4 ||
          !Number.isFinite(adjustment.amount) ||
          adjustment.amount < 0 ||
          adjustment.amount > 1
        ) {
          throw new Error('손가락 펴기 설정이 올바르지 않습니다.')
        }
      }
    }
  }
  return profile
}

export function applyModularFingerPose(
  clip: THREE.AnimationClip,
  profile: ModularHandProfile,
  rigId: string
): THREE.AnimationClip {
  if (profile.rig_id !== rigId)
    throw new Error('손 보정 프로파일의 리그가 다릅니다.')
  const mapping = profile.finger_pose_sources[clip.name]
  const relaxation = profile.finger_relaxation?.[clip.name]
  if (!mapping && !relaxation) return clip
  const adjusted = clip.clone()
  for (const [target, source] of Object.entries(mapping ?? {})) {
    const track = clip.tracks.find(
      (track) => track.name === `${source}.quaternion`
    )
    if (!(track instanceof THREE.QuaternionKeyframeTrack)) {
      throw new Error(`손가락 회전 트랙이 없습니다: ${clip.name}/${source}`)
    }
    adjusted.tracks = adjusted.tracks.filter(
      (track) => track.name !== `${target}.quaternion`
    )
    const replacement = track.clone()
    replacement.name = `${target}.quaternion`
    adjusted.tracks.push(replacement)
  }
  for (const [bone, adjustment] of Object.entries(relaxation ?? {})) {
    const track = adjusted.tracks.find(
      (track) => track.name === `${bone}.quaternion`
    )
    if (!(track instanceof THREE.QuaternionKeyframeTrack)) {
      throw new Error(`손가락 회전 트랙이 없습니다: ${clip.name}/${bone}`)
    }
    const rest = new THREE.Quaternion().fromArray(adjustment.rest_quaternion)
    const rotation = new THREE.Quaternion()
    for (let i = 0; i < track.values.length; i += 4) {
      rotation
        .fromArray(track.values, i)
        .slerp(rest, adjustment.amount)
        .normalize()
        .toArray(track.values, i)
    }
  }
  return adjusted
}

function swordPose(
  profile: ModularHandProfile,
  rigId: string,
  clipName?: string
): SwordPose {
  if (profile.rig_id !== rigId)
    throw new Error('검 장착 프로파일의 리그가 다릅니다.')
  return (
    (clipName && profile.iron_sword_by_clip?.[clipName]) || profile.iron_sword
  )
}

export function modularSwordTracks(
  profile: ModularHandProfile,
  rigId: string,
  clip: THREE.AnimationClip,
  targetName: string
): [THREE.VectorKeyframeTrack, THREE.QuaternionKeyframeTrack] {
  const pose = swordPose(profile, rigId, clip.name)
  const frames = pose.keyframes ?? [{ phase: 0, ...pose }]
  const times = frames.map((frame) => frame.phase * clip.duration)
  return [
    new THREE.VectorKeyframeTrack(
      `${targetName}.position`,
      times,
      frames.flatMap((frame) => frame.position)
    ),
    new THREE.QuaternionKeyframeTrack(
      `${targetName}.quaternion`,
      times,
      frames.flatMap((frame) => frame.quaternion)
    ),
  ]
}

export function poseModularSword(
  prop: THREE.Object3D,
  profile: ModularHandProfile,
  rigId: string,
  clipName?: string,
  phase = 0
): void {
  const pose = swordPose(profile, rigId, clipName)
  const frames = pose.keyframes
  if (!frames) {
    prop.position.fromArray(pose.position)
    prop.quaternion.fromArray(pose.quaternion)
    return
  }
  const at = THREE.MathUtils.clamp(phase, 0, 1)
  const index = Math.max(
    1,
    frames.findIndex((frame) => frame.phase >= at)
  )
  const previous = frames[index - 1],
    next = frames[index]
  const amount = (at - previous.phase) / (next.phase - previous.phase)
  prop.position
    .fromArray(previous.position)
    .lerp(new THREE.Vector3().fromArray(next.position), amount)
  prop.quaternion
    .fromArray(previous.quaternion)
    .slerp(new THREE.Quaternion().fromArray(next.quaternion), amount)
}
