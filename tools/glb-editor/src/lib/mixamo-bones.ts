import type * as THREE from 'three'
import { canonicalKey } from './merge'

/**
 * Standard Mixamo 65-bone skeleton names (without prefix).
 * No "mixamorig:" prefix — Three.js PropertyBinding cannot parse ":"
 * in node names, which causes GLTFExporter to silently drop all animations.
 */
export const MIXAMO_BONE_NAMES = [
  // Root & Spine
  'Hips',
  'Spine',
  'Spine1',
  'Spine2',
  // Head
  'Neck',
  'Head',
  'HeadTop_End',
  'LeftEye',
  'RightEye',
  // Left Arm
  'LeftShoulder',
  'LeftArm',
  'LeftForeArm',
  'LeftHand',
  // Left Fingers
  'LeftHandThumb1',
  'LeftHandThumb2',
  'LeftHandThumb3',
  'LeftHandThumb4',
  'LeftHandIndex1',
  'LeftHandIndex2',
  'LeftHandIndex3',
  'LeftHandIndex4',
  'LeftHandMiddle1',
  'LeftHandMiddle2',
  'LeftHandMiddle3',
  'LeftHandMiddle4',
  'LeftHandRing1',
  'LeftHandRing2',
  'LeftHandRing3',
  'LeftHandRing4',
  'LeftHandPinky1',
  'LeftHandPinky2',
  'LeftHandPinky3',
  'LeftHandPinky4',
  // Right Arm
  'RightShoulder',
  'RightArm',
  'RightForeArm',
  'RightHand',
  // Right Fingers
  'RightHandThumb1',
  'RightHandThumb2',
  'RightHandThumb3',
  'RightHandThumb4',
  'RightHandIndex1',
  'RightHandIndex2',
  'RightHandIndex3',
  'RightHandIndex4',
  'RightHandMiddle1',
  'RightHandMiddle2',
  'RightHandMiddle3',
  'RightHandMiddle4',
  'RightHandRing1',
  'RightHandRing2',
  'RightHandRing3',
  'RightHandRing4',
  'RightHandPinky1',
  'RightHandPinky2',
  'RightHandPinky3',
  'RightHandPinky4',
  // Left Leg
  'LeftUpLeg',
  'LeftLeg',
  'LeftFoot',
  'LeftToeBase',
  'LeftToe_End',
  // Right Leg
  'RightUpLeg',
  'RightLeg',
  'RightFoot',
  'RightToeBase',
  'RightToe_End',
] as const

/** Bones that define the core structure (non-finger, non-end bones) */
const CORE_BONES = [
  'Hips',
  'Spine',
  'Spine1',
  'Spine2',
  'Neck',
  'Head',
  'LeftShoulder',
  'LeftArm',
  'LeftForeArm',
  'LeftHand',
  'RightShoulder',
  'RightArm',
  'RightForeArm',
  'RightHand',
  'LeftUpLeg',
  'LeftLeg',
  'LeftFoot',
  'LeftToeBase',
  'RightUpLeg',
  'RightLeg',
  'RightFoot',
  'RightToeBase',
] as const

export interface MixamoDetectionResult {
  isMixamo: boolean
  matchRatio: number
  coreMatchRatio: number
  nameMap: Record<string, string>
  unmatchedBones: string[]
  unusedMixamoNames: string[]
}

/** Canonical key for a Mixamo bone name */
function mixamoCanonicalKey(name: string): string {
  return canonicalKey(name)
}

/** Normalize a name to lowercase alphanumeric for comparison */
function normalizeForCompare(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]/g, '')
    .replace(/^mixamorig[0-9_]*/, '')
}

/**
 * Build a lookup from canonical key → list of standard Mixamo bone names.
 * Multiple Mixamo bones can share the same canonical key (e.g. Spine, Spine1, Spine2
 * all canonicalize to "spine" after suffix stripping).
 */
function buildMixamoIndex(): Map<string, string[]> {
  const map = new Map<string, string[]>()

  function addEntry(key: string, name: string): void {
    if (!key) return
    const existing = map.get(key)
    if (existing) {
      if (!existing.includes(name)) existing.push(name)
    } else {
      map.set(key, [name])
    }
  }

  for (const name of MIXAMO_BONE_NAMES) {
    addEntry(mixamoCanonicalKey(name), name)

    // Also index with "mixamorig" prefix for bones named like "mixamorigHips"
    addEntry(canonicalKey('mixamorig' + name), name)
  }

  return map
}

