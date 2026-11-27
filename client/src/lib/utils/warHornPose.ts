// The war-horn blow as tools/bake-war-horn.mjs bakes it into the war_horn
// pack; the client plays the baked clip, not this pose.
import * as THREE from 'three'
import { findBoneByName } from './characterAnimationUtils'
import { disposeObjectTree } from './disposeObjectTree'
import { RiderMotion } from './riderMotion'
import { WAR_HORN_RAISE_SECONDS } from './warHorn'

const RAISE_SECONDS = WAR_HORN_RAISE_SECONDS
const HOLD_END = RAISE_SECONDS + 2.15
const END = HOLD_END + 0.48
const MOUTHPIECE = new THREE.Vector3(0.296, 0.076, 0.07)
const GRIPS = [
  new THREE.Vector3(-0.08, 0.064, 0.008),
  new THREE.Vector3(0.15, 0.085, 0.012),
]
const HORN_ORIENTATION = new THREE.Quaternion().setFromEuler(
  new THREE.Euler(-Math.PI / 2, Math.PI / 2, 0, 'YXZ')
)
const lipOffset = (female: boolean) =>
  female
    ? new THREE.Vector3(0, -0.022, 0.096)
    : new THREE.Vector3(0, 0.027, 0.122)

export class WarHornPose {
  private readonly pivot = new THREE.Group()
  private readonly horn: THREE.Object3D
  private readonly motion: RiderMotion
  private readonly lip: THREE.Vector3
  private readonly rest = new THREE.Vector3()
  private readonly targets: {
    position: THREE.Vector3
    gripOffset: THREE.Vector3
  }[]

  private constructor(
    private readonly root: THREE.Object3D,
    private readonly head: THREE.Bone,
    private readonly hand: THREE.Bone,
    source: THREE.Object3D,
    female: boolean
  ) {
    this.motion = new RiderMotion(root)
    this.lip = lipOffset(female)
    this.targets = ['LeftHand', 'RightHand'].map((name) => {
      const hand = findBoneByName(root, name)
      const thumb = findBoneByName(root, name + 'Thumb3')
      const index = findBoneByName(root, name + 'Index2')
      const position = new THREE.Vector3()
      const gripOffset = new THREE.Vector3(0, 0.085, 0)
      if (hand && thumb && index) {
        thumb
          .getWorldPosition(position)
          .lerp(index.getWorldPosition(new THREE.Vector3()), 0.5)
        gripOffset.lerp(hand.worldToLocal(position), 0.65)
      }
      return { position, gripOffset }
    })
    this.horn = source.clone()
    this.horn.position.copy(MOUTHPIECE).negate()
    this.pivot.add(this.horn)
    this.pivot.visible = false
    root.add(this.pivot)
  }

  static create(root: THREE.Object3D, source: THREE.Object3D, female: boolean) {
    const head = findBoneByName(root, 'Head')
    const hand = findBoneByName(root, 'RightHand')
    return head && hand
      ? new WarHornPose(root, head, hand, source, female)
      : null
  }

  restore() {
    this.motion.restore()
  }

  update(seconds: number) {
    if (seconds >= END) return false
    const smooth = THREE.MathUtils.smoothstep
    const lift =
      smooth(seconds, 0, RAISE_SECONDS) * (1 - smooth(seconds, HOLD_END, END))
    const grip =
      smooth(seconds, 0, 0.18) * (1 - smooth(seconds, END - 0.18, END))
    this.head.localToWorld(this.pivot.position.copy(this.lip))
    this.root.worldToLocal(this.pivot.position)
    this.pivot.rotation.set(0.85 - lift * 1.05, 0.24, 0, 'YXZ')
    this.pivot.quaternion.multiply(HORN_ORIENTATION)
    const gripOffset = this.targets[1].position.subVectors(GRIPS[1], MOUTHPIECE)
    gripOffset.applyQuaternion(this.pivot.quaternion)
    this.hand.localToWorld(this.rest.copy(this.targets[1].gripOffset))
    this.root.worldToLocal(this.rest).sub(gripOffset)
    this.pivot.position.lerp(this.rest, 1 - lift)
    this.pivot.visible = true
    for (let i = 0; i < GRIPS.length; i++) {
      this.horn.localToWorld(this.targets[i].position.copy(GRIPS[i]))
    }
    this.motion.applyHandTargets(this.targets, grip)
    return true
  }

  dispose() {
    this.restore()
    this.pivot.removeFromParent()
    disposeObjectTree(this.pivot)
  }
}

export { WAR_HORN_CLIP } from './warHorn'
export const WAR_HORN_SECONDS = END
export const WAR_HORN_HOLD_SECONDS = (RAISE_SECONDS + HOLD_END) / 2

/** World matrix of the horn at the lips with the horn fully raised. */
export function warHornHoldMatrix(root: THREE.Object3D, female: boolean) {
  const head = findBoneByName(root, 'Head')!
  const pivot = head.localToWorld(lipOffset(female))
  const rotation = new THREE.Quaternion()
    .setFromEuler(new THREE.Euler(0.85 - 1.05, 0.24, 0, 'YXZ'))
    .multiply(HORN_ORIENTATION)
  root.updateMatrixWorld(true)
  const facing = root.getWorldQuaternion(new THREE.Quaternion())
  return new THREE.Matrix4()
    .compose(pivot, facing.multiply(rotation), new THREE.Vector3(1, 1, 1))
    .multiply(new THREE.Matrix4().makeTranslation(MOUTHPIECE.clone().negate()))
}
