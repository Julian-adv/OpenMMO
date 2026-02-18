import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'

export type RotationFixAxis = 'x' | 'y' | 'z'
export type RotationFixScope = 'root' | 'all'
export type RotationFixOrder = 'pre' | 'post'
export type MergeMethod = 'track-map' | 'retarget'

export interface RotationFixOptions {
  enabled: boolean
  axis: RotationFixAxis
  deg: number
  scope: RotationFixScope
  order: RotationFixOrder
}

export interface RetargetOptions {
  keepRootMotion: boolean
  normalizeRootStart: boolean
  keepVerticalRootMotion: boolean
}

export interface MergeOptions {
  animName: string
  mergeMethod: MergeMethod
  rotationFix: RotationFixOptions
  retarget: RetargetOptions
  selectedBClipIndex: number
}

export interface MergeStats {
  totalTracks: number
  mappedTracks: number
  correctedTracks: number
  mergedClipCount: number
}

export interface MergeClipsOutput {
  clips: THREE.AnimationClip[]
  stats: MergeStats
}

interface MatchResult {
  name: string
  score: number
  reason: string
}

interface NameIndex {
  idx: Map<string, Set<string>>
  originals: Set<string>
}

interface RotationFixConfig {
  axis: RotationFixAxis
  deg: number
  scope: RotationFixScope
  order: RotationFixOrder
  q: THREE.Quaternion
}

interface RetargetMapResult {
  names: Record<string, string>
  mappedCount: number
  targetCount: number
  sourceCount: number
  avgScore: number
  sourceHipName: string | null
  targetHipName: string | null
}

interface VerticalAxisInfo {
  axisIndex: number
  worldYPerLocalUnit: number
}

const RETARGET_TRACK_NAME_RE = /^\.bones\[(.+)]\.(position|quaternion)$/
const AXIS_LABELS = ['x', 'y', 'z'] as const

export function mergeAnimationClips(
  gltfA: GLTF,
  gltfB: GLTF,
  options: MergeOptions,
  log: (message: string) => void
): MergeClipsOutput {
  const aIndex = buildNameIndexFromScene(gltfA.scene)
  log(`a.glb 노드 수(이름 보유): ${aIndex.originals.size}`)

  const takenNames = new Set<string>()
  for (const clip of gltfA.animations ?? []) {
    const old = clip.name && clip.name.trim() ? clip.name : 'Animation'
    uniqueName(old, takenNames)
  }

  const rotFix = buildRotationFixConfig(options.rotationFix)
  if (rotFix) {
    log(
      `회전 보정 활성화: axis=${rotFix.axis}, deg=${rotFix.deg}, scope=${rotFix.scope}, order=${rotFix.order}`
    )
  }

  const bAnimations = gltfB.animations ?? []
  const srcClip = bAnimations[options.selectedBClipIndex]
  if (!srcClip) {
    log('b.glb에서 병합할 애니메이션이 없습니다.')
    throw new Error('선택된 B 클립이 없습니다.')
  }

  log(
    `b.glb 선택 클립 병합: index=${options.selectedBClipIndex} ("${srcClip.name}")`
  )

  const mergeResult =
    options.mergeMethod === 'retarget'
      ? mergeByRetarget(gltfA, gltfB, srcClip, options, rotFix, takenNames, log)
      : mergeByTrackMapping(srcClip, aIndex, options, rotFix, takenNames, log)

  log(
    `요약: 총 트랙 ${mergeResult.totalTracks}개 중 ${mergeResult.mappedTracks}개 매핑 성공`
  )
  if (rotFix) {
    log(`요약: 회전 보정 적용 트랙 ${mergeResult.correctedTracks}개`)
  }
  log(
    `병합된 클립 수: ${(gltfA.animations?.length ?? 0) + mergeResult.clips.length}`
  )

  return {
    clips: mergeResult.clips,
    stats: {
      totalTracks: mergeResult.totalTracks,
      mappedTracks: mergeResult.mappedTracks,
      correctedTracks: mergeResult.correctedTracks,
      mergedClipCount:
        (gltfA.animations?.length ?? 0) + mergeResult.clips.length,
    },
  }
}

