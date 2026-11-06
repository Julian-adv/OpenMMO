import * as THREE from 'three'
import { getItemDef, isRangedWeapon } from '../data/itemDefs'
import { getWeaponAnimation } from '../data/weaponAnimationDefs'
import instrumentCarry from '../data/instrumentCarry.json'
import {
  MODULAR_MALE_MODEL_PATH,
  MODULAR_FEMALE_MODEL_PATH,
  FEMALE_PRIEST_CHARACTER_MODEL_PATH,
} from './modelPaths'

/** Offset from the wrist bone toward the palm, so a prop looks gripped. */
const HAND_GRIP_OFFSET = new THREE.Vector3(0, 0.08, 0)

type HandFit = { position: THREE.Vector3; rotation?: THREE.Euler }

// Keyed by world model, or by weaponType for a family built to one grip origin.
export const FEMALE_HAND_FITS: Record<string, HandFit> = {
  'weapons/sword.glb': {
    position: new THREE.Vector3(0.0652, 0.0422, -0.0282),
    rotation: new THREE.Euler(0, 0, 0.2),
  },
  'weapons/steel_longsword.glb': {
    position: new THREE.Vector3(0.0504, 0.1152, 0.0198),
    rotation: new THREE.Euler(0, 0, 0.2),
  },
  // Every dagger model puts its grip origin 6.3 cm behind the guard.
  dagger: {
    position: new THREE.Vector3(-0.012, 0.112, 0.02),
    rotation: new THREE.Euler(0, 0, 0.2),
  },
  'weapons/torch.glb': {
    position: new THREE.Vector3(-0.0072, 0.0882, 0.0162),
    rotation: new THREE.Euler(0, Math.PI, 0.2),
  },
  'armor/shield_wooden.glb': {
    position: new THREE.Vector3(-0.0072, 0.0558, -0.0144),
  },
}

// Per-rig grips for props that HAND_GRIP_OFFSET does not seat.
const MODEL_HAND_FITS: Record<string, Record<string, HandFit>> = {
  [MODULAR_FEMALE_MODEL_PATH]: FEMALE_HAND_FITS,
  [MODULAR_MALE_MODEL_PATH]: {
    'weapons/mace.glb': { position: new THREE.Vector3(0.02, 0.12, 0.05) },
    'weapons/torch.glb': { position: new THREE.Vector3(0.025, 0.08, 0.012) },
  },
  [FEMALE_PRIEST_CHARACTER_MODEL_PATH]: {
    'weapons/mace.glb': { position: new THREE.Vector3(0.03, 0.07, 0.03) },
  },
}

function fitHandProp(
  prop: THREE.Object3D,
  worldModel?: string,
  modelPath?: string,
  weaponType?: string
) {
  // Female variants (#fragment) share the female grips.
  const fits = modelPath ? MODEL_HAND_FITS[modelPath.split('#')[0]] : undefined
  const fit =
    (worldModel ? fits?.[worldModel] : undefined) ??
    (weaponType ? fits?.[weaponType] : undefined)
  if (!fit) return
  prop.position.copy(fit.position)
  if (fit.rotation) prop.rotation.copy(fit.rotation)
}

// Seat the model's off-origin handle between the thumb and index finger.
const FISHING_ROD_POSITION = new THREE.Vector3(0.045, 0.01, 0.06)
const FISHING_ROD_ROTATION = new THREE.Euler(0, -Math.PI / 6, -Math.PI / 3)

// Fitted in tools/rig-importer against bow_shoot.
const BOW_POSITION = new THREE.Vector3(0.01, 0.06, 0.04)
const BOW_ROTATION = new THREE.Euler((-13 * Math.PI) / 180, 0, 0)

// Scale grip reach by forearm length; bracers distort hand mesh bounds.
const KNIGHT_FOREARM_METERS = 0.267
const BOW_FOREARM_RATIO = BOW_POSITION.y / KNIGHT_FOREARM_METERS

export const MANDOLIN_ITEM_DEF_ID = 'mandolin'

// Fitted with tools/fit-hand-prop.mjs --tilt 15 --push 0.06 --lift 0.04.
const MANDOLIN_ROTATION = new THREE.Euler(-2.413, -0.409, -0.353)
const MANDOLIN_POSITION = new THREE.Vector3(-0.03, 0.103, 0.126)
export type InstrumentCarry = typeof instrumentCarry

/**
 * Pose a carried instrument: the neck `grip` m from the origin sits in the fist, the body hangs past the
 * fingers, tipped forward by `tilt`° and sideways by `splay`°, turned `twist`° about the neck, then moved
 * `front` m forward (hand +X) and `palm` m into the palm (hand +Z). Tuned in the female preview.
 */
