import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { loadGLB } from './gltfCache'
import { MODULAR_MALE_DIRECTORY, MODULAR_MALE_MODEL_PATH } from './modelPaths'
import type { ArmorEquipment } from '../network/networkTypes'
import { modularOutfitForArmor } from './modularEquipment'
import { disposePeltPhysics } from '../effects/pelt-rig'
import {
  DEFAULT_MODULAR_OUTFIT,
  KNIGHT_MODULAR_OUTFIT,
  BARBARIAN_MODULAR_OUTFIT,
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
let maleModel: Promise<GLTF> | undefined
const animations = new Map<string, Promise<GLTF>>()
const outfitParts = new Set([
  ...modularOutfitParts(DEFAULT_MODULAR_OUTFIT),
  ...modularOutfitParts(KNIGHT_MODULAR_OUTFIT),
  ...modularOutfitParts(BARBARIAN_MODULAR_OUTFIT),
])

export function loadCharacterModel(path: string): Promise<GLTF> {
  if (path !== MODULAR_MALE_MODEL_PATH) return loadGLB(path)
  if (!maleModel) {
    const ids = [...outfitParts]
    maleModel = Promise.all([
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
        showModularOutfit(body, parts, DEFAULT_MODULAR_OUTFIT)
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
        maleModel = undefined
        throw error
      })
  }
  return maleModel
}

export function applyCharacterArmor(
  root: THREE.Object3D,
  armor?: ArmorEquipment
): void {
  const meshes = skinnedParts(root)
  const body: THREE.SkinnedMesh[] = []
  const parts = new Map<string, THREE.SkinnedMesh[]>()
  for (const mesh of meshes) {
    const id = mesh.userData.part_id
    if (outfitParts.has(id)) {
      const group = parts.get(id) ?? []
      group.push(mesh)
      parts.set(id, group)
    } else body.push(mesh)
  }
  if (parts.size) showModularOutfit(body, parts, modularOutfitForArmor(armor))
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
  disposePeltPhysics(root)
  const skeletons = new Set(skinnedParts(root).map((mesh) => mesh.skeleton))
  for (const skeleton of skeletons) skeleton.dispose()
}
