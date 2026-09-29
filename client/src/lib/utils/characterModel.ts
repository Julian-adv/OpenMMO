import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { loadGLB } from './gltfCache'
import { MODULAR_MALE_DIRECTORY, MODULAR_MALE_MODEL_PATH } from './modelPaths'
import type { CharacterClass } from '../network/networkTypes'
import {
  DEFAULT_MODULAR_OUTFIT,
  KNIGHT_MODULAR_OUTFIT,
  bindModularPart,
  modularAnimationClips,
  modularRigId,
  modularOutfitParts,
  modularSwordTracks,
  parseModularHandProfile,
  showModularOutfit,
  skinnedParts,
} from './modularCharacter'

export const MODULAR_SWORD_ATTACHMENT = 'modularSwordAttachment'
const maleModels = new Map<string, Promise<GLTF>>()
const animations = new Map<string, Promise<GLTF>>()

export function loadCharacterModel(
  path: string,
  characterClass?: CharacterClass
): Promise<GLTF> {
  if (path !== MODULAR_MALE_MODEL_PATH) return loadGLB(path)
  const outfit =
    characterClass === 'knight' ? KNIGHT_MODULAR_OUTFIT : DEFAULT_MODULAR_OUTFIT
  const key = outfit.top
  let pending = maleModels.get(key)
  if (!pending) {
    const ids = [...modularOutfitParts(outfit)]
    pending = Promise.all([
      loadGLB(path),
      ...ids.map((id) => loadGLB(`${MODULAR_MALE_DIRECTORY}/${id}.glb`)),
    ])
      .then(([base, ...sources]) => {
        const scene = clone(base.scene) as THREE.Group
        scene.userData.modular_character = true
        const body = skinnedParts(scene)
        const parts = new Map(
          ids.map((id, i) => [id, bindModularPart(scene, sources[i].scene)])
        )
        showModularOutfit(body, parts, outfit)
        for (const mesh of parts.get('hair_crop') ?? []) {
          const tint = (source: THREE.Material) => {
            const material = source.clone()
            if (material instanceof THREE.MeshStandardMaterial)
              material.color.set('#604332')
            return material
          }
          mesh.material = Array.isArray(mesh.material)
            ? mesh.material.map(tint)
            : tint(mesh.material)
        }
        for (const mesh of skinnedParts(scene)) mesh.frustumCulled = false
        const socket = new THREE.Group()
        socket.name = MODULAR_SWORD_ATTACHMENT
        const hand = scene.getObjectByName('RightHand')
        if (!hand) throw new Error('Modular character has no right hand')
        hand.add(socket)
        return { ...base, scene, scenes: [scene] }
      })
      .catch((error) => {
        maleModels.delete(key)
        throw error
      })
    maleModels.set(key, pending)
  }
  return pending
}

export function loadCharacterAnimationPack(
  modelPath: string,
  packPath: string
): Promise<GLTF> {
  if (modelPath !== MODULAR_MALE_MODEL_PATH) return loadGLB(packPath)
  const path = `${MODULAR_MALE_DIRECTORY}/animations/${packPath.split('/').pop()}`
  let pending = animations.get(path)
  if (!pending) {
    pending = Promise.all([loadGLB(MODULAR_MALE_MODEL_PATH), loadGLB(path)])
      .then(([body, pack]) => {
        const profile = parseModularHandProfile(
          pack.scene.userData.hand_profile
        )
        const rigId = modularRigId(body.scene)
        const clips = modularAnimationClips(body.scene, pack, 'corrected').map(
          (source) => {
            const clip = new THREE.AnimationClip(
              source.name,
              source.duration,
              [
                ...source.tracks,
                ...modularSwordTracks(
                  profile,
                  rigId,
                  source,
                  MODULAR_SWORD_ATTACHMENT
                ),
              ],
              source.blendMode
            )
            clip.userData = structuredClone(source.userData)
            return clip
          }
        )
        return { ...pack, scene: body.scene, animations: clips }
      })
      .catch((error) => {
        animations.delete(path)
        throw error
      })
    animations.set(path, pending)
  }
  return pending
}

export function modularSwordAttachment(
  root: THREE.Object3D,
  worldModel: string
): THREE.Object3D | undefined {
  if (worldModel !== 'weapons/sword.glb') return undefined
  return root.getObjectByName(MODULAR_SWORD_ATTACHMENT)
}

export function disposeCharacterSkeletons(root: THREE.Object3D): void {
  const skeletons = new Set(skinnedParts(root).map((mesh) => mesh.skeleton))
  for (const skeleton of skeletons) skeleton.dispose()
}
