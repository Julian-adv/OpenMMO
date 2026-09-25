import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import {
  attribute,
  float,
  length,
  smoothstep,
  texture,
  uniform,
  uv,
  vec3,
} from 'three/tsl'
import type { BoatMount } from '../utils/boatMount'

type SurfaceAt = (x: number, z: number) => number | null
const WAKE_SPACING = 0.12
const WAKE_LIFETIME = 2
const WATER_LIFT = 0.035

class WaterParticles {
  readonly mesh: THREE.InstancedMesh<THREE.PlaneGeometry, MeshBasicNodeMaterial>
  private readonly brightness = uniform(1)
  private readonly opacity: THREE.InstancedBufferAttribute
  private readonly patches: THREE.InstancedBufferAttribute
  private readonly pool
  private next = 0
  private readonly matrix = new THREE.Matrix4()
  private readonly rotation = new THREE.Quaternion()
  private readonly scale = new THREE.Vector3()
  private readonly up = new THREE.Vector3(0, 1, 0)

  constructor(
    foam: THREE.Texture,
    capacity: number,
    private readonly spray: boolean
  ) {
    const geometry = new THREE.PlaneGeometry(1, 1)
    if (!spray) geometry.rotateX(-Math.PI / 2)
    this.opacity = new THREE.InstancedBufferAttribute(
      new Float32Array(capacity),
      1
    )
    this.patches = new THREE.InstancedBufferAttribute(
      new Float32Array(capacity * 2),
      2
    )
    this.opacity.setUsage(THREE.DynamicDrawUsage)
    this.patches.setUsage(THREE.DynamicDrawUsage)
    geometry.setAttribute('aBoatOpacity', this.opacity)
    geometry.setAttribute('aBoatPatch', this.patches)
    const material = new MeshBasicNodeMaterial({
      transparent: true,
      depthWrite: false,
      side: THREE.DoubleSide,
    })
    const patch = texture(
      foam,
      uv().mul(0.2).add(attribute('aBoatPatch', 'vec2'))
    ).r
    const radial = float(1).sub(smoothstep(0.12, 0.5, length(uv().sub(0.5))))
    material.colorNode = vec3(0.9, 0.96, 1).mul(this.brightness)
    material.opacityNode = (
      spray ? patch.mul(0.3).add(0.7) : smoothstep(0.22, 0.58, patch)
    )
      .mul(radial)
      .mul(attribute('aBoatOpacity', 'float'))
    this.mesh = new THREE.InstancedMesh(geometry, material, capacity)
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.mesh.count = 0
    this.mesh.frustumCulled = false
    this.mesh.renderOrder = spray ? 3 : 2
    this.pool = Array.from({ length: capacity }, () => ({
      position: new THREE.Vector3(),
      velocity: new THREE.Vector3(),
      age: 0,
      motionAge: 0,
      lifetime: 0,
      size: 0,
      yaw: 0,
      u: 0,
      v: 0,
    }))
  }

  emit(
    position: THREE.Vector3,
    velocity: THREE.Vector3,
    size: number,
    lifetime: number
  ) {
    const particle = this.pool[this.next]
    this.next = (this.next + 1) % this.pool.length
    particle.position.copy(position)
    particle.velocity.copy(velocity)
    particle.age = 0
    particle.motionAge = 0
    particle.lifetime = lifetime
    particle.size = size
    particle.yaw = Math.random() * Math.PI * 2
    particle.u = Math.random() * 0.8
    particle.v = Math.random() * 0.8
  }

  update(
    dt: number,
    camera: THREE.Camera,
    surfaceAt: SurfaceAt,
    brightness: number,
    drifting = true
  ) {
    this.brightness.value = brightness
    if (this.spray) camera.getWorldQuaternion(this.rotation)
    let count = 0
    for (const particle of this.pool) {
      if (particle.age >= particle.lifetime) continue
      particle.age += dt
      if (particle.age >= particle.lifetime) continue
      if (!this.spray && !drifting) particle.velocity.set(0, 0, 0)
      if (particle.velocity.lengthSq() > 0) particle.motionAge += dt
      if (this.spray) particle.velocity.y -= 6 * dt
      particle.position.addScaledVector(particle.velocity, dt)
      const surface = surfaceAt(particle.position.x, particle.position.z)
      if (surface === null || (this.spray && particle.position.y < surface)) {
        particle.lifetime = 0
        continue
      }
      if (!this.spray) {
        particle.position.y = surface + WATER_LIFT
        this.rotation.setFromAxisAngle(this.up, particle.yaw)
      }
      const t = particle.age / particle.lifetime
      const growth = this.spray ? 1 : 1 + particle.motionAge / particle.lifetime
      const size = particle.size * growth
      this.scale.set(size, this.spray ? size * 1.4 : 1, size)
      this.matrix.compose(particle.position, this.rotation, this.scale)
      this.mesh.setMatrixAt(count, this.matrix)
      this.opacity.setX(
        count,
        Math.min(particle.age / 0.06, 1) *
          (this.spray ? (1 - t) ** 1.3 : (0.85 * (1 - t) ** 2.2) / growth)
      )
      this.patches.setXY(count, particle.u, particle.v)
      count++
    }
    this.mesh.count = count
    this.mesh.visible = count > 0
    this.mesh.instanceMatrix.needsUpdate = true
    this.opacity.needsUpdate = true
    this.patches.needsUpdate = true
  }

  clear() {
    for (const particle of this.pool) particle.lifetime = 0
    this.mesh.count = 0
    this.mesh.visible = false
  }

  dispose() {
    this.mesh.dispose()
    this.mesh.geometry.dispose()
    this.mesh.material.dispose()
    this.mesh.removeFromParent()
  }
}

