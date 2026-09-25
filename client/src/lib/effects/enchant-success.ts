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
  uniform,
  uv,
} from 'three/tsl'
import type { EnchantEffectAnchor } from '../utils/playerEffectAnchors'
import { createWeaponGlowGeometry } from '../utils/weaponGlowGeometry'
import {
  ENCHANT_WEAPON_RAYS,
  getWeaponEffectAxis,
} from '../utils/weaponEffectAxis'
import {
  ENCHANT_LIGHT_INTENSITY,
  ENCHANT_ARMOR_LIGHT_INTENSITY,
  type EnchantLight,
} from '../utils/enchantLight'

export const ENCHANT_SUCCESS_DURATION = 5
export const ENCHANT_SUCCESS_GATHER_DURATION = 0.3
export const ENCHANT_SUCCESS_RELEASE_START = ENCHANT_SUCCESS_DURATION - 0.3
const ENCHANT_RAY_CYCLE_SECONDS = 3.2
const ENCHANT_ARMOR_RAYS = 60
const ENCHANT_MAX_RAYS = Math.max(ENCHANT_WEAPON_RAYS, ENCHANT_ARMOR_RAYS)
const ENCHANT_ARMOR_RAY_CYCLE_SECONDS = 2.1

export class EnchantSuccessEffect {
  readonly group = new THREE.Group()
  readonly light: EnchantLight = {
    playerId: -1,
    position: new THREE.Vector3(),
    intensity: 0,
  }
  private opacity = uniform(0)
  private phase = uniform(0)
  private armorFlow = uniform(0)
  private armorPulse = uniform(0)
  private geometry = new THREE.PlaneGeometry(1, 1)
  private glowMaterial = new MeshBasicNodeMaterial()
  private armorMaterial = new MeshBasicNodeMaterial()
  private rayMaterial = new MeshBasicNodeMaterial()
  private axisMaterial = new MeshBasicNodeMaterial()
  private bladeMaterial = new MeshBasicNodeMaterial()
  private rayGeometry = new THREE.PlaneGeometry(1, 1).translate(0, 0.5, 0)
  private rayOpacity = new THREE.InstancedBufferAttribute(
    new Float32Array(ENCHANT_MAX_RAYS),
    1
  )
  private rayProgress = new THREE.InstancedBufferAttribute(
    new Float32Array(ENCHANT_MAX_RAYS),
    1
  )
  private rays: THREE.InstancedMesh
  private axisGlow: THREE.Mesh
  private bladeGlow: THREE.Mesh
  private glowingWeaponId: string | null = null
  private bladeGeometry: THREE.BufferGeometry | null = null
  private core: THREE.Mesh
  private armorGlow: THREE.Mesh
  private dust: THREE.InstancedMesh
  private dummy = new THREE.Object3D()
  private cameraDirection = new THREE.Vector3()
  private rayDirection = new THREE.Vector3()
  private screenDirection = new THREE.Vector3()
  private rayRotation = new THREE.Matrix4()
  private rayNormal = new THREE.Vector3()
  private weaponCenter = new THREE.Vector3()
  private effectAxis = new THREE.Vector3()
  private radialAxis = new THREE.Vector3()
  private radialSide = new THREE.Vector3()

