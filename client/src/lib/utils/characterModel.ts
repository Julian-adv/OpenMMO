import { trimModularClothing } from './modularClothing'
import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { loadGLB } from './gltfCache'
import {
  MODULAR_MALE_DIRECTORY,
  MODULAR_MALE_MODEL_PATH,
  MODULAR_FEMALE_DIRECTORY,
  isModularFemaleModel,
} from './modelPaths'
import {
  femaleFace,
  loadFemaleCharacter,
  type CharacterDetail,
  loadFemaleAnimationPack,
  applyFemaleArmorAsync,
  disposeFemaleCharacter,
  femaleTabardMeshes,
  isFemaleHair,
} from './femaleCharacter'
import type {
  ArmorEquipment,
  CharacterAppearance,
} from '../network/networkTypes'
import { modularOutfitForArmor } from './modularEquipment'
import { disposePeltPhysics, preparePeltPhysics } from '../effects/pelt-rig'
import { disposeValkyrieWings } from '../effects/valkyrie-wings'
import { separateWavyHairMaterials } from './wavyHairMaterials'
import { femaleOutfitMask } from './female-outfit-mask'
import modularSoleOffsets from './modularSoleOffsets.json'
import { skinTabard, TABARD_PART, type TabardSkin } from './tabard'
import {
  applyAppearanceColors,
  disposeAppearanceColors,
} from '../shaders/character-appearance-colors'
import {
  DEFAULT_MODULAR_OUTFIT,
  KNIGHT_MODULAR_OUTFIT,
  BARBARIAN_MODULAR_OUTFIT,
  PROXIED_OUTFITS,
  SELECTABLE_HAIR_PARTS,
  type ModularOutfit,
  bindModularFace,
  bindModularPart,
  region,
  modularAnimationClips,
  modularRigId,
  modularOutfitParts,
  modularSwordTracks,
  parseModularHandProfile,
  showModularOutfit,
  prepareCharacterPart,
  skinnedParts,
} from './modularCharacter'

export const MODULAR_SWORD_ATTACHMENT = 'modularSwordAttachment'
const maleModels = new Map<string, Promise<GLTF>>()
const animations = new Map<string, Promise<GLTF>>()
// Load other parts when equipped.
const eagerParts = modularOutfitParts(DEFAULT_MODULAR_OUTFIT)
const lazyParts = new Set(
  [KNIGHT_MODULAR_OUTFIT, BARBARIAN_MODULAR_OUTFIT, ...PROXIED_OUTFITS]
    .flatMap((outfit) => [...modularOutfitParts(outfit)])
    .concat(
      ...SELECTABLE_HAIR_PARTS,
      'helmet_brilliance',
      'helmet_blindfold',
      TABARD_PART
    )
    .filter((id) => !eagerParts.has(id))
)
const outfitParts = new Set([...eagerParts, ...lazyParts])
const lazyStates = new WeakMap<
  THREE.Object3D,
  {
    generation: number
    loads: Map<string, Promise<void>>
    outfitKey?: string
    mask?: ReturnType<typeof femaleOutfitMask>
    maskedParts: string[]
    tabard?: { skin: TabardSkin; undyed: string } | null
  }
>()
const soleOffsets: Record<ModularOutfit['boots'], number> = modularSoleOffsets

