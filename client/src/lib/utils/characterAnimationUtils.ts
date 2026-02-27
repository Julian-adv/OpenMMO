import * as THREE from 'three'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
import { AnimationName } from '../types/animations'

export const LOCOMOTION_WAIT_TIMEOUT_MS = 2000

type AnimationSource = 'base' | 'locomotion' | 'combat_melee'

export interface OrderedAnimationSelection {
  name: AnimationName
  clip: THREE.AnimationClip
  source: AnimationSource
  fromFallback: boolean
}

export interface RetargetSourceScenes {
  base?: THREE.Object3D | null
  locomotion?: THREE.Object3D | null
  combatMelee?: THREE.Object3D | null
}

const ANIMATION_SOURCE_BY_NAME: Record<AnimationName, AnimationSource> = {
  [AnimationName.IDLE1]: 'locomotion',
  [AnimationName.IDLE2]: 'locomotion',
  [AnimationName.IDLE3]: 'locomotion',
  [AnimationName.IDLE4]: 'locomotion',
  [AnimationName.IDLE5]: 'locomotion',
  [AnimationName.WALK]: 'locomotion',
  [AnimationName.JOG]: 'locomotion',
  [AnimationName.RUN]: 'locomotion',
  [AnimationName.SLASH1]: 'combat_melee',
  [AnimationName.SLASH2]: 'combat_melee',
  [AnimationName.SLASH3]: 'combat_melee',
  [AnimationName.SLASH4]: 'combat_melee',
  [AnimationName.SLASH5]: 'combat_melee',
  [AnimationName.ATTACK1]: 'combat_melee',
  [AnimationName.ATTACK2]: 'combat_melee',
  [AnimationName.ATTACK3]: 'combat_melee',
  [AnimationName.ATTACK4]: 'combat_melee',
  [AnimationName.DYING]: 'combat_melee',
}

const RETARGET_TRACK_NAME_PATTERN = /^\.bones\[(.+?)\]\.(position|quaternion)$/
const HIP_BONE_CANDIDATES = [
  'Hips',
  'hips',
  'Hip',
  'hip',
  'Pelvis',
  'pelvis',
  'mixamorigHips',
] as const
const retargetedClipCache = new Map<string, THREE.AnimationClip>()
const ENABLE_RUNTIME_BONE_RETARGETING = true

export function getGltfAnimations(gltf: unknown): THREE.AnimationClip[] {
  if (!gltf || typeof gltf !== 'object' || !('animations' in gltf)) return []

  const animations = (gltf as { animations?: unknown }).animations
  return Array.isArray(animations) ? (animations as THREE.AnimationClip[]) : []
}

export function createCharacterModelRoot(sourceScene: THREE.Object3D): {
  clonedScene: THREE.Object3D
  modelRoot: THREE.Group
} {
  const clonedScene = SkeletonUtils.clone(sourceScene) as THREE.Object3D
  const modelRoot = new THREE.Group()
  modelRoot.add(clonedScene)

  modelRoot.traverse((child) => {
    if (child instanceof THREE.Mesh) {
      child.castShadow = true
      child.receiveShadow = true
    }
  })

  return { clonedScene, modelRoot }
}

function findPrimarySkinnedMesh(
  root: THREE.Object3D
): THREE.SkinnedMesh | null {
  let bestMatch: THREE.SkinnedMesh | null = null

  root.traverse((child) => {
    if (!(child instanceof THREE.SkinnedMesh) || !child.skeleton) return
    if (
      !bestMatch ||
      child.skeleton.bones.length > bestMatch.skeleton.bones.length
    ) {
      bestMatch = child
    }
  })

  return bestMatch
}

function quaternionDistance(a: THREE.Quaternion, b: THREE.Quaternion): number {
  const direct = Math.hypot(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w)
  const negated = Math.hypot(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w)
  return Math.min(direct, negated)
}

function roundForProfile(value: number): number {
  return Math.round(value * 1000) / 1000
}

