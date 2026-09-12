import * as THREE from 'three'
import { findBoneByName } from './characterAnimationUtils'
import { getWeaponEffectAxis } from './weaponEffectAxis'

export class EnchantWeaponPose {
  weight = 0
  private applied = false
  private readonly bones: {
    node: THREE.Object3D
    position: THREE.Vector3
    rotation: THREE.Quaternion
    targetPosition: THREE.Vector3
    targetRotation: THREE.Quaternion
  }[] = []

  constructor(
    root: THREE.Object3D,
    readonly weapon: THREE.Object3D,
    handName: 'RightHand' | 'LeftHand',
    twoHanded: boolean
  ) {
    root.traverse((node) => {
      if (node instanceof THREE.Bone || node === weapon) {
        this.bones.push({
          node,
          position: node.position.clone(),
          rotation: node.quaternion.clone(),
          targetPosition: node.position.clone(),
          targetRotation: node.quaternion.clone(),
        })
      }
    })
    const side = handName === 'RightHand' ? 'Right' : 'Left'
    const other = side === 'Right' ? 'Left' : 'Right'
    const hand = findBoneByName(root, handName)
    const head = findBoneByName(root, 'Head')
    if (hand && head) {
      const axis = getWeaponEffectAxis(weapon)
      root.updateWorldMatrix(true, true)
      const forward = new THREE.Vector3(0, 0, 1).transformDirection(
        root.matrixWorld
      )
      const right = new THREE.Vector3(1, 0, 0).transformDirection(
        root.matrixWorld
      )
      const target = head
        .getWorldPosition(new THREE.Vector3())
        .addScaledVector(forward, 0.28)
        .addScaledVector(right, side === 'Right' ? 0.12 : -0.12)
      target.y += 0.04
      const originalWeaponRotation = weapon.getWorldQuaternion(
        new THREE.Quaternion()
      )
      const offHand = findBoneByName(root, `${other}Hand`)
      const offHandRotation = offHand?.getWorldQuaternion(
        new THREE.Quaternion()
      )
      this.solveArm(
        root,
        side,
        target,
        right.clone().multiplyScalar(side === 'Right' ? 1 : -1)
      )
      const blade = axis.direction
        .clone()
        .transformDirection(weapon.matrixWorld)
      const turn = new THREE.Quaternion().setFromUnitVectors(
        blade,
        new THREE.Vector3(0, 1, 0)
      )
      this.orient(
        hand,
        hand.getWorldQuaternion(new THREE.Quaternion()).premultiply(turn)
      )
      if (twoHanded && offHand && offHandRotation) {
        const delta = weapon
          .getWorldQuaternion(new THREE.Quaternion())
          .multiply(originalWeaponRotation.invert())
        weapon.localToWorld(
          target
            .copy(axis.direction)
            .multiplyScalar(-Math.min(0.2, axis.length * 0.12))
        )
        this.solveArm(
          root,
          other,
          target,
          right.clone().multiplyScalar(side === 'Right' ? -1 : 1)
        )
        this.orient(offHand, offHandRotation.premultiply(delta))
      }
    }
    for (const saved of this.bones) {
      saved.targetPosition.copy(saved.node.position)
      saved.targetRotation.copy(saved.node.quaternion)
      saved.node.position.copy(saved.position)
      saved.node.quaternion.copy(saved.rotation)
    }
    root.updateWorldMatrix(true, true)
  }

  restore() {
    if (!this.applied) return
    for (const saved of this.bones) {
      saved.node.position.copy(saved.position)
      saved.node.quaternion.copy(saved.rotation)
    }
    this.applied = false
  }

  apply(deltaTime: number, active: boolean) {
    this.weight = THREE.MathUtils.clamp(
      this.weight + deltaTime * (active ? 3 : -4),
      0,
      1
    )
    if (this.weight === 0) return
    const weight = THREE.MathUtils.smoothstep(this.weight, 0, 1)
    for (const saved of this.bones) {
      saved.position.copy(saved.node.position)
      saved.rotation.copy(saved.node.quaternion)
      saved.node.position.lerp(saved.targetPosition, weight)
      saved.node.quaternion.slerp(saved.targetRotation, weight)
    }
    this.applied = true
  }

  private orient(bone: THREE.Bone, rotation: THREE.Quaternion) {
    const parent = bone
      .parent!.getWorldQuaternion(new THREE.Quaternion())
      .invert()
    bone.quaternion.copy(parent.multiply(rotation))
    bone.updateWorldMatrix(true, true)
  }

  private aim(bone: THREE.Bone, end: THREE.Bone, target: THREE.Vector3) {
    const start = bone.getWorldPosition(new THREE.Vector3())
    const from = end
      .getWorldPosition(new THREE.Vector3())
      .sub(start)
      .normalize()
    const to = target.clone().sub(start).normalize()
    const rotation = new THREE.Quaternion().setFromUnitVectors(from, to)
    this.orient(
      bone,
      bone.getWorldQuaternion(new THREE.Quaternion()).premultiply(rotation)
    )
  }

  private solveArm(
    root: THREE.Object3D,
    side: string,
    target: THREE.Vector3,
    pole: THREE.Vector3
  ) {
    const upper = findBoneByName(root, `${side}Arm`)
    const lower = findBoneByName(root, `${side}ForeArm`)
    const hand = findBoneByName(root, `${side}Hand`)
    if (!upper || !lower || !hand) return
    const start = upper.getWorldPosition(new THREE.Vector3())
    const elbow = lower.getWorldPosition(new THREE.Vector3())
    const end = hand.getWorldPosition(new THREE.Vector3())
    const rotation = hand.getWorldQuaternion(new THREE.Quaternion())
    const a = start.distanceTo(elbow),
      b = elbow.distanceTo(end)
    if (a < 1e-5 || b < 1e-5) return
    const direction = target.clone().sub(start)
    const distance = THREE.MathUtils.clamp(
      direction.length(),
      Math.abs(a - b) + 1e-5,
      a + b - 1e-5
    )
    direction.normalize()
    const bend = pole
      .addScaledVector(direction, -pole.dot(direction))
      .normalize()
    const along = (a * a - b * b + distance * distance) / (2 * distance)
    const across = Math.sqrt(Math.max(0, a * a - along * along))
    const jointTarget = start
      .clone()
      .addScaledVector(direction, along)
      .addScaledVector(bend, across)
    this.aim(upper, lower, jointTarget)
    this.aim(lower, hand, target)
    this.orient(hand, rotation)
  }
}