function mergeByTrackMapping(
  srcClip: THREE.AnimationClip,
  aIndex: NameIndex,
  options: MergeOptions,
  rotFix: RotationFixConfig | null,
  takenNames: Set<string>,
  log: (message: string) => void
): {
  clips: THREE.AnimationClip[]
  totalTracks: number
  mappedTracks: number
  correctedTracks: number
} {
  const newTracks: THREE.KeyframeTrack[] = []
  let totalTracks = 0
  let mappedTracks = 0
  let correctedTracks = 0

  for (const track of srcClip.tracks) {
    totalTracks += 1
    const bNode = track.name.split('.')[0]
    const match = pickBestMatch(bNode, aIndex)

    if (!match) {
      log(`매치 실패: ${bNode}`)
      continue
    }

    const cloned = track.clone()
    cloned.name = cloned.name.replace(bNode, match.name)

    if (
      rotFix &&
      isQuaternionTrack(cloned) &&
      (rotFix.scope === 'all' || isLikelyRootNode(match.name))
    ) {
      applyQuaternionRotationFix(cloned, rotFix)
      correctedTracks += 1
    }

    newTracks.push(cloned)
    mappedTracks += 1
    log(
      `매핑: ${bNode} -> ${match.name} (reason=${match.reason}, score=${match.score.toFixed(3)})`
    )
  }

  if (newTracks.length === 0) {
    log(`클립 "${srcClip.name}" 매핑된 트랙 없음 -> 스킵`)
    return {
      clips: [],
      totalTracks,
      mappedTracks,
      correctedTracks,
    }
  }

  const newClip = srcClip.clone()
  newClip.tracks = newTracks
  newClip.name = uniqueName(options.animName, takenNames)

  log(
    `클립 "${srcClip.name}" -> "${newClip.name}" (${newTracks.length} 트랙 매핑 성공)`
  )

  return {
    clips: [newClip],
    totalTracks,
    mappedTracks,
    correctedTracks,
  }
}

function mergeByRetarget(
  gltfA: GLTF,
  gltfB: GLTF,
  srcClip: THREE.AnimationClip,
  options: MergeOptions,
  rotFix: RotationFixConfig | null,
  takenNames: Set<string>,
  log: (message: string) => void
): {
  clips: THREE.AnimationClip[]
  totalTracks: number
  mappedTracks: number
  correctedTracks: number
} {
  const targetRoot = SkeletonUtils.clone(gltfA.scene)
  const sourceRoot = SkeletonUtils.clone(gltfB.scene)

  const targetSkinned = findFirstSkinnedMesh(targetRoot)
  const sourceSkinned = findFirstSkinnedMesh(sourceRoot)

  if (!targetSkinned) {
    throw new Error('a.glb에서 SkinnedMesh를 찾을 수 없습니다.')
  }
  if (!sourceSkinned) {
    throw new Error('b.glb에서 SkinnedMesh를 찾을 수 없습니다.')
  }

  const mapResult = buildRetargetNameMap(targetSkinned, sourceSkinned)
  if (mapResult.mappedCount === 0) {
    throw new Error('리타겟 본 매핑 실패: 이름 매칭 결과가 없습니다.')
  }

  log(
    `리타겟 매핑: target ${mapResult.targetCount}개 / source ${mapResult.sourceCount}개 중 ${mapResult.mappedCount}개 연결 (avg score=${mapResult.avgScore.toFixed(3)})`
  )

  const sourceHipName = mapResult.sourceHipName
  if (sourceHipName) {
    log(
      `리타겟 hip: source=${sourceHipName}, target=${mapResult.targetHipName ?? '(unknown)'}`
    )
  } else {
    log('리타겟 hip 자동 탐지 실패: root 모션 품질이 떨어질 수 있습니다.')
  }

  const retargetedClip = SkeletonUtils.retargetClip(
    targetSkinned,
    sourceSkinned,
    srcClip,
    {
      names: mapResult.names,
      hip: sourceHipName ?? undefined,
      useFirstFramePosition: options.retarget.normalizeRootStart,
      preserveBoneMatrix: true,
      preserveHipPosition: true,
    }
  )

  const normalizedTracks = retargetedClip.tracks.map((track) => {
    const cloned = track.clone()
    cloned.name = normalizeRetargetTrackName(cloned.name)
    return cloned
  })

  let correctedTracks = 0
  const targetHipName = mapResult.targetHipName
  const verticalAxisInfo =
    targetHipName !== null
      ? detectRootVerticalAxis(gltfA.scene, targetHipName, log)
      : { axisIndex: 1, worldYPerLocalUnit: 1 }
  const upAxisIndex = verticalAxisInfo.axisIndex

  const adjustedTracks: THREE.KeyframeTrack[] = []
  for (const track of normalizedTracks) {
    if (
      !options.retarget.keepRootMotion &&
      targetHipName &&
      track.name === `${targetHipName}.position`
    ) {
      continue
    }

    if (
      rotFix &&
      isQuaternionTrack(track) &&
      (rotFix.scope === 'all' || isLikelyRootNode(track.name.split('.')[0]))
    ) {
      applyQuaternionRotationFix(track, rotFix)
      correctedTracks += 1
    }

    adjustedTracks.push(track)
  }

  if (
    options.retarget.keepRootMotion &&
    targetHipName &&
    options.retarget.normalizeRootStart
  ) {
    alignRootStartToReference(
      gltfA,
      adjustedTracks,
      targetHipName,
      upAxisIndex,
      options.retarget.keepVerticalRootMotion,
      log
    )
  } else if (
    options.retarget.keepRootMotion &&
    targetHipName &&
    !options.retarget.keepVerticalRootMotion
  ) {
    flattenRootVerticalMotion(
      gltfA,
      adjustedTracks,
      targetHipName,
      upAxisIndex,
      log
    )
  }

  // normalizeRootStart=true 일 때도 이 블록이 실행된다.
  // alignRootStartToReference 가 수직 위치를 고정한 뒤, lockClipToGround 가
  // 실제 발 본 위치를 기준으로 정밀 지면 스냅을 추가 적용하는 것이 의도된 동작이다.
  if (
    options.retarget.keepRootMotion &&
    targetHipName &&
    !options.retarget.keepVerticalRootMotion
  ) {
    lockClipToGround(
      gltfA,
      adjustedTracks,
      targetHipName,
      retargetedClip.duration,
      upAxisIndex,
      verticalAxisInfo.worldYPerLocalUnit,
      log
    )
  }

  if (adjustedTracks.length === 0) {
    throw new Error('리타겟 결과 트랙이 비어 있습니다.')
  }

  const newClip = srcClip.clone()
  newClip.tracks = adjustedTracks
  newClip.name = uniqueName(options.animName, takenNames)

  log(
    `클립 "${srcClip.name}" -> "${newClip.name}" (retarget, ${adjustedTracks.length} 트랙)`
  )

  return {
    clips: [newClip],
    totalTracks: srcClip.tracks.length,
    mappedTracks: adjustedTracks.length,
    correctedTracks,
  }
}