function buildSkeletonProfileKey(skinnedMesh: THREE.SkinnedMesh): string {
  const sortedBones = [...skinnedMesh.skeleton.bones].sort((a, b) =>
    a.name.localeCompare(b.name)
  )
  return sortedBones
    .map((bone) =>
      [
        bone.name,
        roundForProfile(bone.position.x),
        roundForProfile(bone.position.y),
        roundForProfile(bone.position.z),
        roundForProfile(bone.quaternion.x),
        roundForProfile(bone.quaternion.y),
        roundForProfile(bone.quaternion.z),
        roundForProfile(bone.quaternion.w),
        roundForProfile(bone.scale.x),
        roundForProfile(bone.scale.y),
        roundForProfile(bone.scale.z),
      ].join(':')
    )
    .join('|')
}

function hasEquivalentSkeletonRestPose(
  targetSkinnedMesh: THREE.SkinnedMesh,
  sourceSkinnedMesh: THREE.SkinnedMesh
): boolean {
  const targetBones = targetSkinnedMesh.skeleton.bones.filter(
    (bone) => bone.name.length > 0
  )
  const sourceBoneByName = new Map(
    sourceSkinnedMesh.skeleton.bones
      .filter((bone) => bone.name.length > 0)
      .map((bone) => [bone.name, bone])
  )

  const commonBones = targetBones.filter((bone) =>
    sourceBoneByName.has(bone.name)
  )
  const coverage =
    commonBones.length / Math.max(targetBones.length, sourceBoneByName.size)
  if (coverage < 0.95) return false

  for (const targetBone of commonBones) {
    const sourceBone = sourceBoneByName.get(targetBone.name)
    if (!sourceBone) return false

    if (targetBone.position.distanceTo(sourceBone.position) > 0.001)
      return false
    if (targetBone.scale.distanceTo(sourceBone.scale) > 0.001) return false
    if (
      quaternionDistance(targetBone.quaternion, sourceBone.quaternion) > 0.001
    ) {
      return false
    }
  }

  return true
}

function normalizeRetargetedClipTrackNames(
  retargetedClip: THREE.AnimationClip,
  originalClipName: string
): THREE.AnimationClip {
  let renamedTrackFound = false
  const convertedTracks: THREE.KeyframeTrack[] = []

  for (const track of retargetedClip.tracks) {
    const match = RETARGET_TRACK_NAME_PATTERN.exec(track.name)
    if (!match) {
      convertedTracks.push(track)
      continue
    }

    const [, boneName, property] = match

    const renamedTrack = track.clone()
    renamedTrack.name = `${boneName}.${property}`
    renamedTrackFound = true
    convertedTracks.push(renamedTrack)
  }

  if (!renamedTrackFound) return retargetedClip

  return new THREE.AnimationClip(
    originalClipName,
    retargetedClip.duration,
    convertedTracks
  )
}

function buildBoneNameMap(
  targetSkinnedMesh: THREE.SkinnedMesh,
  sourceSkinnedMesh: THREE.SkinnedMesh
): Record<string, string> {
  const sourceBoneNames = new Set(
    sourceSkinnedMesh.skeleton.bones
      .map((bone) => bone.name)
      .filter((name) => name.length > 0)
  )
  const nameMap: Record<string, string> = {}

  for (const targetBone of targetSkinnedMesh.skeleton.bones) {
    if (!targetBone.name || !sourceBoneNames.has(targetBone.name)) continue
    nameMap[targetBone.name] = targetBone.name
  }

  return nameMap
}

function resolveHipBoneName(sourceSkinnedMesh: THREE.SkinnedMesh): string {
  const sourceBoneNames = new Set(
    sourceSkinnedMesh.skeleton.bones.map((bone) => bone.name)
  )
  return (
    HIP_BONE_CANDIDATES.find((boneName) => sourceBoneNames.has(boneName)) ??
    sourceSkinnedMesh.skeleton.bones[0]?.name ??
    'Hips'
  )
}

function getHipBoneRestY(
  skinnedMesh: THREE.SkinnedMesh,
  hipBoneName: string
): number | null {
  const hipBone = skinnedMesh.skeleton.bones.find(
    (b) => b.name === hipBoneName
  )
  return hipBone ? hipBone.position.y : null
}

