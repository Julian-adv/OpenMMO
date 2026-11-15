import * as THREE from 'three'
import { getItemDef } from '../data/itemDefs'
import { updatePeltPhysics } from '../effects/pelt-rig'
import {
  ARMOR_SLOTS,
  type ArmorEquipment,
  type Gender,
} from '../network/networkTypes'
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
  MODULAR_MALE_MODEL_PATH,
} from './modelPaths'

const STAND_HEIGHT = 0.06
const POSE_CLIP = 'idle1'
const SETTLE_SECONDS = 1.5

interface Cloth {
  holder: THREE.Object3D
  at: THREE.Vector3
  yaw: number
  settle: number
}

// Mannequins whose skirt or pelt still has to hang, keyed by character root.
const cloths = new Map<THREE.Object3D, Cloth>()

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
    const slot = ARMOR_SLOTS.find((s) => s === getItemDef(id)?.equipSlot)
    if (slot) armor[slot] = id
    else console.warn(`Mannequin cannot wear ${id}`)
  }
  return armor
}

/** Stands a dressed character on the stand, frozen at the first frame of its idle. */
export async function mountMannequin(
  holder: THREE.Object3D,
  items: string[] = [],
  body: Gender = 'female'
) {
  let disposed = false
  let root: THREE.Object3D | null = null
  let mixer: THREE.AnimationMixer | null = null
  holder.addEventListener('dispose', () => {
    disposed = true
    if (!root) return
    cloths.delete(root)
    mixer?.stopAllAction()
    mixer?.uncacheRoot(root)
    disposeCharacterSkeletons(root)
  })
  const path =
    body === 'male' ? MODULAR_MALE_MODEL_PATH : MODULAR_FEMALE_MODEL_PATH
  try {
    const [model, pack] = await Promise.all([
      loadCharacterModel(path, undefined, 'game'),
      loadCharacterAnimationPack(
        path,
        CHARACTER_ANIMATION_PACK_PATHS.locomotion
      ),
    ])
    if (disposed) return
    const dressed = createCharacterModelRoot(model.scene).modelRoot
    await applyCharacterArmor(dressed, armorOf(items))
    if (disposed) {
      disposeCharacterSkeletons(dressed)
      return
    }
    const clip = getGltfAnimations(pack).find((c) => c.name === POSE_CLIP)
    if (clip) {
      mixer = new THREE.AnimationMixer(dressed)
      mixer.clipAction(clip).play()
      mixer.update(0)
    }
    dressed.position.y = STAND_HEIGHT
    holder.add(dressed)
    root = dressed
    let rigged = false
    dressed.traverse((node) => {
      rigged ||= !!(node.userData.pelt_physics || node.userData.robe_physics)
    })
    if (rigged)
      cloths.set(dressed, {
        holder,
        at: new THREE.Vector3(NaN),
        yaw: 0,
        settle: 0,
      })
  } catch (error) {
    console.error('Failed to load mannequin', error)
  }
}

/** Lets cloth hang for a moment after a mannequin appears or is moved; the pose itself never changes. */
export function updateMannequins(dt: number) {
  for (const [root, cloth] of cloths) {
    const { holder } = cloth
    if (!holder.parent) {
      cloth.at.setX(NaN)
      continue
    }
    if (!cloth.at.equals(holder.position) || cloth.yaw !== holder.rotation.y) {
      cloth.at.copy(holder.position)
      cloth.yaw = holder.rotation.y
      cloth.settle = SETTLE_SECONDS
    }
    if (cloth.settle <= 0) continue
    cloth.settle -= dt
    updatePeltPhysics(root, dt)
  }
}