function normalizeRetargetTrackName(name: string): string {
  const match = RETARGET_TRACK_NAME_RE.exec(name)
  if (!match) return name
  return `${match[1]}.${match[2]}`
}

function alignRootStartToReference(
  gltfA: GLTF,
  tracks: THREE.KeyframeTrack[],
  targetHipName: string,
  upAxisIndex: number,
  keepVerticalRootMotion: boolean,
  log: (message: string) => void
): void {
  const rootTrack = tracks.find(
    (track): track is THREE.VectorKeyframeTrack =>
      track.name === `${targetHipName}.position` &&
      track instanceof THREE.VectorKeyframeTrack &&
      track.values.length >= 3
  )

  if (!rootTrack) {
    return
  }

  const refStart = findReferenceHipStart(gltfA.animations ?? [], targetHipName)
  if (!refStart) {
    log(`루트 시작점 정렬 스킵: 기준 hip track 없음 (${targetHipName})`)
    return
  }

  const values = rootTrack.values
  const offsetX = refStart.x - values[0]
  const offsetY = refStart.y - values[1]
  const offsetZ = refStart.z - values[2]
  const refStartByAxis = [refStart.x, refStart.y, refStart.z]

  for (let i = 0; i < values.length; i += 3) {
    values[i] += offsetX
    values[i + 1] += offsetY
    values[i + 2] += offsetZ

    if (!keepVerticalRootMotion) {
      values[i + upAxisIndex] = refStartByAxis[upAxisIndex]
    }
  }

  log(
    keepVerticalRootMotion
      ? `루트 시작점 정렬 적용: ${targetHipName}.position offset=(${offsetX.toFixed(4)}, ${offsetY.toFixed(4)}, ${offsetZ.toFixed(4)})`
      : `루트 시작점 정렬 + ${AXIS_LABELS[upAxisIndex]} 고정: ${targetHipName}.position`
  )
}

