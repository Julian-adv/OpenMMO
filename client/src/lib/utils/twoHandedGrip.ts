import * as THREE from 'three'
import { findBoneByName } from './characterAnimationUtils'

export class TwoHandedGrip {
  private readonly rotation: THREE.Quaternion
  private readonly axis: THREE.Vector3
  private readonly point = new THREE.Vector3()
  private readonly direction = new THREE.Vector3()
  private readonly correction = new THREE.Quaternion()

  constructor(
    private readonly prop: THREE.Object3D,
    private readonly offHand: THREE.Object3D,
    private readonly reach: number,
    private readonly fingers: THREE.Object3D[] = []
  ) {
    this.rotation = prop.quaternion.clone()
    this.axis = new THREE.Vector3(-1, 0, 0).applyQuaternion(this.rotation)
  }

  update(active: boolean) {
    this.prop.quaternion.copy(this.rotation)
    const hand = this.prop.parent
    if (!active || !hand) return

    if (this.fingers.length > 0) {
      this.direction.set(0, 0, 0)
      for (const finger of this.fingers) {
        this.direction.add(finger.getWorldPosition(this.point))
      }
      this.direction.divideScalar(this.fingers.length)
    } else {
      this.offHand.localToWorld(this.direction.set(0, 0.08, 0))
    }
    hand.worldToLocal(this.direction).sub(this.prop.position)
    const distance = this.direction.length()
    if (distance < 1e-4) return

    const weight =
      1 - THREE.MathUtils.smoothstep(distance, this.reach, this.reach * 1.5)
    if (weight === 0) return

    this.correction.setFromUnitVectors(this.axis, this.direction.normalize())
    this.correction.multiply(this.rotation)
    this.prop.quaternion.slerp(this.correction, weight)
  }
}

export function createTwoHandedGrip(
  root: THREE.Object3D,
  prop: THREE.Object3D,
  reach: number
): TwoHandedGrip | null {
  const hand = findBoneByName(root, 'LeftHand')
  if (!hand) return null
  const index = findBoneByName(root, 'LeftHandIndex1')
  const thumb = findBoneByName(root, 'LeftHandThumb3')
  return new TwoHandedGrip(
    prop,
    hand,
    reach,
    index && thumb ? [index, thumb] : []
  )
}