function correctHipHeightInClip(
  clip: THREE.AnimationClip,
  hipBoneName: string,
  yDelta: number
): void {
  const track = clip.tracks.find((t) => t.name === `${hipBoneName}.position`)
  if (!track) return

  // Position values are stored as [x, y, z, x, y, z, ...]
  const values = track.values
  for (let i = 1; i < values.length; i += 3) {
    values[i] += yDelta
  }
}

export function retargetAnimationsForCharacterModel(
  targetScene: THREE.Object3D,
  retargetSourceScene: THREE.Object3D | null | undefined,
  clips: THREE.AnimationClip[]
): THREE.AnimationClip[] {
  if (!ENABLE_RUNTIME_BONE_RETARGETING) return clips
  if (clips.length === 0 || !retargetSourceScene) return clips

  // Operate on clones only. Both target and source scenes can come from shared
  // loader instances, and retarget internals mutate skeleton transforms.
  const targetSceneClone = SkeletonUtils.clone(targetScene) as THREE.Object3D

  // `retargetSourceScene` comes from a shared loader cache. Retargeting mutates
  // skeleton state (`pose`, matrix updates), so work on a clone to avoid
  // leaking transforms back into female_knight.glb previews.
  const retargetSourceClone = SkeletonUtils.clone(
    retargetSourceScene
  ) as THREE.Object3D

  const targetSkinnedMesh = findPrimarySkinnedMesh(targetSceneClone)
  const sourceSkinnedMesh = findPrimarySkinnedMesh(retargetSourceClone)
  if (!targetSkinnedMesh || !sourceSkinnedMesh) return clips

  targetSkinnedMesh.skeleton.pose()
  sourceSkinnedMesh.skeleton.pose()
  targetSkinnedMesh.updateMatrixWorld(true)
  sourceSkinnedMesh.updateMatrixWorld(true)

  if (hasEquivalentSkeletonRestPose(targetSkinnedMesh, sourceSkinnedMesh)) {
    return clips
  }

  const boneNameMap = buildBoneNameMap(targetSkinnedMesh, sourceSkinnedMesh)
  if (Object.keys(boneNameMap).length === 0) return clips

  const targetProfileKey = buildSkeletonProfileKey(targetSkinnedMesh)
  const sourceProfileKey = buildSkeletonProfileKey(sourceSkinnedMesh)
  const hipBoneName = resolveHipBoneName(sourceSkinnedMesh)

  const targetHipY = getHipBoneRestY(targetSkinnedMesh, hipBoneName)
  const sourceHipY = getHipBoneRestY(sourceSkinnedMesh, hipBoneName)
  const hipYDelta =
    targetHipY !== null && sourceHipY !== null ? targetHipY - sourceHipY : 0

  console.log(
    `[retarget] hip="${hipBoneName}" targetY=${targetHipY} sourceY=${sourceHipY} delta=${hipYDelta}`
  )

  const retargetedClips = clips.map((clip) => {
    const cacheKey = `${targetProfileKey}::${sourceProfileKey}::${clip.uuid}`
    const cachedClip = retargetedClipCache.get(cacheKey)
    if (cachedClip) return cachedClip

    try {
      targetSkinnedMesh.skeleton.pose()
      targetSkinnedMesh.updateMatrixWorld(true)

      const retargetedClip = SkeletonUtils.retargetClip(
        targetSkinnedMesh,
        sourceSkinnedMesh,
        clip,
        {
          names: boneNameMap,
          hip: hipBoneName,
          preserveBoneMatrix: true,
          useTargetMatrix: false,
          useFirstFramePosition: false,
        }
      )
      const normalizedClip = normalizeRetargetedClipTrackNames(
        retargetedClip,
        clip.name
      )
      if (normalizedClip.tracks.length === 0) {
        return clip
      }
      if (Math.abs(hipYDelta) > 0.001) {
        correctHipHeightInClip(normalizedClip, hipBoneName, hipYDelta)
      }
      retargetedClipCache.set(cacheKey, normalizedClip)
      return normalizedClip
    } catch (error) {
      console.warn(`Failed to retarget animation clip "${clip.name}"`, error)
      return clip
    }
  })

  return retargetedClips
}