function flattenRootVerticalMotion(
  gltfA: GLTF,
  tracks: THREE.KeyframeTrack[],
  targetHipName: string,
  upAxisIndex: number,
  log: (message: string) => void
): void {
  const rootTrack = tracks.find(
    (track): track is THREE.VectorKeyframeTrack =>
      track.name === `${targetHipName}.position` &&
      track instanceof THREE.VectorKeyframeTrack &&
      track.values.length >= 3
  )

  if (!rootTrack) return

  const refStart = findReferenceHipStart(gltfA.animations ?? [], targetHipName)
  const fixedValue =
    refStart !== null
      ? [refStart.x, refStart.y, refStart.z][upAxisIndex]
      : rootTrack.values[upAxisIndex]
  const values = rootTrack.values

  for (let i = upAxisIndex; i < values.length; i += 3) {
    values[i] = fixedValue
  }

  log(
    `루트 ${AXIS_LABELS[upAxisIndex]} 모션 고정 적용: ${targetHipName}.position ${AXIS_LABELS[upAxisIndex]}=${fixedValue.toFixed(4)}`
  )
}

function lockClipToGround(
  gltfA: GLTF,
  tracks: THREE.KeyframeTrack[],
  targetHipName: string,
  duration: number,
  upAxisIndex: number,
  worldYPerLocalUnit: number,
  log: (message: string) => void
): void {
  const rootTrack = tracks.find(
    (track): track is THREE.VectorKeyframeTrack =>
      track.name === `${targetHipName}.position` &&
      track instanceof THREE.VectorKeyframeTrack &&
      track.values.length >= 3
  )

  if (!rootTrack || rootTrack.times.length === 0) return

  const sampleCount = Math.max(rootTrack.times.length * 2, 180)
  const resampled = resampleVectorTrack(rootTrack, duration, sampleCount)
  rootTrack.times = resampled.times
  rootTrack.values = resampled.values
  rootTrack.setInterpolation(THREE.InterpolateLinear)

  const probeRoot = SkeletonUtils.clone(gltfA.scene)
  const probeGroup = new THREE.Group()
  probeGroup.add(probeRoot)

  const probeClip = new THREE.AnimationClip('__ground_lock__', duration, tracks)
  const probeMixer = new THREE.AnimationMixer(probeGroup)
  const probeAction = probeMixer.clipAction(probeClip)
  probeAction.play()
  const probeHip = probeRoot.getObjectByName(targetHipName)
  const groundBoneNames = collectGroundBoneNames(probeRoot)

  const sampleGroundY = (time: number): number => {
    probeMixer.setTime(time)
    probeGroup.updateMatrixWorld(true)
    const boneMinY = sampleGroundYFromBones(probeRoot, groundBoneNames)
    if (boneMinY !== null) return boneMinY
    const fallbackBox = new THREE.Box3().setFromObject(probeGroup)
    return fallbackBox.min.y
  }

  if (!probeHip) {
    log(`지면 고정 스킵: probe hip 본(${targetHipName})을 찾을 수 없음`)
    return
  }

  const probeSensitivityAt = (time: number, axisIndex: number): number => {
    const epsilon = 0.01
    const baseMinY = sampleGroundY(time)

    const original = probeHip.position.getComponent(axisIndex)
    probeHip.position.setComponent(axisIndex, original + epsilon)
    probeGroup.updateMatrixWorld(true)
    const shiftedMinY =
      sampleGroundYFromBones(probeRoot, groundBoneNames) ??
      new THREE.Box3().setFromObject(probeGroup).min.y
    probeHip.position.setComponent(axisIndex, original)
    probeGroup.updateMatrixWorld(true)

    return (shiftedMinY - baseMinY) / epsilon
  }

  if (Math.abs(worldYPerLocalUnit) < 1e-8) {
    log(
      `지면 고정 스킵: 수직축 영향도가 너무 작음 (${worldYPerLocalUnit.toFixed(6)})`
    )
    return
  }

  const targetGroundMinY = sampleReferenceGroundMinY(gltfA, groundBoneNames)
  if (groundBoneNames.length > 0) {
    log(`지면 기준 본: ${groundBoneNames.join(', ')}`)
  } else {
    log('지면 기준 본 탐지 실패: bbox fallback 사용')
  }
  const values = rootTrack.values

  let correctedFrames = 0
  let maxLift = 0
  let maxDrop = 0
  let tailStabilized = false

  const PASS_COUNT = 3
  for (let pass = 0; pass < PASS_COUNT; pass += 1) {
    for (let i = 0; i < rootTrack.times.length; i += 1) {
      const time = rootTrack.times[i]
      const minY = sampleGroundY(time)
      const deltaWorld = targetGroundMinY - minY
      if (Math.abs(deltaWorld) <= 1e-5) continue

      const localSensitivity = probeSensitivityAt(time, upAxisIndex)
      const effectiveSensitivity =
        Math.abs(localSensitivity) > 1e-6
          ? localSensitivity
          : worldYPerLocalUnit
      const deltaLocal = THREE.MathUtils.clamp(
        deltaWorld / effectiveSensitivity,
        -50,
        50
      )
      values[i * 3 + upAxisIndex] += deltaLocal
      correctedFrames += 1
      if (deltaWorld > maxLift) maxLift = deltaWorld
      if (deltaWorld < maxDrop) maxDrop = deltaWorld
    }
  }

  // Clamp terminal key on vertical axis to avoid end-frame pop while preserving motion.
  if (rootTrack.times.length >= 2) {
    const lastFrameIndex = rootTrack.times.length - 1
    const prevValue = values[(lastFrameIndex - 1) * 3 + upAxisIndex]
    const lastValue = values[lastFrameIndex * 3 + upAxisIndex]
    if (Math.abs(lastValue - prevValue) > 1e-5) {
      values[lastFrameIndex * 3 + upAxisIndex] = prevValue
      tailStabilized = true
    }
  }

  let residualBelow = 0
  let residualAbove = 0
  for (let i = 0; i < rootTrack.times.length; i += 1) {
    const minY = sampleGroundY(rootTrack.times[i])
    const diff = targetGroundMinY - minY
    if (diff > residualBelow) residualBelow = diff
    if (-diff > residualAbove) residualAbove = -diff
  }

  if (correctedFrames > 0) {
    log(
      `지면 고정 적용: targetMinY=${targetGroundMinY.toFixed(4)}, 보정=${correctedFrames}회, maxUp=+${maxLift.toFixed(4)}, maxDown=${maxDrop.toFixed(4)}, residualBelow=${residualBelow.toFixed(4)}, residualAbove=${residualAbove.toFixed(4)} (${AXIS_LABELS[upAxisIndex]}축, influence=${worldYPerLocalUnit.toFixed(4)})`
    )
    if (tailStabilized) {
      log(
        `끝프레임 안정화 적용: ${targetHipName}.position.${AXIS_LABELS[upAxisIndex]}`
      )
    }
  } else {
    log('지면 고정 적용: 추가 보정 불필요')
  }
}

