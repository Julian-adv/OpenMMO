import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import {
  attribute,
  color,
  exp,
  float,
  mix,
  normalLocal,
  normalView,
  positionLocal,
  sin,
  texture,
  uniform,
  uv,
  vec2,
} from 'three/tsl'
import type { EnchantEffectAnchor } from '../utils/playerEffectAnchors'
import { createWeaponGlowGeometry } from '../utils/weaponGlowGeometry'
import {
  ENCHANT_WEAPON_RAYS,
  getWeaponEffectAxis,
} from '../utils/weaponEffectAxis'
import {
  ENCHANT_LIGHT_INTENSITY,
  type EnchantLight,
} from '../utils/enchantLight'

export const ENCHANT_SUCCESS_DURATION = 5
export const ENCHANT_SUCCESS_GATHER_DURATION = 0.3
export const ENCHANT_SUCCESS_RELEASE_START = ENCHANT_SUCCESS_DURATION - 0.3
const ENCHANT_RAY_CYCLE_SECONDS = 3.2

export class EnchantSuccessEffect {
  readonly group = new THREE.Group()
  readonly light: EnchantLight = {
    playerId: -1,
    position: new THREE.Vector3(),
    intensity: 0,
  }
  private opacity = uniform(0)
  private phase = uniform(0)
  private dissolve = uniform(-0.2)
  private geometry = new THREE.PlaneGeometry(1, 1)
  private glowMaterial = new MeshBasicNodeMaterial()
  private threadMaterial = new MeshBasicNodeMaterial()
  private rayMaterial = new MeshBasicNodeMaterial()
  private axisMaterial = new MeshBasicNodeMaterial()
  private bladeMaterial = new MeshBasicNodeMaterial()
  private rayGeometry = new THREE.PlaneGeometry(1, 1).translate(0, 0.5, 0)
  private rayOpacity = new THREE.InstancedBufferAttribute(
    new Float32Array(ENCHANT_WEAPON_RAYS),
    1
  )
  private rays: THREE.InstancedMesh
  private axisGlow: THREE.Mesh
  private bladeGlow: THREE.Mesh
  private glowingWeaponId: string | null = null
  private bladeGeometry: THREE.BufferGeometry | null = null
  private core: THREE.Mesh
  private threads: THREE.Mesh[]
  private dust: THREE.InstancedMesh
  private dummy = new THREE.Object3D()
  private cameraDirection = new THREE.Vector3()
  private rayDirection = new THREE.Vector3()
  private screenDirection = new THREE.Vector3()
  private rayRotation = new THREE.Matrix4()
  private rayNormal = new THREE.Vector3()
  private weaponCenter = new THREE.Vector3()
  private weaponAxis = new THREE.Vector3()
  private radialAxis = new THREE.Vector3()
  private radialSide = new THREE.Vector3()