  constructor() {
    for (const material of [
      this.glowMaterial,
      this.armorMaterial,
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
    this.armorMaterial.colorNode = color('#f5e6bf')
    this.armorMaterial.opacityNode = radial
      .pow(1.8)
      .mul(this.opacity)
      .mul(this.armorPulse.smoothstep(0, 0.12))
      .mul(float(1).sub(this.armorPulse).pow(1.5))
    const spread = uv().y.pow(0.75).mul(0.35).add(0.05)
    const diffusion = exp(uv().x.sub(0.5).div(spread).pow(2).mul(-2))
    const filament = exp(uv().x.sub(0.5).div(0.035).pow(2).mul(-2))
    const outwardFade = exp(uv().y.mul(-1.6)).mul(float(1).sub(uv().y).pow(1.2))
    const goldShaft = exp(uv().x.sub(0.5).div(0.18).pow(2).mul(-2))
    const fineCore = exp(uv().x.sub(0.5).div(0.026).pow(2).mul(-2))
    const companions = exp(
      uv().x.sub(0.5).add(uv().y.mul(0.18)).div(0.02).pow(2).mul(-2)
    ).add(exp(uv().x.sub(0.5).sub(uv().y.mul(0.18)).div(0.016).pow(2).mul(-2)))
    const armorRay = goldShaft.mul(0.4).add(fineCore).add(companions.mul(0.3))
    const armorFade = exp(uv().y.mul(-0.65)).mul(
      float(1).sub(uv().y).smoothstep(0, 0.2)
    )
    const movingLight = exp(
      uv()
        .y.sub(attribute<'float'>('aEnchantRayProgress', 'float').mul(1.3))
        .add(0.1)
        .div(0.22)
        .pow(2)
        .mul(-2)
    )
      .mul(1.1)
      .add(0.38)
    this.rayMaterial.colorNode = mix(
      color('#fff7dc'),
      color('#e5bf78'),
      uv().y.smoothstep(0, 0.7)
    )
    this.rayMaterial.opacityNode = mix(
      diffusion.mul(0.24).add(filament.mul(0.8)).mul(outwardFade),
      armorRay.mul(armorFade),
      this.armorFlow
    )
      .mul(mix(float(1), movingLight, this.armorFlow))
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
    this.rayGeometry.setAttribute('aEnchantRayProgress', this.rayProgress)
    this.rayOpacity.setUsage(THREE.DynamicDrawUsage)
    this.rayProgress.setUsage(THREE.DynamicDrawUsage)
    this.rays = new THREE.InstancedMesh(
      this.rayGeometry,
      this.rayMaterial,
      ENCHANT_MAX_RAYS
    )
    this.rays.name = 'enchant-rays'
    this.rays.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.core = new THREE.Mesh(this.geometry, this.glowMaterial)
    this.armorGlow = new THREE.Mesh(this.geometry, this.armorMaterial)
    this.armorGlow.name = 'enchant-armor-glow'
    this.dust = new THREE.InstancedMesh(this.geometry, this.glowMaterial, 32)
    this.dust.name = 'enchant-motes'
    this.dust.frustumCulled = false
    this.dust.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.group.add(
      this.core,
      this.armorGlow,
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
    this.group.visible = fade > 0
    camera.getWorldDirection(this.cameraDirection)
    this.group.position
      .copy(anchor.position)
      .addScaledVector(this.cameraDirection, -0.5)
    this.light.position.copy(this.group.position)
    this.opacity.value = fade * (0.35 + p * 0.65)
    this.light.intensity =
      fade *
      p *
      (anchor.weapon ? ENCHANT_LIGHT_INTENSITY : ENCHANT_ARMOR_LIGHT_INTENSITY)
    this.armorGlow.visible = anchor.weapon === null
    this.axisGlow.visible = anchor.weapon !== null
    this.bladeGlow.visible = anchor.weapon !== null
    this.armorFlow.value = anchor.weapon === null ? 1 : 0
    if (anchor.weapon) {
      this.updateWeapon(anchor.weapon, elapsed, camera, reduced)
      return
    }
    this.updateArmor(elapsed, camera, reduced)
  }

  private updateArmor(elapsed: number, camera: THREE.Camera, reduced: boolean) {
    const gather = THREE.MathUtils.smoothstep(
      elapsed,
      0,
      ENCHANT_SUCCESS_GATHER_DURATION
    )
    const release = THREE.MathUtils.smoothstep(
      elapsed,
      ENCHANT_SUCCESS_RELEASE_START,
      ENCHANT_SUCCESS_DURATION
    )
    this.effectAxis.set(0, 1, 0).applyQuaternion(camera.quaternion)
    this.radialAxis.set(1, 0, 0).applyQuaternion(camera.quaternion)
    this.radialSide.crossVectors(this.effectAxis, this.radialAxis).normalize()
    const emissionTime = Math.max(0, elapsed - ENCHANT_SUCCESS_GATHER_DURATION)
    const pulse = (emissionTime / 1.2) % 1
    this.armorPulse.value = pulse
    this.core.position.set(0, 0, 0)
    this.core.quaternion.copy(camera.quaternion)
    this.core.scale.setScalar(
      0.15 + gather * 0.15 + (1 - pulse) * 0.08 + release * 0.2
    )
    this.armorGlow.position.copy(this.core.position)
    this.armorGlow.quaternion.copy(camera.quaternion)
    this.armorGlow.scale.set(0.2 + pulse * 1.7, 0.3 + pulse * 2.2, 1)
    this.updateRadiance(elapsed, camera, reduced, true, 1.5)
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
    this.effectAxis.copy(axis.direction).transformDirection(weapon.matrixWorld)
    const axisLength = this.dummy.position
      .copy(axis.center)
      .addScaledVector(axis.direction, axis.length)
      .applyMatrix4(weapon.matrixWorld)
      .distanceTo(this.weaponCenter)
    this.radialAxis.crossVectors(this.effectAxis, this.cameraDirection)
    if (this.radialAxis.lengthSq() < 0.001)
      this.radialAxis.set(1, 0, 0).applyQuaternion(camera.quaternion)
    this.radialAxis.normalize()
    this.radialSide.crossVectors(this.effectAxis, this.radialAxis).normalize()
    this.group.position
      .copy(this.weaponCenter)
      .addScaledVector(this.cameraDirection, -0.12)
    this.core.position.copy(this.effectAxis).multiplyScalar(axisLength * 0.12)
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
    this.rayNormal.crossVectors(this.radialAxis, this.effectAxis).normalize()
    this.rayRotation.makeBasis(this.radialAxis, this.effectAxis, this.rayNormal)
    this.axisGlow.quaternion.setFromRotationMatrix(this.rayRotation)
    this.axisGlow.scale.set(0.14, axisLength + 0.04, 1)
    this.updateRadiance(elapsed, camera, reduced, false, axisLength)
  }

  private updateRadiance(
    elapsed: number,
    camera: THREE.Camera,
    reduced: boolean,
    armor: boolean,
    radianceSize: number
  ) {
    const directions = armor ? ENCHANT_ARMOR_RAYS : ENCHANT_WEAPON_RAYS
    const count = reduced ? directions / 2 : directions
    const maxActive = count / 3
    const cycle = armor
      ? ENCHANT_ARMOR_RAY_CYCLE_SECONDS
      : ENCHANT_RAY_CYCLE_SECONDS
    let active = 0
    for (let i = 0; i < count; i++) {
      const stagger = ((i * 13) % count) / count
      const age = armor
        ? (elapsed - ENCHANT_SUCCESS_GATHER_DURATION) / cycle - stagger
        : elapsed / cycle + stagger
      if (age < 0) continue
      const phase = age % 1
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
        .addScaledVector(this.effectAxis, Math.sin(angle))
      this.screenDirection.crossVectors(this.rayDirection, this.cameraDirection)
      if (this.screenDirection.lengthSq() < 0.001)
        this.screenDirection.copy(this.effectAxis)
      this.screenDirection.normalize()
      this.rayNormal
        .crossVectors(this.screenDirection, this.rayDirection)
        .normalize()
      const length = armor
        ? (1 + (i % 5) * 0.125) *
          (0.03 + THREE.MathUtils.smoothstep(life, 0, 0.8) * 0.97)
        : radianceSize *
          (0.9 + (i % 5) * 0.18) *
          (0.65 + THREE.MathUtils.smoothstep(life, 0, 0.7) * 0.35)
      this.dummy.position.copy(this.core.position)
      this.rayRotation.makeBasis(
        this.screenDirection,
        this.rayDirection,
        this.rayNormal
      )
      this.dummy.quaternion.setFromRotationMatrix(this.rayRotation)
      this.dummy.scale.set(
        (0.3 + (i % 3) * 0.12) * (armor ? 0.65 : 1),
        length,
        1
      )
      this.dummy.updateMatrix()
      this.rays.setMatrixAt(active, this.dummy.matrix)
      this.rayOpacity.setX(active, pulse)
      this.rayProgress.setX(active, life)
      active++
    }
    this.rays.count = active
    this.rays.instanceMatrix.needsUpdate = true
    this.rayOpacity.needsUpdate = true
    this.rayProgress.needsUpdate = true
    this.dust.count = armor ? (reduced ? 8 : 32) : reduced ? 12 : 32
    for (let i = 0; i < this.dust.count; i++) {
      const age = armor
        ? (elapsed - ENCHANT_SUCCESS_GATHER_DURATION) / 1.2 -
          i / this.dust.count
        : elapsed * 0.3 + i * 0.61803398875
      const life = Math.max(0, age) % 1
      const angle = i * 2.399963229728653
      const radius =
        radianceSize * (armor ? 0.02 + life * 0.95 : 0.15 + life * 1.2)
      this.dummy.position
        .copy(this.core.position)
        .addScaledVector(this.radialAxis, Math.cos(angle) * radius)
        .addScaledVector(this.effectAxis, Math.sin(angle) * radius)
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
    this.armorMaterial.dispose()
    this.rayMaterial.dispose()
    this.axisMaterial.dispose()
    this.bladeMaterial.dispose()
    this.bladeGeometry?.dispose()
    this.glowingWeaponId = null
    this.group.clear()
    this.group.removeFromParent()
  }
}