function resampleVectorTrack(
  track: THREE.VectorKeyframeTrack,
  duration: number,
  sampleCount: number
): { times: Float32Array; values: Float32Array } {
  const count = Math.max(2, Math.floor(sampleCount))
  const times = new Float32Array(count + 1)
  const values = new Float32Array((count + 1) * 3)

  for (let i = 0; i <= count; i += 1) {
    const t = (duration * i) / count
    times[i] = t
    const sampled = sampleVectorTrackAtTime(track, t)
    values[i * 3] = sampled[0]
    values[i * 3 + 1] = sampled[1]
    values[i * 3 + 2] = sampled[2]
  }

  return { times, values }
}

function sampleVectorTrackAtTime(
  track: THREE.VectorKeyframeTrack,
  t: number
): [number, number, number] {
  const times = track.times
  const values = track.values
  if (times.length === 0) return [0, 0, 0]

  if (t <= times[0]) return [values[0], values[1], values[2]]

  const lastIndex = times.length - 1
  if (t >= times[lastIndex]) {
    return [
      values[lastIndex * 3],
      values[lastIndex * 3 + 1],
      values[lastIndex * 3 + 2],
    ]
  }

  let lo = 0
  let hi = lastIndex
  while (lo + 1 < hi) {
    const mid = (lo + hi) >> 1
    if (times[mid] <= t) lo = mid
    else hi = mid
  }

  const t0 = times[lo]
  const t1 = times[lo + 1]
  const alpha = t1 > t0 ? (t - t0) / (t1 - t0) : 0

  const i0 = lo * 3
  const i1 = (lo + 1) * 3
  return [
    values[i0] + (values[i1] - values[i0]) * alpha,
    values[i0 + 1] + (values[i1 + 1] - values[i0 + 1]) * alpha,
    values[i0 + 2] + (values[i1 + 2] - values[i0 + 2]) * alpha,
  ]
}