export function retargetOrderedCharacterAnimationsForModel(
  targetScene: THREE.Object3D,
  orderedSelections: OrderedAnimationSelection[],
  sourceScenes: RetargetSourceScenes
): THREE.AnimationClip[] {
  if (!ENABLE_RUNTIME_BONE_RETARGETING) {
    return orderedSelections.map((selection) => selection.clip)
  }

  const bySource = {
    base: orderedSelections.filter((selection) => selection.source === 'base'),
    locomotion: orderedSelections.filter(
      (selection) => selection.source === 'locomotion'
    ),
    combat_melee: orderedSelections.filter(
      (selection) => selection.source === 'combat_melee'
    ),
  }

  const retargetedBySource = {
    base: retargetAnimationsForCharacterModel(
      targetScene,
      sourceScenes.base,
      bySource.base.map((selection) => selection.clip)
    ),
    locomotion: retargetAnimationsForCharacterModel(
      targetScene,
      sourceScenes.locomotion,
      bySource.locomotion.map((selection) => selection.clip)
    ),
    combat_melee: retargetAnimationsForCharacterModel(
      targetScene,
      sourceScenes.combatMelee ?? sourceScenes.locomotion,
      bySource.combat_melee.map((selection) => selection.clip)
    ),
  }

  let baseIndex = 0
  let locomotionIndex = 0
  let combatMeleeIndex = 0

  return orderedSelections.map((selection) => {
    if (selection.source === 'base') {
      const clip = retargetedBySource.base[baseIndex]
      baseIndex += 1
      return clip ?? selection.clip
    }

    if (selection.source === 'locomotion') {
      const clip = retargetedBySource.locomotion[locomotionIndex]
      locomotionIndex += 1
      return clip ?? selection.clip
    }

    const clip = retargetedBySource.combat_melee[combatMeleeIndex]
    combatMeleeIndex += 1
    return clip ?? selection.clip
  })
}

export function selectOrderedCharacterAnimations(
  baseAnimations: THREE.AnimationClip[],
  locomotionAnimations: THREE.AnimationClip[],
  combatMeleeAnimations: THREE.AnimationClip[]
): OrderedAnimationSelection[] {
  const baseClipByName = new Map(
    baseAnimations.map((clip) => [clip.name, clip])
  )
  const locomotionClipByName = new Map(
    locomotionAnimations.map((clip) => [clip.name, clip])
  )
  const combatMeleeClipByName = new Map(
    combatMeleeAnimations.map((clip) => [clip.name, clip])
  )
  const firstBaseClip = baseAnimations[0]
  const firstLocomotionClip = locomotionAnimations[0]
  const firstCombatMeleeClip = combatMeleeAnimations[0]
  const orderedSelections: OrderedAnimationSelection[] = []

  for (const name of Object.values(AnimationName)) {
    const source = ANIMATION_SOURCE_BY_NAME[name]
    if (!source) {
      console.error(
        `No animation source mapping defined for "${name}"; update animation source map.`
      )
      return []
    }

    const selectedClip =
      source === 'locomotion'
        ? locomotionClipByName.get(name)
        : source === 'combat_melee'
          ? combatMeleeClipByName.get(name)
          : baseClipByName.get(name)

    let clip = selectedClip
    let fromFallback = false

    if (!clip) {
      const fallbackClip =
        source === 'locomotion'
          ? firstLocomotionClip
          : source === 'combat_melee'
            ? firstCombatMeleeClip
            : firstBaseClip

      if (fallbackClip) {
        clip = fallbackClip
        fromFallback = true
        console.warn(
          `Missing animation "${name}" in ${source}.glb; using first ${source} clip "${fallbackClip.name}" as fallback.`
        )
      }
    }

    if (!clip) {
      console.error(
        `Missing animation "${name}" in ${source}.glb and no fallback clip is available.`
      )
      return []
    }

    orderedSelections.push({
      name,
      clip,
      source,
      fromFallback,
    })
  }

  return orderedSelections
}