export function loadCharacterModel(
  path: string,
  appearance?: Pick<CharacterAppearance, 'face'>,
  detail: CharacterDetail = 'closeup'
): Promise<GLTF> {
  if (isModularFemaleModel(path))
    return loadFemaleCharacter(path, detail, femaleFace(appearance?.face))
  if (path !== MODULAR_MALE_MODEL_PATH) return loadGLB(path)
  const face =
    appearance?.face === 'custom' || appearance?.face === 'elegant'
      ? 'default'
      : (appearance?.face ?? 'default')
  let maleModel = maleModels.get(face)
  if (!maleModel) {
    const ids = [...eagerParts]
    maleModel = Promise.all([
      loadGLB(path),
      loadGLB(`${MODULAR_MALE_DIRECTORY}/face_${face}.glb`),
      ...ids.map((id) => loadGLB(`${MODULAR_MALE_DIRECTORY}/${id}.glb`)),
    ])
      .then(([base, faceSource, ...sources]) => {
        const scene = clone(base.scene) as THREE.Group
        scene.userData.modular_character = true
        scene.userData.modular_face = face
        bindModularFace(scene, faceSource.scene)
        const body = skinnedParts(scene)
        const parts = new Map(
          ids.map((id, i) => [id, bindModularPart(scene, sources[i].scene)])
        )
        showModularOutfit(body, parts, DEFAULT_MODULAR_OUTFIT)
        for (const mesh of skinnedParts(scene)) prepareCharacterPart(mesh)
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
          mesh.visible = false
          prepareCharacterPart(mesh)
          if (id === 'hair_wavy_bone') separateWavyHairMaterials(mesh)
        }
        registerBodyMask(scene, state, id, gltf.scene.userData.body_mask)
        state.outfitKey = undefined
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

// Masks are fitted to the default face; other faces have their own topology.
function registerBodyMask(
  scene: THREE.Object3D,
  state: NonNullable<ReturnType<typeof lazyStates.get>>,
  id: string,
  mask: unknown
) {
  if (!mask || scene.userData.modular_face !== 'default') return
  const head = outfitMeshes(scene).body.find((mesh) => region(mesh) === 'head')
  if (!head) return
  state.mask ??= femaleOutfitMask()
  state.mask.register(id, mask, head)
  state.maskedParts.push(id)
}

/** Wear a tabard in `skin`, or take it off with null. Only the modular male
 *  has one. */
export async function applyCharacterTabard(
  root: THREE.Object3D,
  skin: TabardSkin | null,
  undyed = '#ffffff'
): Promise<void> {
  const scene = modularScene(root)
  if (!scene) return
  if (scene.userData.female_parts) {
    femaleTabards.set(scene, skin && { skin, undyed })
    await showFemaleTabard(root)
    return
  }
  let state = lazyStates.get(scene)
  if (!state)
    lazyStates.set(
      scene,
      (state = { generation: 0, loads: new Map(), maskedParts: [] })
    )
  state.tabard = skin && { skin, undyed }
  if (skin) {
    try {
      await bindLazyPart(scene, state, TABARD_PART)
    } catch (error) {
      console.error('Failed to load the tabard', error)
      return
    }
  }
  if (lazyStates.get(scene) === state) showTabard(scene, state)
}

const femaleTabards = new WeakMap<
  THREE.Object3D,
  { skin: TabardSkin; undyed: string } | null
>()

async function showFemaleTabard(root: THREE.Object3D) {
  const scene = modularScene(root)
  const wanted = scene && femaleTabards.get(scene)
  if (!scene || wanted === undefined) return
  let meshes: THREE.SkinnedMesh[] | null
  try {
    meshes = await femaleTabardMeshes(root)
  } catch (error) {
    console.error('Failed to load the tabard', error)
    return
  }
  const latest = femaleTabards.get(scene)
  for (const mesh of meshes ?? []) mesh.visible = !!latest
  if (latest && meshes) skinTabard(meshes, latest.skin, latest.undyed)
}

function showTabard(
  scene: THREE.Object3D,
  state: NonNullable<ReturnType<typeof lazyStates.get>>
) {
  const { parts } = outfitMeshes(scene)
  const meshes = parts.get(TABARD_PART) ?? []
  for (const mesh of meshes) mesh.visible = !!state.tabard
  for (const mesh of parts.get('top_guard') ?? [])
    trimModularClothing(mesh, !!state.tabard && 'guard_tabard')
  if (state.tabard) skinTabard(meshes, state.tabard.skin, state.tabard.undyed)
}

/** Keep the current outfit until the latest selection loads. */
export async function applyCharacterArmor(
  root: THREE.Object3D,
  armor?: ArmorEquipment,
  appearance?: CharacterAppearance
): Promise<void> {
  await applyArmor(root, armor, appearance)
  preparePeltPhysics(root)
}

async function applyArmor(
  root: THREE.Object3D,
  armor?: ArmorEquipment,
  appearance?: CharacterAppearance
): Promise<void> {
  const hair = appearance?.hair
  const femaleHair = isFemaleHair(hair) ? hair : undefined
  if (await applyFemaleArmorAsync(root, armor, femaleHair)) {
    if (femaleHair) applyAppearanceColors(root, skinnedParts(root), appearance)
    await showFemaleTabard(root)
    return
  }
  const scene = modularScene(root)
  if (!scene) return
  const outfit = modularOutfitForArmor(armor)
  if (appearance && !isFemaleHair(appearance.hair)) {
    outfit.hair =
      appearance.hair === 'none' ? 'none' : `hair_${appearance.hair}`
  }
  let state = lazyStates.get(scene)
  if (!state)
    lazyStates.set(
      scene,
      (state = { generation: 0, loads: new Map(), maskedParts: [] })
    )
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
  const outfitKey = JSON.stringify(outfit)
  if (state.outfitKey !== outfitKey) {
    const selected = showModularOutfit(body, parts, outfit)
    state.mask?.setEquippedAll(
      Object.fromEntries(state.maskedParts.map((id) => [id, selected.has(id)]))
    )
    state.outfitKey = outfitKey
  }
  showTabard(scene, state)
  applyAppearanceColors(root, meshes, appearance)
  scene.position.y = soleOffsets[outfit.boots]
}

export function loadCharacterAnimationPack(
  modelPath: string,
  packPath: string
): Promise<GLTF> {
  const packFile = packPath.split('/').pop()
  if (isModularFemaleModel(modelPath))
    return loadFemaleAnimationPack(
      `${MODULAR_FEMALE_DIRECTORY}/animations/${packFile}`
    )
  if (modelPath !== MODULAR_MALE_MODEL_PATH) return loadGLB(packPath)
  const path = `${MODULAR_MALE_DIRECTORY}/animations/${packFile}`
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
  disposeValkyrieWings(root)
  disposeFemaleCharacter(root)
  const scene = modularScene(root)
  if (scene) {
    lazyStates.get(scene)?.mask?.dispose()
    lazyStates.delete(scene)
  }
  disposeAppearanceColors(root)
  disposePeltPhysics(root)
  const skeletons = new Set(skinnedParts(root).map((mesh) => mesh.skeleton))
  for (const skeleton of skeletons) skeleton.dispose()
}