const MIXAMO_INDEX = buildMixamoIndex()

const CORE_KEYS = new Set(CORE_BONES.map(mixamoCanonicalKey))

/**
 * From a list of candidate Mixamo names, pick the one whose normalized form
 * is closest to the input bone name. This resolves ambiguity when multiple
 * Mixamo bones share the same canonical key (e.g. Spine vs Spine1 vs Spine2).
 */
function pickBestCandidate(
  boneName: string,
  candidates: string[],
  usedNames: Set<string>
): string | null {
  const boneNorm = normalizeForCompare(boneName)
  let best: string | null = null
  let bestScore = -1

  for (const candidate of candidates) {
    if (usedNames.has(candidate)) continue

    const candidateNorm = normalizeForCompare(candidate)

    // Exact normalized match is best
    if (
      boneNorm === candidateNorm ||
      boneNorm === normalizeForCompare(candidate)
    ) {
      return candidate
    }

    // Score by longest common prefix length (after normalization)
    const minLen = Math.min(boneNorm.length, candidateNorm.length)
    let commonPrefix = 0
    for (let i = 0; i < minLen; i++) {
      if (boneNorm[i] === candidateNorm[i]) commonPrefix++
      else break
    }

    // Prefer shorter candidates when prefix matches equally (Spine over Spine2)
    const score = commonPrefix * 1000 - candidateNorm.length
    if (score > bestScore) {
      bestScore = score
      best = candidate
    }
  }

  return best
}

/**
 * Detect whether a set of bones matches the Mixamo skeleton structure.
 * Uses fuzzy name matching via canonicalKey from merge.ts.
 */
export function detectMixamoSkeleton(
  bones: Array<THREE.Bone | { name: string }>
): MixamoDetectionResult {
  const nameMap: Record<string, string> = {}
  const usedMixamoNames = new Set<string>()
  const unmatchedBones: string[] = []

  for (const bone of bones) {
    if (!bone.name) continue

    const key = canonicalKey(bone.name)
    if (!key) continue

    const candidates = MIXAMO_INDEX.get(key)
    if (candidates) {
      const best = pickBestCandidate(bone.name, candidates, usedMixamoNames)
      if (best) {
        nameMap[bone.name] = best
        usedMixamoNames.add(best)
        continue
      }
    }

    unmatchedBones.push(bone.name)
  }

  const matchedCoreCount = [...usedMixamoNames].filter((n) => {
    const key = mixamoCanonicalKey(n)
    return CORE_KEYS.has(key)
  }).length

  const coreMatchRatio = matchedCoreCount / CORE_BONES.length
  const matchRatio = usedMixamoNames.size / MIXAMO_BONE_NAMES.length

  const unusedMixamoNames = MIXAMO_BONE_NAMES.filter(
    (n) => !usedMixamoNames.has(n)
  )

  return {
    isMixamo: coreMatchRatio >= 0.7,
    matchRatio,
    coreMatchRatio,
    nameMap,
    unmatchedBones,
    unusedMixamoNames,
  }
}

/**
 * Rename all bones in the scene and update animation track names to match.
 * Returns the number of bones renamed.
 */
export function renameBonesToMixamo(
  scene: THREE.Object3D,
  animations: THREE.AnimationClip[],
  nameMap: Record<string, string>
): number {
  let renamed = 0

  // Rename all Object3D nodes in the scene
  scene.traverse((node) => {
    if (!node.name) return
    const newName = nameMap[node.name]
    if (newName && newName !== node.name) {
      node.name = newName
      renamed += 1
    }
  })

  // Update animation track names
  // Track names follow the pattern: "NodeName.property" (e.g. "Hips.position")
  for (const clip of animations) {
    for (const track of clip.tracks) {
      const dotIdx = track.name.indexOf('.')
      if (dotIdx < 0) continue

      const nodeName = track.name.slice(0, dotIdx)
      const rest = track.name.slice(dotIdx)
      const newNodeName = nameMap[nodeName]

      if (newNodeName && newNodeName !== nodeName) {
        track.name = newNodeName + rest
      }
    }
  }

  return renamed
}
