import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { loadGLB } from './gltfCache'
import { MODULAR_MALE_DIRECTORY, MODULAR_MALE_MODEL_PATH } from './modelPaths'
import type {
  ArmorEquipment,
  CharacterAppearance,
} from '../network/networkTypes'
import { modularOutfitForArmor } from './modularEquipment'
import { disposePeltPhysics } from '../effects/pelt-rig'
import { separateWavyHairMaterials } from './wavyHairMaterials'
import modularSoleOffsets from './modularSoleOffsets.json'
import {
  applyAppearanceColors,
  disposeAppearanceColors,
} from '../shaders/character-appearance-colors'
import {
  DEFAULT_MODULAR_OUTFIT,
  KNIGHT_MODULAR_OUTFIT,
  BARBARIAN_MODULAR_OUTFIT,
  ROGUE_MODULAR_OUTFIT,
  CAVEMAN_MODULAR_OUTFIT,
  RANGER_MODULAR_OUTFIT,
  SELECTABLE_HAIR_PARTS,
  type ModularOutfit,
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
// Load other parts when equipped.
const eagerParts = modularOutfitParts(DEFAULT_MODULAR_OUTFIT)
const lazyParts = new Set(
  [
    KNIGHT_MODULAR_OUTFIT,
    BARBARIAN_MODULAR_OUTFIT,
    ROGUE_MODULAR_OUTFIT,
    CAVEMAN_MODULAR_OUTFIT,
    RANGER_MODULAR_OUTFIT,
  ]
    .flatMap((outfit) => [...modularOutfitParts(outfit)])
    .concat(...SELECTABLE_HAIR_PARTS)
    .filter((id) => !eagerParts.has(id))
)
const outfitParts = new Set([...eagerParts, ...lazyParts])
const lazyStates = new WeakMap<
  THREE.Object3D,
  { generation: number; loads: Map<string, Promise<void>> }
>()
const soleOffsets: Record<ModularOutfit['boots'], number> = modularSoleOffsets

export function loadCharacterModel(
  path: string,
  appearance?: Pick<CharacterAppearance, 'face'>
): Promise<GLTF> {
  if (path !== MODULAR_MALE_MODEL_PATH) return loadGLB(path)
  const face = appearance?.face ?? 'default'
  let maleModel = maleModels.get(face)
  if (!maleModel) {
    const ids = [...eagerParts]
    maleModel = Promise.all([
      loadGLB(
        face === 'default' ? path : `${MODULAR_MALE_DIRECTORY}/base_${face}.glb`
      ),
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
        for (const mesh of skinnedParts(scene)) mesh.frustumCulled = false
        const socket = new THREE.Group()
        socket.name = MODULAR_SWORD_ATTACHMENT
        const hand = scene.getObjectByName('RightHand')
        if (!hand) throw new Error('Modular character has no right hand')
        hand.add(socket)
        return { ...base, scene, scenes: [scene] }
      })
      .catch((error) => {
        maleModels.delete(face)
        throw error
      })
    maleModels.set(face, maleModel)
  }
  return maleModel
}

function modularScene(root: THREE.Object3D): THREE.Object3D | undefined {
  return root.userData.modular_character
    ? root
    : root.children.find((child) => child.userData.modular_character)
}

function outfitMeshes(scene: THREE.Object3D) {
  const meshes = skinnedParts(scene)
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
  return { meshes, body, parts }
}

function bindLazyPart(
  scene: THREE.Object3D,
  state: NonNullable<ReturnType<typeof lazyStates.get>>,
  id: string
): Promise<void> {
  let load = state.loads.get(id)
  if (!load) {
    load = loadGLB(`${MODULAR_MALE_DIRECTORY}/${id}.glb`).then(
      (gltf) => {
        if (lazyStates.get(scene) !== state) return
        for (const mesh of bindModularPart(scene, gltf.scene)) {
          mesh.frustumCulled = false
          if (id === 'hair_wavy_bone') separateWavyHairMaterials(mesh)
        }
      },
      (error) => {
        state.loads.delete(id)
        throw error
      }
    )
    state.loads.set(id, load)
  }
  return load
}

/** Keep the current outfit until the latest selection loads. */
export async function applyCharacterArmor(
  root: THREE.Object3D,
  armor?: ArmorEquipment,
  appearance?: CharacterAppearance
): Promise<void> {
  const scene = modularScene(root)
  if (!scene) return
  const outfit = modularOutfitForArmor(armor)
  if (appearance) {
    outfit.hair =
      appearance.hair === 'none' ? 'none' : `hair_${appearance.hair}`
  }
  let state = lazyStates.get(scene)
  if (!state)
    lazyStates.set(scene, (state = { generation: 0, loads: new Map() }))
  const generation = ++state.generation
  let { meshes, body, parts } = outfitMeshes(scene)
  const missing = [...modularOutfitParts(outfit)].filter((id) => !parts.has(id))
  if (missing.length) {
    try {
      await Promise.all(missing.map((id) => bindLazyPart(scene, state, id)))
    } catch (error) {
      console.error('Failed to load modular outfit parts', error)
      return
    }
    if (state.generation !== generation || lazyStates.get(scene) !== state)
      return
    ;({ meshes, body, parts } = outfitMeshes(scene))
  }
  showModularOutfit(body, parts, outfit)
  applyAppearanceColors(root, meshes, appearance)
  scene.position.y = soleOffsets[outfit.boots]
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
  const scene = modularScene(root)
  if (scene) lazyStates.delete(scene)
  disposeAppearanceColors(root)
  disposePeltPhysics(root)
  const skeletons = new Set(skinnedParts(root).map((mesh) => mesh.skeleton))
  for (const skeleton of skeletons) skeleton.dispose()
}