export class BoatWaterEffects {
  readonly group = new THREE.Group()
  private readonly foam: WaterParticles
  private readonly spray: WaterParticles
  private readonly contacts
  private readonly previous = new THREE.Vector3()
  private readonly origin = new THREE.Vector3()
  private readonly forward = new THREE.Vector3()
  private readonly right = new THREE.Vector3()
  private readonly delta = new THREE.Vector3()
  private readonly point = new THREE.Vector3()
  private readonly velocity = new THREE.Vector3()
  private initialized = false
  private wakeDistance = 0

  constructor(
    private readonly boat: BoatMount,
    foamMap: THREE.Texture,
    private readonly surfaceAt: SurfaceAt
  ) {
    this.group.name = 'boatWaterEffects'
    this.foam = new WaterParticles(foamMap, 192, false)
    this.spray = new WaterParticles(foamMap, 64, true)
    this.foam.mesh.name = 'boatWakeFoam'
    this.spray.mesh.name = 'boatOarSpray'
    this.group.add(this.foam.mesh, this.spray.mesh)
    this.contacts = boat.bladeTips.map(() => ({
      previous: new THREE.Vector3(),
      current: new THREE.Vector3(),
      depth: 0,
      armed: false,
    }))
  }

  update(dt: number, camera: THREE.Camera, rowing: boolean, sunY = 1) {
    if (dt <= 0) return
    this.boat.root.updateWorldMatrix(true, true)
    this.origin.setFromMatrixPosition(this.boat.root.matrixWorld)
    this.delta.subVectors(this.origin, this.previous)
    const surface = this.surfaceAt(this.origin.x, this.origin.z)
    if (
      surface === null ||
      Math.abs(this.origin.y - surface) > 2 ||
      dt > 0.25 ||
      (this.initialized && this.delta.length() > 2)
    ) {
      this.clear()
      return
    }
    this.forward.set(0, 0, 1).transformDirection(this.boat.root.matrixWorld)
    this.forward.y = 0
    this.forward.normalize()
    this.right.set(this.forward.z, 0, -this.forward.x)
    const distance = this.delta.dot(this.forward)
    const advancing = this.initialized && rowing && distance > dt * 0.05
    if (advancing) {
      const speed = Math.min(distance / dt, 6)
      for (
        let along = WAKE_SPACING - this.wakeDistance;
        along <= distance;
        along += WAKE_SPACING
      ) {
        for (const side of [-1, 1]) {
          this.point
            .lerpVectors(this.previous, this.origin, along / distance)
            .addScaledVector(this.forward, 1.45)
            .addScaledVector(this.right, side * (0.1 + Math.random() * 0.035))
          const waterY = this.surfaceAt(this.point.x, this.point.z)
          if (waterY === null) continue
          this.point.y = waterY + WATER_LIFT
          this.velocity.copy(this.right).multiplyScalar(side * speed * 0.2)
          this.foam.emit(
            this.point,
            this.velocity,
            0.28 + Math.random() * 0.08,
            WAKE_LIFETIME * (0.6 + Math.random() * 0.4)
          )
        }
      }
      this.wakeDistance = (this.wakeDistance + distance) % WAKE_SPACING
    } else this.wakeDistance = 0

    for (let i = 0; i < this.contacts.length; i++) {
      const contact = this.contacts[i]
      contact.current.setFromMatrixPosition(this.boat.bladeTips[i].matrixWorld)
      const waterY = this.surfaceAt(contact.current.x, contact.current.z)
      const depth = waterY === null ? Infinity : contact.current.y - waterY
      if (!rowing || waterY === null) contact.armed = false
      else if (depth > 0.035) contact.armed = true
      else if (this.initialized && contact.armed && depth <= 0) {
        const fraction = THREE.MathUtils.clamp(
          contact.depth / (contact.depth - depth),
          0,
          1
        )
        this.point.lerpVectors(contact.previous, contact.current, fraction)
        this.point.y = this.surfaceAt(this.point.x, this.point.z) ?? waterY
        this.splash(this.point, contact.current, contact.previous, dt)
        contact.armed = false
      }
      contact.previous.copy(contact.current)
      contact.depth = depth
    }
    this.previous.copy(this.origin)
    this.initialized = true
    const day = THREE.MathUtils.smoothstep(sunY, -0.05, 0.1)
    const brightness = 0.12 + day * 0.88
    this.foam.update(dt, camera, this.surfaceAt, brightness, advancing)
    this.spray.update(dt, camera, this.surfaceAt, brightness)
  }

  private splash(
    point: THREE.Vector3,
    current: THREE.Vector3,
    previous: THREE.Vector3,
    dt: number
  ) {
    this.delta
      .subVectors(current, previous)
      .multiplyScalar(0.12 / dt)
      .clampLength(0, 1)
    for (let i = 0; i < 12; i++) {
      const angle = Math.random() * Math.PI * 2
      const spread = 0.25 + Math.random() * 0.65
      this.velocity.set(
        Math.cos(angle) * spread + this.delta.x,
        1.1 + Math.random(),
        Math.sin(angle) * spread + this.delta.z
      )
      this.spray.emit(
        point,
        this.velocity,
        0.035 + Math.random() * 0.035,
        0.45 + Math.random() * 0.25
      )
    }
    this.velocity.copy(this.delta).setY(0).multiplyScalar(0.3)
    this.foam.emit(point, this.velocity, 0.4, 0.85)
  }

  clear() {
    this.foam.clear()
    this.spray.clear()
    this.initialized = false
    this.wakeDistance = 0
    for (const contact of this.contacts) contact.armed = false
  }

  dispose() {
    this.foam.dispose()
    this.spray.dispose()
    this.group.removeFromParent()
  }
}
