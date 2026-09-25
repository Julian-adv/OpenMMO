import * as THREE from 'three'
import { findBoneByName } from './characterAnimationUtils'

const SEGMENTS = 20
const UP = new THREE.Vector3(0, 1, 0)

export class HorseReins {
  private readonly mouth: THREE.Object3D | undefined
  private readonly hands: { bone: THREE.Bone; side: number; offset: number }[] =
    []
  private readonly geometry = new THREE.CylinderGeometry(0.006, 0.006, 1, 6)
  private readonly material = new THREE.MeshStandardMaterial({
    color: '#352315',
    roughness: 0.95,
  })
  private readonly mesh = new THREE.InstancedMesh(
    this.geometry,
    this.material,
    SEGMENTS * 2
  )
  private readonly start = new THREE.Vector3()
  private readonly end = new THREE.Vector3()
  private readonly previous = new THREE.Vector3()
  private readonly point = new THREE.Vector3()
  private readonly direction = new THREE.Vector3()
  private readonly center = new THREE.Vector3()
  private readonly scale = new THREE.Vector3(1, 1, 1)
  private readonly rotation = new THREE.Quaternion()
  private readonly matrix = new THREE.Matrix4()

  constructor(
    private readonly horse: THREE.Object3D,
    rider: THREE.Object3D
  ) {
    this.mouth = horse.getObjectByName('Mouth')
    for (const [name, side] of [
      ['Left', 1],
      ['Right', -1],
    ] as const) {
      const finger = findBoneByName(rider, `${name}HandIndex1`)
      const bone = finger ?? findBoneByName(rider, `${name}Hand`)
      if (bone) this.hands.push({ bone, side, offset: finger ? 0 : 0.05 })
    }
    this.mesh.name = 'HorseReins'
    this.mesh.count = 0
    this.mesh.frustumCulled = false
    this.mesh.castShadow = true
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.mesh.raycast = () => {}
    horse.add(this.mesh)
  }

  update() {
    this.mesh.count = 0
    if (!this.mouth) return
    for (const hand of this.hands) {
      // Mouth's local -Z points toward the horse's left cheek.
      this.start.set(-0.025, 0, -hand.side * 0.065)
      this.horse.worldToLocal(this.mouth.localToWorld(this.start))
      this.end.set(0, hand.offset, 0)
      this.horse.worldToLocal(hand.bone.localToWorld(this.end))
      const sag = Math.min(0.24, this.start.distanceTo(this.end) * 0.14)
      this.previous.copy(this.start)
      for (let i = 1; i <= SEGMENTS; i++) {
        const t = i / SEGMENTS
        const curve = 4 * t * (1 - t)
        this.point.lerpVectors(this.start, this.end, t)
        this.point.y -= sag * curve
        this.point.x += hand.side * 0.08 * curve
        this.direction.subVectors(this.point, this.previous)
        this.scale.y = this.direction.length()
        this.rotation.setFromUnitVectors(UP, this.direction.normalize())
        this.center.addVectors(this.previous, this.point).multiplyScalar(0.5)
        this.matrix.compose(this.center, this.rotation, this.scale)
        this.mesh.setMatrixAt(this.mesh.count++, this.matrix)
        this.previous.copy(this.point)
      }
    }
    this.mesh.instanceMatrix.needsUpdate = true
  }

  dispose() {
    this.mesh.removeFromParent()
    this.mesh.dispose()
    this.geometry.dispose()
    this.material.dispose()
  }
}
