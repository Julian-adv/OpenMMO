import * as THREE from 'three'
import { getItemDef } from '../data/itemDefs'
import {
  disposePeltPhysics,
  resetPeltPhysics,
  updatePeltPhysics,
} from '../effects/pelt-rig'
import type { ArmorEquipment } from '../network/networkTypes'
import {
  createCharacterModelRoot,
  getGltfAnimations,
} from './characterAnimationUtils'
import {
  applyCharacterArmor,
  disposeCharacterSkeletons,
  loadCharacterAnimationPack,
  loadCharacterModel,
} from './characterModel'
import {
  CHARACTER_ANIMATION_PACK_PATHS,
  MODULAR_FEMALE_MODEL_PATH,
} from './modelPaths'

const STAND_HEIGHT = 0.06
const POSE_CLIP = 'idle1'
const ARMOR_SLOTS = new Set(['head', 'chest', 'pants', 'boots', 'hands'])

interface Mannequin {
  root: THREE.Object3D | null
  mixer: THREE.AnimationMixer | null
  disposed: boolean
}

const live = new Set<THREE.Object3D>()

export function buildMannequinStand(): THREE.Group {
  const stand = new THREE.Mesh(
    new THREE.CylinderGeometry(0.4, 0.42, STAND_HEIGHT, 24),
    new THREE.MeshStandardMaterial({ color: 0x3a2416, roughness: 0.7 })
  )
  stand.position.y = STAND_HEIGHT / 2
  stand.castShadow = true
  stand.receiveShadow = true
  const group = new THREE.Group()
  group.add(stand)
  return group
}

function armorOf(items: string[]): ArmorEquipment {
  const armor: ArmorEquipment = {}
  for (const id of items) {
    const slot = getItemDef(id)?.equipSlot
    if (slot && ARMOR_SLOTS.has(slot)) armor[slot as keyof ArmorEquipment] = id
    else console.warn(`Mannequin cannot wear ${id}`)
  }
  return armor
}

/** Stands a dressed character on the stand, frozen at the first frame of its idle. */
export function mountMannequin(holder: THREE.Object3D, items: string[] = []) {
  const state: Mannequin = { root: null, mixer: null, disposed: false }
  holder.userData.mannequin = state
  Promise.all([
    loadCharacterModel(MODULAR_FEMALE_MODEL_PATH, undefined, 'game'),
    loadCharacterAnimationPack(
      MODULAR_FEMALE_MODEL_PATH,
      CHARACTER_ANIMATION_PACK_PATHS.locomotion
    ),
  ])
    .then(async ([model, pack]) => {
      if (state.disposed) return
      const { modelRoot: root } = createCharacterModelRoot(model.scene)
      await applyCharacterArmor(root, armorOf(items))
      if (state.disposed) {
        disposeCharacterSkeletons(root)
        return
      }
      const clip = getGltfAnimations(pack).find((c) => c.name === POSE_CLIP)
      if (clip) {
        state.mixer = new THREE.AnimationMixer(root)
        state.mixer.clipAction(clip).play()
        state.mixer.update(0)
      }
      root.position.y = STAND_HEIGHT
      holder.add(root)
      state.root = root
      resetPeltPhysics(root)
      live.add(root)
    })
    .catch((error) => console.error('Failed to load mannequin', error))
}

/** Skirt and pelt rigs work in world space, so they follow a moved mannequin. */
export function updateMannequins(dt: number) {
  for (const root of live) updatePeltPhysics(root, dt)
}

export function disposeMannequin(holder: THREE.Object3D) {
  const state = holder.userData.mannequin as Mannequin | undefined
  if (!state || state.disposed) return
  state.disposed = true
  if (!state.root) return
  live.delete(state.root)
  // The clone shares its geometry and materials with every other character.
  state.root.removeFromParent()
  state.mixer?.stopAllAction()
  state.mixer?.uncacheRoot(state.root)
  disposePeltPhysics(state.root)
  disposeCharacterSkeletons(state.root)
}