  constructor(map: THREE.Texture) {
    for (const material of [
      this.glowMaterial,
      this.threadMaterial,
      this.rayMaterial,
      this.axisMaterial,
      this.bladeMaterial,
    ]) {
      material.transparent = true
      material.depthWrite = false
      material.blending = THREE.AdditiveBlending
      material.side = THREE.DoubleSide
      material.toneMapped = false
    }
    const radial = float(1).sub(uv().sub(0.5).length().mul(2)).max(0)
    this.glowMaterial.colorNode = color('#f5e6bf')
    this.glowMaterial.opacityNode = radial.pow(2.8).mul(this.opacity)
    const flow = vec2(
      sin(uv().y.mul(16).sub(this.phase.mul(5))).mul(0.018),
      sin(uv().x.mul(13).add(this.phase.mul(4))).mul(0.012)
    )
    const sample = texture(map, uv().add(flow).clamp(0.001, 0.999))
    const breakup = sin(uv().x.mul(23).add(uv().y.mul(17)))
      .mul(sin(uv().y.mul(31).sub(this.phase.mul(2))))
      .mul(0.25)
      .add(uv().y.mul(0.5))
      .add(0.25)
    const erosion = breakup.smoothstep(this.dissolve, this.dissolve.add(0.25))
    const border = uv().min(float(1).sub(uv()))
    const feather = border.x
      .smoothstep(0, 0.08)
      .mul(border.y.smoothstep(0, 0.08))
    this.threadMaterial.colorNode = sample.rgb
    this.threadMaterial.opacityNode = sample.a
      .mul(this.opacity)
      .mul(erosion)
      .mul(feather)
      .mul(0.45)
    const spread = uv().y.pow(0.75).mul(0.35).add(0.05)
    const diffusion = exp(uv().x.sub(0.5).div(spread).pow(2).mul(-2))
    const filament = exp(uv().x.sub(0.5).div(0.035).pow(2).mul(-2))
    const outwardFade = exp(uv().y.mul(-1.6)).mul(float(1).sub(uv().y).pow(1.2))
    this.rayMaterial.colorNode = mix(
      color('#fff7dc'),
      color('#e5bf78'),
      uv().y.smoothstep(0, 0.7)
    )
    this.rayMaterial.opacityNode = diffusion
      .mul(0.24)
      .add(filament.mul(0.8))
      .mul(outwardFade)
      .mul(attribute('aEnchantRayOpacity', 'float'))
      .mul(this.opacity)
    this.axisMaterial.colorNode = color('#fff2c6')
    this.axisMaterial.opacityNode = exp(uv().x.sub(0.5).mul(6).pow(2).negate())
      .mul(uv().y.smoothstep(0, 0.1))
      .mul(float(1).sub(uv().y).smoothstep(0, 0.1))
      .mul(this.opacity)
      .mul(0.22)
    this.axisGlow = new THREE.Mesh(this.geometry, this.axisMaterial)
    this.axisGlow.name = 'enchant-weapon-axis-glow'
    const zigzag = uv().y.mul(9).fract().sub(0.5).abs().mul(2).sub(0.5)
    const branch = sin(uv().y.mul(29).add(this.phase.mul(0.08))).mul(0.22)
    const veinA = exp(uv().x.sub(0.5).sub(zigzag.mul(0.35)).pow(2).mul(-4500))
    const veinB = exp(uv().x.sub(0.48).sub(branch).pow(2).mul(-6500))
    const veins = veinA.add(veinB).clamp(0, 1)
    const rim = float(1).sub(normalView.z.abs()).pow(2)
    this.bladeMaterial.positionNode = positionLocal.add(normalLocal.mul(0.002))
    this.bladeMaterial.colorNode = mix(
      color('#e5b96b'),
      color('#fff9e6'),
      veins.max(rim)
    )
    this.bladeMaterial.opacityNode = veins
      .mul(0.8)
      .add(rim.mul(0.7))
      .add(0.16)
      .mul(uv().y.smoothstep(0.24, 0.32))
      .mul(this.opacity)
    this.bladeGlow = new THREE.Mesh(this.geometry, this.bladeMaterial)
    this.bladeGlow.name = 'enchant-weapon-blade-glow'
    this.bladeGlow.matrixAutoUpdate = false
    this.rayGeometry.setAttribute('aEnchantRayOpacity', this.rayOpacity)
    this.rayOpacity.setUsage(THREE.DynamicDrawUsage)
    this.rays = new THREE.InstancedMesh(
      this.rayGeometry,
      this.rayMaterial,
      ENCHANT_WEAPON_RAYS
    )
    this.rays.name = 'enchant-weapon-rays'
    this.rays.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.core = new THREE.Mesh(this.geometry, this.glowMaterial)
    this.threads = Array.from(
      { length: 3 },
      () => new THREE.Mesh(this.geometry, this.threadMaterial)
    )
    this.dust = new THREE.InstancedMesh(this.geometry, this.glowMaterial, 32)
    this.dust.frustumCulled = false
    this.dust.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.group.add(
      this.core,
      ...this.threads,
      this.dust,
      this.rays,
      this.axisGlow,
      this.bladeGlow
    )
    this.group.traverse((object) => {
      object.frustumCulled = false
    })
    this.clear()
  }

