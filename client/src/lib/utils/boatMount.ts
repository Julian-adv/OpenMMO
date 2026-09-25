import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'

export const ROWBOAT_MODEL_PATH = '/models/mounts/rowboat.glb'

const BOB_HEIGHT = 0.035
const BOB_SECONDS = 3.4
const ROLL_RADIANS = 0.035
const OAR_SWEEP = 0.55
const OAR_STROKE_SECONDS = 1.9
const OAR_HOLD_SECONDS = 5
// build_rowboat.py bakes the shipped shaft angle into the mesh.
const SHIPPED_ANGLE = Math.PI / 30
const SHAFT_AXIS = new THREE.Vector3(1, 0, 0)

export class BoatMount {
  readonly root: THREE.Object3D
  readonly seat: THREE.Object3D
  readonly grips: THREE.Object3D[] = []
  readonly bladeTips: THREE.Object3D[] = []
  readonly riderBaseOffsetY = -0.25
  rowingWeight = 0
  riderLean = 0
  private readonly oars: {
    node: THREE.Object3D
    side: number
    restPosition: THREE.Vector3
    restRotation: THREE.Quaternion
    alignment: THREE.Quaternion
  }[] = []
  private readonly rotation = new THREE.Quaternion()
  private readonly feather = new THREE.Quaternion()
  private readonly euler = new THREE.Euler(0, 0, 0, 'YXZ')
  private readonly point = new THREE.Vector3()
  private elapsed = 0
  private stroke = 0
  private strokeWeight = 0
  private holdRemaining = 0

  constructor(gltf: GLTF) {
    this.root = gltf.scene.clone()
    this.seat = this.root.getObjectByName('RideSeat') ?? this.root
    for (const [name, side] of [
      ['OarPort', -1],
      ['OarStarboard', 1],
    ] as const) {
      const node = this.root.getObjectByName(name)
      if (!node) continue
      const grip = new THREE.Object3D()
      grip.position.set(
        -side * Math.sin(SHIPPED_ANGLE),
        0,
        -Math.cos(SHIPPED_ANGLE)
      )
      node.add(grip)
      this.grips.push(grip)
      const bladeTip = new THREE.Object3D()
      bladeTip.position.copy(grip.position).multiplyScalar(-1.224)
      node.add(bladeTip)
      this.bladeTips.push(bladeTip)
      this.oars.push({
        node,
        side,
        restPosition: node.position.clone(),
        restRotation: node.quaternion.clone(),
        alignment: new THREE.Quaternion().setFromAxisAngle(
          new THREE.Vector3(0, 1, 0),
          side * (Math.PI / 2 - SHIPPED_ANGLE)
        ),
      })
    }
    this.root.traverse((node) => {
      if (node instanceof THREE.Mesh) {
        node.castShadow = true
        node.receiveShadow = true
      }
    })
  }

  update(dt: number, speed: number, canRow = true) {
    this.elapsed += dt
    const swell = (this.elapsed / BOB_SECONDS) * Math.PI * 2
    this.root.position.y = Math.sin(swell) * BOB_HEIGHT
    this.root.rotation.z = Math.cos(swell * 0.7) * ROLL_RADIANS

    const moving = canRow && Math.abs(speed) > 0.05
    if (moving) this.holdRemaining = OAR_HOLD_SECONDS
    else if (!canRow) this.holdRemaining = 0
    const holdDt = moving ? dt : Math.min(dt, this.holdRemaining)
    if (!moving) this.holdRemaining = Math.max(0, this.holdRemaining - dt)
    this.rowingWeight = THREE.MathUtils.damp(this.rowingWeight, 1, 8, holdDt)
    this.rowingWeight = THREE.MathUtils.damp(
      this.rowingWeight,
      0,
      4,
      dt - holdDt
    )
    this.strokeWeight = THREE.MathUtils.damp(
      this.strokeWeight,
      moving ? 1 : 0,
      8,
      dt
    )
    if (moving) {
      const cadence = THREE.MathUtils.clamp(Math.abs(speed) / 3, 0.7, 1.3)
      this.stroke += (dt / OAR_STROKE_SECONDS) * Math.PI * 2 * cadence
    }
    const sweep = Math.sin(this.stroke) * OAR_SWEEP * this.strokeWeight
    const dip =
      0.2 + 0.22 * Math.max(0, Math.cos(this.stroke)) * this.strokeWeight
    this.feather.setFromAxisAngle(
      SHAFT_AXIS,
      (Math.PI / 2) *
        THREE.MathUtils.smoothstep(Math.cos(this.stroke), -0.15, 0.15) *
        this.strokeWeight
    )
    this.riderLean = (0.12 - 0.2 * Math.sin(this.stroke)) * this.strokeWeight
    for (const oar of this.oars) {
      const { node, side } = oar
      this.rotation.setFromEuler(this.euler.set(0, side * sweep, -side * dip))
      this.point.set(side * 0.55, 0, 0).applyQuaternion(this.rotation)
      this.point.x += side * 0.57
      this.point.y += 0.44
      this.point.z -= 0.2
      node.position.lerpVectors(oar.restPosition, this.point, this.rowingWeight)
      this.rotation.multiply(this.feather).multiply(oar.alignment)
      node.quaternion.slerpQuaternions(
        oar.restRotation,
        this.rotation,
        this.rowingWeight
      )
    }
  }
}
