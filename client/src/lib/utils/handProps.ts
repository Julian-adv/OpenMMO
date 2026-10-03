import * as THREE from 'three'
import { isRangedWeapon } from '../data/itemDefs'
import { getWeaponAnimation } from '../data/weaponAnimationDefs'
import { MODULAR_MALE_MODEL_PATH } from './modelPaths'

/** Offset from the wrist bone toward the palm, so a prop looks gripped. */
const HAND_GRIP_OFFSET = new THREE.Vector3(0, 0.08, 0)
const MODULAR_TORCH_GRIP_OFFSET = new THREE.Vector3(0.025, 0.08, 0.012)

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

/** Which hand holds a main-hand item. A bow is drawn with the right hand, so
 *  the stave itself sits in the left — every other weapon leads with the right. */
export function mainHandBoneFor(
  itemDefId: string | null | undefined
): 'RightHand' | 'LeftHand' {
  return isRangedWeapon(itemDefId) ? 'LeftHand' : 'RightHand'
}

/** Pose a main-hand prop for the item it represents. `forearm` is this rig's
 *  forearm length (`forearmLength`); without it the bow falls back to the
 *  offset it was fitted at. */
export function poseMainHandProp(
  prop: THREE.Object3D,
  itemDefId: string,
  forearm?: number
) {
  prop.position.copy(HAND_GRIP_OFFSET)
  const rotation = getWeaponAnimation(itemDefId)?.gripRotationRadians
  if (rotation) {
    const [x, y, z] = rotation.split('|').map(Number)
    prop.rotation.set(x, y, z)
  } else if (itemDefId === 'fishing_rod') {
    prop.position.copy(FISHING_ROD_POSITION)
    prop.rotation.copy(FISHING_ROD_ROTATION)
  } else if (itemDefId === MANDOLIN_ITEM_DEF_ID) {
    prop.position.copy(MANDOLIN_POSITION)
    prop.rotation.copy(MANDOLIN_ROTATION)
  } else if (isRangedWeapon(itemDefId)) {
    prop.position.copy(BOW_POSITION)
    if (forearm && forearm > 0) prop.position.y = forearm * BOW_FOREARM_RATIO
    prop.rotation.copy(BOW_ROTATION)
  }
}

const forearmCache = new Map<string, number>()

/** Elbow-to-wrist in metres, from the bind pose, for the arm `handBone` is on.
 *  0 when either bone is missing. Cached per `key` (the model's URL). */
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
  prop.position.copy(
    modelPath === MODULAR_MALE_MODEL_PATH && worldModel === 'weapons/torch.glb'
      ? MODULAR_TORCH_GRIP_OFFSET
      : HAND_GRIP_OFFSET
  )
  prop.rotation.y = Math.PI
}

/** Where the flame sits on a torch GLB that names no `torch_tip` empty. */
export const FALLBACK_TORCH_TIP_LOCAL_OFFSET = new THREE.Vector3(0.6, 0, 0)

/** Named tip empty baked into a prop GLB, or a fallback child at the given
 *  local offset — either way it rides the bone chain. */
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