  update(
    elapsed: number,
    anchor: EnchantEffectAnchor,
    camera: THREE.Camera,
    reduced = false
  ) {
    if (elapsed >= ENCHANT_SUCCESS_DURATION) {
      this.clear()
      return
    }
    const p = THREE.MathUtils.clamp(
      elapsed / ENCHANT_SUCCESS_GATHER_DURATION,
      0,
      1
    )
    const release =
      elapsed < ENCHANT_SUCCESS_RELEASE_START
        ? null
        : (elapsed - ENCHANT_SUCCESS_RELEASE_START) * 2
    const tail = THREE.MathUtils.clamp((release ?? 0) / 0.6, 0, 1)
    const easedTail = tail * tail * (3 - 2 * tail)
    const fade =
      release === null ? THREE.MathUtils.smoothstep(p, 0, 0.2) : 1 - easedTail
    this.phase.value = elapsed * 2 + tail * 0.7
    this.dissolve.value = -0.2 + easedTail * 1.4
    this.group.visible = fade > 0
    camera.getWorldDirection(this.cameraDirection)
    this.group.position
      .copy(anchor.position)
      .addScaledVector(this.cameraDirection, -0.5)
    this.light.position.copy(this.group.position)
    this.opacity.value = fade * (0.35 + p * 0.65)
    this.light.intensity = fade * p * ENCHANT_LIGHT_INTENSITY
    this.rays.visible = anchor.weapon !== null
    this.axisGlow.visible = anchor.weapon !== null
    this.bladeGlow.visible = anchor.weapon !== null
    if (anchor.weapon) {
      this.threads.forEach((thread) => {
        thread.visible = false
      })
      this.updateWeapon(anchor.weapon, elapsed, camera, reduced)
      return
    }
    this.core.position.set(0, 0, 0)
    this.core.quaternion.copy(camera.quaternion)
    this.core.scale.setScalar(
      release === null ? 0.18 + p * 0.38 : 0.56 + easedTail * 0.65
    )
    for (let i = 0; i < this.threads.length; i++) {
      const thread = this.threads[i]
      thread.visible = !reduced || i === 0
      const angle = i * 2.094 + p * 3 + tail * 0.65
      const radius =
        release === null ? 0.23 * (1 - p) + 0.06 : 0.06 + release * 0.5
      thread.position.set(
        Math.cos(angle) * radius,
        Math.sin(angle) * radius + 0.1,
        0
      )
      thread.quaternion.copy(camera.quaternion)
      thread.rotateZ(angle)
      thread.scale.setScalar(0.4 + p * 0.35 + easedTail * 0.18)
    }
    this.dust.count = reduced ? 8 : 20
    for (let i = 0; i < this.dust.count; i++) {
      const angle = i * 2.39996 + p * 2 + tail * 0.4
      const radius =
        release === null
          ? (0.45 + (i % 4) * 0.13) * (1 - p) + 0.08
          : 0.08 + release * (0.5 + (i % 4) * 0.2)
      this.dummy.position.set(
        Math.cos(angle) * radius,
        Math.sin(angle) * radius * 0.7 + (release ?? 0) * 0.4,
        Math.sin(i * 1.7) * radius * 0.4
      )
      this.dummy.quaternion.copy(camera.quaternion)
      this.dummy.scale.setScalar((0.035 + (i % 3) * 0.015) * fade)
      this.dummy.updateMatrix()
      this.dust.setMatrixAt(i, this.dummy.matrix)
    }
    this.dust.instanceMatrix.needsUpdate = true
  }