function sampleReferenceGroundMinY(
  gltfA: GLTF,
  preferredBoneNames: string[]
): number {
  const probeRoot = SkeletonUtils.clone(gltfA.scene)
  const probeGroup = new THREE.Group()
  probeGroup.add(probeRoot)
  probeGroup.updateMatrixWorld(true)

  const preferred = ['idle1', 'idle', 'walk', 'jog', 'run']
  const refClip =
    preferred
      .map((name) =>
        (gltfA.animations ?? []).find(
          (clip) => canonicalKey(clip.name) === canonicalKey(name)
        )
      )
      .find((clip) => clip !== undefined) ?? (gltfA.animations ?? [])[0]

  if (refClip) {
    const mixer = new THREE.AnimationMixer(probeGroup)
    const action = mixer.clipAction(refClip)
    action.play()
    mixer.setTime(0)
    probeGroup.updateMatrixWorld(true)
  }

  const targetBoneNames =
    preferredBoneNames.length > 0
      ? preferredBoneNames
      : collectGroundBoneNames(probeRoot)
  const boneMinY = sampleGroundYFromBones(probeRoot, targetBoneNames)
  if (boneMinY !== null) return boneMinY

  const box = new THREE.Box3().setFromObject(probeGroup)
  return box.min.y
}

function collectGroundBoneNames(root: THREE.Object3D): string[] {
  const names = new Set<string>()

  root.traverse((node) => {
    if (!(node instanceof THREE.Bone)) return
    const key = canonicalKey(node.name).replace(/_/g, '')
    const isGroundBone =
      key.includes('toe') || key.includes('foot') || key.includes('ankle')
    const isUpperLeg =
      key.includes('upleg') || key.includes('thigh') || key.includes('calf')
    if (!isGroundBone || isUpperLeg) return
    names.add(node.name)
  })

  return Array.from(names)
}

function sampleGroundYFromBones(
  root: THREE.Object3D,
  boneNames: string[]
): number | null {
  if (boneNames.length === 0) return null

  let minY = Infinity
  let found = false
  for (const name of boneNames) {
    const bone = root.getObjectByName(name)
    if (!bone) continue
    const y = bone.getWorldPosition(new THREE.Vector3()).y
    if (y < minY) minY = y
    found = true
  }

  return found ? minY : null
}

function detectRootVerticalAxis(
  scene: THREE.Object3D,
  hipBoneName: string,
  log: (message: string) => void
): VerticalAxisInfo {
  const probeRoot = SkeletonUtils.clone(scene)
  const hipBone = probeRoot.getObjectByName(hipBoneName)

  if (!hipBone) {
    log(`수직축 자동 감지 실패: hip 본(${hipBoneName}) 없음, 기본 y 사용`)
    return { axisIndex: 1, worldYPerLocalUnit: 1 }
  }
  const hip = hipBone

  probeRoot.updateMatrixWorld(true)
  const baseWorldPos = hip.getWorldPosition(new THREE.Vector3())
  const responses = [0, 0, 0]

  for (let axis = 0; axis < 3; axis += 1) {
    const original = hip.position.getComponent(axis)
    hip.position.setComponent(axis, original + 1)
    probeRoot.updateMatrixWorld(true)
    const movedWorldPos = hip.getWorldPosition(new THREE.Vector3())
    responses[axis] = movedWorldPos.y - baseWorldPos.y
    hip.position.setComponent(axis, original)
  }

  probeRoot.updateMatrixWorld(true)

  let selected = 0
  if (Math.abs(responses[1]) > Math.abs(responses[selected])) selected = 1
  if (Math.abs(responses[2]) > Math.abs(responses[selected])) selected = 2

  log(
    `수직축 자동 감지: hip=${hipBoneName}, response=[${responses.map((v) => v.toFixed(3)).join(', ')}], selected=${AXIS_LABELS[selected]}, influence=${responses[selected].toFixed(4)}`
  )
  return {
    axisIndex: selected,
    worldYPerLocalUnit: responses[selected],
  }
}

function findReferenceHipStart(
  clips: THREE.AnimationClip[],
  targetHipName: string
): THREE.Vector3 | null {
  const trackName = `${targetHipName}.position`
  const preferred = ['idle1', 'idle', 'walk', 'jog', 'run']

  const preferredClip =
    preferred
      .map((name) =>
        clips.find((clip) => canonicalKey(clip.name) === canonicalKey(name))
      )
      .find((clip) => clip !== undefined) ?? null

  const fromPreferred = preferredClip?.tracks.find(
    (track): track is THREE.VectorKeyframeTrack =>
      track.name === trackName &&
      track instanceof THREE.VectorKeyframeTrack &&
      track.values.length >= 3
  )?.values

  if (fromPreferred && fromPreferred.length >= 3) {
    return new THREE.Vector3(
      fromPreferred[0],
      fromPreferred[1],
      fromPreferred[2]
    )
  }

  for (const clip of clips) {
    const track = clip.tracks.find(
      (t): t is THREE.VectorKeyframeTrack =>
        t.name === trackName &&
        t instanceof THREE.VectorKeyframeTrack &&
        t.values.length >= 3
    )

    if (track) {
      return new THREE.Vector3(
        track.values[0],
        track.values[1],
        track.values[2]
      )
    }
  }

  return null
}

