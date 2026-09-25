import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { color, exp, float, mix, uniform, uv } from 'three/tsl'
import {
  TELEPORT_ARRIVAL_MS,
  TELEPORT_DEPARTURE_MS,
  TELEPORT_REVEAL_MS,
  TELEPORT_VANISH_MS,
  type TeleportEffect,
} from '../stores/teleportEffectStore'

const HEIGHT = 18

export class TeleportEffectSystem {
  readonly group = new THREE.Group()
  private geometry = new THREE.PlaneGeometry(1, 1)
  private beamMaterial = new MeshBasicNodeMaterial()
  private ringMaterial = new MeshBasicNodeMaterial()
  private glowMaterial = new MeshBasicNodeMaterial()
  private beamOpacity = uniform(0)
  private floorOpacity = uniform(0)
  private beams: THREE.Mesh[] = []
  private ring = new THREE.Mesh(this.geometry, this.ringMaterial)
  private glow = new THREE.Mesh(this.geometry, this.glowMaterial)
  private sparks = new THREE.InstancedMesh(this.geometry, this.beamMaterial, 20)
  private dummy = new THREE.Object3D()

  constructor() {
    for (const material of [
      this.beamMaterial,
      this.ringMaterial,
      this.glowMaterial,
    ]) {
      material.transparent = true
      material.depthWrite = false
      material.blending = THREE.AdditiveBlending
      material.side = THREE.DoubleSide
      material.toneMapped = false
    }
    const distance = uv().x.sub(0.5).abs()
    const core = exp(distance.mul(28).pow(2).negate())
    const halo = exp(distance.mul(5).pow(2).negate())
    this.beamMaterial.colorNode = mix(color('#69bfff'), color('#f2ffff'), core)
    this.beamMaterial.opacityNode = core
      .mul(0.85)
      .add(halo.mul(0.25))
      .mul(uv().y.smoothstep(0, 0.08))
      .mul(float(1).sub(uv().y).smoothstep(0, 0.18))
      .mul(this.beamOpacity)
    const radius = uv().sub(0.5).length().mul(2)
    this.ringMaterial.colorNode = color('#a3e5ff')
    this.ringMaterial.opacityNode = exp(
      radius.sub(0.72).mul(25).pow(2).negate()
    )
      .add(exp(radius.mul(3).pow(2).negate()).mul(0.3))
      .mul(this.floorOpacity)
    this.glowMaterial.colorNode = color('#dcfaff')
    this.glowMaterial.opacityNode = float(1)
      .sub(radius)
      .max(0)
      .pow(3)
      .mul(this.floorOpacity)
    for (let i = 0; i < 3; i++) {
      const beam = new THREE.Mesh(this.geometry, this.beamMaterial)
      beam.rotation.y = (i * Math.PI) / 3
      this.beams.push(beam)
      this.group.add(beam)
    }
    this.ring.rotation.x = -Math.PI / 2
    this.ring.position.y = 0.06
    this.glow.position.y = 1.1
    this.sparks.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.sparks.frustumCulled = false
    this.group.add(this.ring, this.glow, this.sparks)
    this.clear()
  }

  update(
    elapsedMs: number,
    phase: TeleportEffect['phase'],
    camera: THREE.Camera
  ) {
    const departing = phase === 'Departing'
    const duration = departing ? TELEPORT_DEPARTURE_MS : TELEPORT_ARRIVAL_MS
    if (phase === 'Cancelled' || elapsedMs >= duration) {
      this.clear()
      return false
    }
    this.group.visible = true
    const time = Math.max(0, elapsedMs)
    const contact = departing ? TELEPORT_VANISH_MS : TELEPORT_REVEAL_MS
    const progress = Math.min(1, time / contact)
    const fade = Math.max(0, (time - contact) / (duration - contact))
    const bottom = departing
      ? HEIGHT * fade * fade
      : HEIGHT * (1 - progress) ** 2
    const top = departing ? HEIGHT * (1 - (1 - progress) ** 3) : HEIGHT
    const width = (departing ? 1.8 - progress * 0.6 : 1.8) * (1 - fade * 0.8)
    this.beamOpacity.value = Math.min(1, time / 55) * (1 - fade) ** 1.3
    this.floorOpacity.value = departing
      ? Math.sin(Math.PI * Math.min(1, time / (contact + 100)))
      : Math.max(0, 1 - Math.abs(time - contact) / 320)
    for (const beam of this.beams) {
      beam.position.y = (bottom + top) / 2
      beam.scale.set(width, Math.max(0.01, top - bottom), 1)
    }
    this.ring.scale.setScalar(departing ? 2.8 - progress * 0.9 : 1.8 + fade * 4)
    this.glow.scale.setScalar(4 + this.floorOpacity.value * 2)
    this.glow.quaternion.copy(camera.quaternion)
    for (let i = 0; i < this.sparks.count; i++) {
      const offset = i / this.sparks.count
      const travel = (time / 450 + offset * 0.7) % 1
      const angle = i * 2.39996
      const radius = 0.35 + (i % 4) * 0.18
      this.dummy.position.set(
        Math.cos(angle) * radius,
        (departing ? travel : 1 - travel) * HEIGHT,
        Math.sin(angle) * radius
      )
      this.dummy.rotation.set(0, angle, 0)
      this.dummy.scale.set(0.12, 0.4 + (i % 3) * 0.3, 1)
      this.dummy.updateMatrix()
      this.sparks.setMatrixAt(i, this.dummy.matrix)
    }
    this.sparks.instanceMatrix.needsUpdate = true
    return true
  }

  clear() {
    this.group.visible = false
    this.beamOpacity.value = 0
    this.floorOpacity.value = 0
  }

  dispose() {
    this.geometry.dispose()
    this.beamMaterial.dispose()
    this.ringMaterial.dispose()
    this.glowMaterial.dispose()
    this.sparks.dispose()
    this.group.clear()
  }
}