  private updateWeapon(
    weapon: THREE.Object3D,
    elapsed: number,
    camera: THREE.Camera,
    reduced: boolean
  ) {
    const axis = getWeaponEffectAxis(weapon)
    if (!weapon.visible || axis.length <= 0) {
      this.clear()
      return
    }
    weapon.updateWorldMatrix(true, false)
    if (this.glowingWeaponId !== weapon.uuid) {
      this.bladeGeometry?.dispose()
      this.bladeGeometry = createWeaponGlowGeometry(weapon)
      this.bladeGlow.geometry = this.bladeGeometry
      this.glowingWeaponId = weapon.uuid
    }
    this.weaponCenter.copy(axis.center).applyMatrix4(weapon.matrixWorld)
    this.weaponAxis.copy(axis.direction).transformDirection(weapon.matrixWorld)
    const axisLength = this.dummy.position
      .copy(axis.center)
      .addScaledVector(axis.direction, axis.length)
      .applyMatrix4(weapon.matrixWorld)
      .distanceTo(this.weaponCenter)
    this.radialAxis.crossVectors(this.weaponAxis, this.cameraDirection)
    if (this.radialAxis.lengthSq() < 0.001)
      this.radialAxis.set(1, 0, 0).applyQuaternion(camera.quaternion)
    this.radialAxis.normalize()
    this.radialSide.crossVectors(this.weaponAxis, this.radialAxis).normalize()
    this.group.position
      .copy(this.weaponCenter)
      .addScaledVector(this.cameraDirection, -0.12)
    this.core.position.copy(this.weaponAxis).multiplyScalar(axisLength * 0.12)
    this.core.quaternion.copy(camera.quaternion)
    this.core.scale.setScalar(0.65 + Math.sin(elapsed * 2.4) * 0.04)
    this.light.position
      .copy(this.weaponCenter)
      .add(this.core.position)
      .addScaledVector(this.cameraDirection, -0.35)
    this.bladeGlow.matrix.copy(weapon.matrixWorld)
    this.bladeGlow.matrix.setPosition(
      this.dummy.position
        .setFromMatrixPosition(weapon.matrixWorld)
        .sub(this.group.position)
        .addScaledVector(this.cameraDirection, -0.015)
    )
    this.rayNormal.crossVectors(this.radialAxis, this.weaponAxis).normalize()
    this.rayRotation.makeBasis(this.radialAxis, this.weaponAxis, this.rayNormal)
    this.axisGlow.quaternion.setFromRotationMatrix(this.rayRotation)
    this.axisGlow.scale.set(0.14, axisLength + 0.04, 1)
    const count = reduced ? ENCHANT_WEAPON_RAYS / 2 : ENCHANT_WEAPON_RAYS
    const maxActive = count / 3
    let active = 0
    for (let i = 0; i < count; i++) {
      const phase =
        (elapsed / ENCHANT_RAY_CYCLE_SECONDS + ((i * 13) % count) / count) % 1
      const life = phase * 3
      if (life >= 1 || active >= maxActive) continue
      const pulse =
        THREE.MathUtils.smoothstep(life, 0, 0.18) *
        (1 - THREE.MathUtils.smoothstep(life, 0.55, 1))
      if (pulse <= 0) continue
      const angle = i * 2.399963229728653
      this.rayDirection
        .copy(this.radialAxis)
        .multiplyScalar(Math.cos(angle))
        .addScaledVector(this.weaponAxis, Math.sin(angle))
      this.screenDirection.crossVectors(this.rayDirection, this.cameraDirection)
      if (this.screenDirection.lengthSq() < 0.001)
        this.screenDirection.copy(this.weaponAxis)
      this.screenDirection.normalize()
      this.rayNormal
        .crossVectors(this.screenDirection, this.rayDirection)
        .normalize()
      const length =
        axisLength *
        (0.9 + (i % 5) * 0.18) *
        (0.65 + THREE.MathUtils.smoothstep(life, 0, 0.7) * 0.35)
      this.dummy.position.copy(this.core.position)
      this.rayRotation.makeBasis(
        this.screenDirection,
        this.rayDirection,
        this.rayNormal
      )
      this.dummy.quaternion.setFromRotationMatrix(this.rayRotation)
      this.dummy.scale.set(0.3 + (i % 3) * 0.12, length, 1)
      this.dummy.updateMatrix()
      this.rays.setMatrixAt(active, this.dummy.matrix)
      this.rayOpacity.setX(active, pulse)
      active++
    }
    this.rays.count = active
    this.rays.instanceMatrix.needsUpdate = true
    this.rayOpacity.needsUpdate = true
    this.dust.count = reduced ? 12 : 32
    for (let i = 0; i < this.dust.count; i++) {
      const life = (elapsed * 0.3 + i * 0.61803398875) % 1
      const angle = i * 2.399963229728653
      const radius = axisLength * (0.15 + life * 1.2)
      this.dummy.position
        .copy(this.core.position)
        .addScaledVector(this.radialAxis, Math.cos(angle) * radius)
        .addScaledVector(this.weaponAxis, Math.sin(angle) * radius)
        .addScaledVector(this.radialSide, Math.sin(i * 1.7) * 0.1)
      this.dummy.quaternion.copy(camera.quaternion)
      this.dummy.scale.setScalar(
        (0.035 + (i % 3) * 0.015) * Math.sin(Math.PI * life)
      )
      this.dummy.updateMatrix()
      this.dust.setMatrixAt(i, this.dummy.matrix)
    }
    this.dust.instanceMatrix.needsUpdate = true
  }

  clear() {
    this.group.visible = false
    this.light.intensity = 0
  }

  dispose() {
    this.clear()
    this.dust.dispose()
    this.rays.dispose()
    this.geometry.dispose()
    this.rayGeometry.dispose()
    this.glowMaterial.dispose()
    this.threadMaterial.dispose()
    this.rayMaterial.dispose()
    this.axisMaterial.dispose()
    this.bladeMaterial.dispose()
    this.bladeGeometry?.dispose()
    this.glowingWeaponId = null
    this.group.clear()
    this.group.removeFromParent()
  }
}
