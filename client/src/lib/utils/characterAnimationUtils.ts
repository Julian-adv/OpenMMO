import * as THREE from 'three'
import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
import { ANIMATION_ORDER, AnimationName } from '../types/animations'

export const LOCOMOTION_WAIT_TIMEOUT_MS = 2000

type AnimationSource = 'base' | 'locomotion'

export interface OrderedAnimationSelection {
  name: AnimationName
  clip: THREE.AnimationClip
  source: AnimationSource
  fromFallback: boolean
}

const LOCOMOTION_ANIMATION_NAMES = new Set<AnimationName>([
  AnimationName.IDLE1,
  AnimationName.IDLE2,
  AnimationName.IDLE3,
  AnimationName.IDLE4,
  AnimationName.WALK,
  AnimationName.JOG,
  AnimationName.RUN,
])

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

export function selectOrderedCharacterAnimations(
  baseAnimations: THREE.AnimationClip[],
  locomotionAnimations: THREE.AnimationClip[]
): OrderedAnimationSelection[] {
  const defaultBaseClip = baseAnimations[0]
  const defaultLocomotionClip = locomotionAnimations[0]
  const defaultClip = defaultBaseClip ?? defaultLocomotionClip
  if (!defaultClip) return []

  const defaultSource: AnimationSource = defaultBaseClip ? 'base' : 'locomotion'

  const baseClipByName = new Map(
    baseAnimations.map((clip) => [clip.name, clip])
  )
  const locomotionClipByName = new Map(
    locomotionAnimations.map((clip) => [clip.name, clip])
  )

  return ANIMATION_ORDER.map((name) => {
    const baseClip = baseClipByName.get(name)
    const locomotionClip = locomotionClipByName.get(name)
    const preferredClip = LOCOMOTION_ANIMATION_NAMES.has(name)
      ? (locomotionClip ?? baseClip)
      : (baseClip ?? locomotionClip)

    if (preferredClip) {
      return {
        name,
        clip: preferredClip,
        source: preferredClip === locomotionClip ? 'locomotion' : 'base',
        fromFallback: false,
      }
    }

    return {
      name,
      clip: defaultClip,
      source: defaultSource,
      fromFallback: true,
    }
  })
}