function buildRetargetNameMap(
  targetSkinned: THREE.SkinnedMesh,
  sourceSkinned: THREE.SkinnedMesh
): RetargetMapResult {
  const targetBones = targetSkinned.skeleton.bones
  const sourceBones = sourceSkinned.skeleton.bones

  const sourceIndex = buildNameIndexFromBones(sourceBones)
  const sourceHipName = guessHipBoneName(sourceBones)

  const names: Record<string, string> = {}
  let mappedCount = 0
  let scoreSum = 0
  let targetHipName: string | null = null

  for (const targetBone of targetBones) {
    const match = pickBestMatch(targetBone.name, sourceIndex, 0.74)
    if (!match) continue

    names[targetBone.name] = match.name
    mappedCount += 1
    scoreSum += match.score

    if (sourceHipName && match.name === sourceHipName) {
      targetHipName = targetBone.name
    }
  }

  if (!targetHipName) {
    targetHipName = guessHipBoneName(targetBones)
  }

  return {
    names,
    mappedCount,
    targetCount: targetBones.length,
    sourceCount: sourceBones.length,
    avgScore: mappedCount > 0 ? scoreSum / mappedCount : 0,
    sourceHipName,
    targetHipName,
  }
}

function findFirstSkinnedMesh(root: THREE.Object3D): THREE.SkinnedMesh | null {
  let found: THREE.SkinnedMesh | null = null

  root.traverse((node) => {
    if (found) return
    if ((node as THREE.SkinnedMesh).isSkinnedMesh) {
      found = node as THREE.SkinnedMesh
    }
  })

  return found
}

function guessHipBoneName(
  bones: Array<THREE.Bone | { name: string }>
): string | null {
  const candidates = ['mixamorighips', 'hips', 'pelvis', 'hip']

  for (const c of candidates) {
    const found = bones.find((bone) => {
      const key = canonicalKey(bone.name).replace(/_/g, '')
      return key === c || key.endsWith(c)
    })

    if (found) return found.name
  }

  return null
}

function buildNameIndexFromBones(
  bones: Array<THREE.Bone | { name: string }>
): NameIndex {
  const idx = new Map<string, Set<string>>()
  const originals = new Set<string>()

  for (const bone of bones) {
    if (!bone.name) continue

    const raw = bone.name
    originals.add(raw)
    const key = canonicalKey(raw)
    if (!key) continue

    if (!idx.has(key)) {
      idx.set(key, new Set<string>())
    }

    idx.get(key)?.add(raw)
  }

  return { idx, originals }
}

function buildNameIndexFromScene(scene: THREE.Object3D): NameIndex {
  const idx = new Map<string, Set<string>>()
  const originals = new Set<string>()

  scene.traverse((obj) => {
    if (!obj.name) return

    const raw = obj.name
    originals.add(raw)
    const key = canonicalKey(raw)
    if (!key) return

    if (!idx.has(key)) {
      idx.set(key, new Set<string>())
    }

    idx.get(key)?.add(raw)
  })

  return { idx, originals }
}

function normalizeSide(name: string): string {
  let n = name.replace(/\.l\b/gi, '_l').replace(/\.r\b/gi, '_r')
  n = n.replace(/[\s-]+l\b/gi, '_l').replace(/[\s-]+r\b/gi, '_r')
  return n
}

function stripNumericSuffix(name: string): string {
  let n = name
  for (let i = 0; i < 3; i += 1) {
    n = n.replace(/([_.\s-]?\(?\d{1,4}\)?)$/, '')
  }
  return n
}

function canonicalKey(name: string): string {
  if (!name) return ''
  let n = normalizeSide(name)
  n = stripNumericSuffix(n)
  n = n.toLowerCase()
  n = n.replace(/[^a-z0-9_]/g, '')
  n = n.replace(/_+/g, '_').replace(/^_+|_+$/g, '')
  return n
}

function levenshtein(a: string, b: string): number {
  const m = a.length
  const n = b.length
  if (m === 0) return n
  if (n === 0) return m

  const dp = new Array<number>(n + 1)
  for (let j = 0; j <= n; j += 1) dp[j] = j

  for (let i = 1; i <= m; i += 1) {
    let prev = dp[0]
    dp[0] = i

    for (let j = 1; j <= n; j += 1) {
      const temp = dp[j]
      const cost = a[i - 1] === b[j - 1] ? 0 : 1
      dp[j] = Math.min(dp[j] + 1, dp[j - 1] + 1, prev + cost)
      prev = temp
    }
  }

  return dp[n]
}