export function poseInstrumentCarry(
  prop: THREE.Object3D,
  carry: InstrumentCarry = instrumentCarry
) {
  const degrees = Math.PI / 180
  const turn = (axis: THREE.Vector3, angle: number) =>
    new THREE.Quaternion().setFromAxisAngle(axis, angle)
  const x = new THREE.Vector3(1, 0, 0)
  const z = new THREE.Vector3(0, 0, 1)
  prop.quaternion
    .copy(turn(x, carry.splay * degrees))
    .multiply(turn(z, -Math.PI / 2 - carry.tilt * degrees))
    .multiply(turn(x, carry.twist * degrees))
  prop.position
    .copy(HAND_GRIP_OFFSET)
    .add(new THREE.Vector3(carry.front, 0, carry.palm))
    .sub(new THREE.Vector3(carry.grip, 0, 0).applyQuaternion(prop.quaternion))
}

/** Pose the mandolin for the `/play_music` strumming clip. */
export function poseMusicProp(prop: THREE.Object3D) {
  prop.position.copy(MANDOLIN_POSITION)
  prop.rotation.copy(MANDOLIN_ROTATION)
}

// Bows sit in the left hand; other weapons lead with the right.
export function mainHandBoneFor(
  itemDefId: string | null | undefined
): 'RightHand' | 'LeftHand' {
  return isRangedWeapon(itemDefId) ? 'LeftHand' : 'RightHand'
}

// Scale the bow's offset to this rig's forearm when available.
export function poseMainHandProp(
  prop: THREE.Object3D,
  itemDefId: string,
  forearm?: number,
  modelPath?: string
) {
  prop.position.copy(HAND_GRIP_OFFSET)
  const rotation = getWeaponAnimation(itemDefId)?.gripRotationRadians
  if (rotation) {
    const [x, y, z] = rotation.split('|').map(Number)
    prop.rotation.set(x, y, z)
  } else if (itemDefId === 'fishing_rod') {
    prop.position.copy(FISHING_ROD_POSITION)
    prop.rotation.copy(FISHING_ROD_ROTATION)
  } else if (getItemDef(itemDefId)?.category === 'instrument') {
    poseInstrumentCarry(prop)
  } else if (isRangedWeapon(itemDefId)) {
    prop.position.copy(BOW_POSITION)
    if (forearm && forearm > 0) prop.position.y = forearm * BOW_FOREARM_RATIO
    prop.rotation.copy(BOW_ROTATION)
  }
  const def = getItemDef(itemDefId)
  fitHandProp(prop, def?.worldModel, modelPath, def?.weaponType)
}

const forearmCache = new Map<string, number>()

// Cache the bind-pose forearm length in metres; missing bones return zero.
export function forearmLength(
  root: THREE.Object3D,
  handBone: 'RightHand' | 'LeftHand',
  key: string
): number {
  const cached = forearmCache.get(key)
  if (cached !== undefined) return cached

  const foreArmBone = handBone === 'LeftHand' ? 'LeftForeArm' : 'RightForeArm'
  let length = 0
  root.traverse((child) => {
    const mesh = child as THREE.SkinnedMesh
    if (length > 0 || !mesh.isSkinnedMesh) return
    const at = (name: string) => {
      const index = mesh.skeleton.bones.findIndex((b) => b.name === name)
      if (index < 0) return null
      // The bind matrix is the inverse of what the skeleton stores.
      return new THREE.Vector3().setFromMatrixPosition(
        mesh.skeleton.boneInverses[index].clone().invert()
      )
    }
    const hand = at(handBone)
    const foreArm = at(foreArmBone)
    if (hand && foreArm) length = hand.distanceTo(foreArm)
  })

  forearmCache.set(key, length)
  return length
}
/** Pose a left-hand prop — shields and torches face the other way. */
export function poseOffHandProp(
  prop: THREE.Object3D,
  worldModel?: string,
  modelPath?: string
) {
  prop.position.copy(HAND_GRIP_OFFSET)
  if (worldModel === 'armor/earth_shield.glb') {
    prop.rotation.set(0.170659, 0.002636, 1.657126)
  } else {
    prop.rotation.set(0, Math.PI, 0)
  }
  fitHandProp(prop, worldModel, modelPath)
}

/** Where the flame sits on a torch GLB that names no `torch_tip` empty. */
export const FALLBACK_TORCH_TIP_LOCAL_OFFSET = new THREE.Vector3(0.6, 0, 0)

// Use the named tip, or attach a fallback at its local offset.
export function resolveTipNode(
  prop: THREE.Object3D,
  name: string,
  fallbackOffset: THREE.Vector3
): THREE.Object3D {
  const found = prop.getObjectByName(name)
  if (found) return found
  const node = new THREE.Object3D()
  node.position.copy(fallbackOffset)
  prop.add(node)
  return node
}