function similarity(a: string, b: string): number {
  if (!a || !b) return 0
  const d = levenshtein(a, b)
  const L = Math.max(a.length, b.length)
  return 1 - d / (L || 1)
}

function pickBestMatch(
  sourceNameRaw: string,
  targetIndex: NameIndex,
  threshold: number = 0.82
): MatchResult | null {
  const sourceKey = canonicalKey(sourceNameRaw)

  if (targetIndex.idx.has(sourceKey)) {
    const cands = Array.from(targetIndex.idx.get(sourceKey) ?? [])
    let best = cands[0]
    let bestScore = -1

    for (const cand of cands) {
      const score = similarity(canonicalKey(cand), sourceKey)
      if (score > bestScore) {
        bestScore = score
        best = cand
      }
    }

    return { name: best, score: 1.0, reason: 'exact-key' }
  }

  const stripped = canonicalKey(
    stripNumericSuffix(normalizeSide(sourceNameRaw))
  )
  if (targetIndex.idx.has(stripped)) {
    const cands = Array.from(targetIndex.idx.get(stripped) ?? [])
    return { name: cands[0], score: 0.98, reason: 'stripped-suffix' }
  }

  const sideNorm = canonicalKey(normalizeSide(sourceNameRaw))
  if (targetIndex.idx.has(sideNorm)) {
    const cands = Array.from(targetIndex.idx.get(sideNorm) ?? [])
    return { name: cands[0], score: 0.96, reason: 'side-normalized' }
  }

  let bestName: string | null = null
  let bestScore = 0

  for (const [targetKey, setRaw] of targetIndex.idx.entries()) {
    const score = similarity(sourceKey, targetKey)
    if (score > bestScore) {
      bestScore = score
      bestName = setRaw.values().next().value ?? null
    }
  }

  if (bestName && bestScore >= threshold) {
    return { name: bestName, score: bestScore, reason: 'fuzzy' }
  }

  return null
}

function isQuaternionTrack(
  track: THREE.KeyframeTrack
): track is THREE.QuaternionKeyframeTrack {
  return (
    track.name.endsWith('.quaternion') &&
    track.values.length % 4 === 0 &&
    track instanceof THREE.QuaternionKeyframeTrack
  )
}

function isLikelyRootNode(name: string): boolean {
  const key = canonicalKey(name).replace(/_/g, '')
  return (
    key === 'root' ||
    key === 'armature' ||
    key === 'hips' ||
    key === 'pelvis' ||
    key === 'mixamorighips' ||
    key === 'skeletonroot' ||
    key.endsWith('root')
  )
}

function buildRotationFixConfig(
  options: RotationFixOptions
): RotationFixConfig | null {
  if (!options.enabled) return null
  if (!Number.isFinite(options.deg) || Math.abs(options.deg) < 1e-8) return null

  const q = new THREE.Quaternion()
  const rad = THREE.MathUtils.degToRad(options.deg)

  if (options.axis === 'x') q.setFromAxisAngle(new THREE.Vector3(1, 0, 0), rad)
  else if (options.axis === 'y')
    q.setFromAxisAngle(new THREE.Vector3(0, 1, 0), rad)
  else q.setFromAxisAngle(new THREE.Vector3(0, 0, 1), rad)

  return {
    axis: options.axis,
    deg: options.deg,
    scope: options.scope,
    order: options.order,
    q,
  }
}

function applyQuaternionRotationFix(
  track: THREE.QuaternionKeyframeTrack,
  fixCfg: RotationFixConfig
): void {
  const q = new THREE.Quaternion()
  const values = track.values

  for (let i = 0; i < values.length; i += 4) {
    q.set(values[i], values[i + 1], values[i + 2], values[i + 3])

    if (fixCfg.order === 'post') q.multiply(fixCfg.q)
    else q.premultiply(fixCfg.q)

    q.normalize()
    values[i] = q.x
    values[i + 1] = q.y
    values[i + 2] = q.z
    values[i + 3] = q.w
  }
}

function uniqueName(base: string, taken: Set<string>): string {
  const raw = base && base.trim() ? base.trim() : 'Clip'
  if (!taken.has(raw)) {
    taken.add(raw)
    return raw
  }

  let i = 1
  while (taken.has(`${raw}_${i}`)) i += 1

  const finalName = `${raw}_${i}`
  taken.add(finalName)
  return finalName
}
