import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { color, float, sin, texture, uniform, uv, vec2 } from 'three/tsl'

export class EnchantSuccessEffect {
  readonly group = new THREE.Group()
  readonly light = new THREE.PointLight('#f4e6c9', 0, 3.5, 2)
  private opacity = uniform(0)
  private phase = uniform(0)
  private dissolve = uniform(-0.2)
  private geometry = new THREE.PlaneGeometry(1, 1)
  private glowMaterial = new MeshBasicNodeMaterial()
  private threadMaterial = new MeshBasicNodeMaterial()
  private core: THREE.Mesh
  private threads: THREE.Mesh[]
  private dust: THREE.InstancedMesh
  private dummy = new THREE.Object3D()

  constructor(map: THREE.Texture) {
    for (const material of [this.glowMaterial, this.threadMaterial]) {
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
    this.core = new THREE.Mesh(this.geometry, this.glowMaterial)
    this.threads = Array.from(
      { length: 3 },
      () => new THREE.Mesh(this.geometry, this.threadMaterial)
    )
    this.dust = new THREE.InstancedMesh(this.geometry, this.glowMaterial, 20)
    this.dust.frustumCulled = false
    this.dust.instanceMatrix.setUsage(THREE.DynamicDrawUsage)
    this.group.add(this.core, ...this.threads, this.dust)
    this.group.traverse((object) => {
      object.frustumCulled = false
    })
    this.clear()
  }

  update(
    progress: number,
    release: number | null,
    hand: THREE.Vector3,
    camera: THREE.Camera,
    reduced = false
  ) {
    const p = THREE.MathUtils.clamp(progress, 0, 1)
    const tail = THREE.MathUtils.clamp((release ?? 0) / 0.6, 0, 1)
    const easedTail = tail * tail * (3 - 2 * tail)
    const fade =
      release === null ? THREE.MathUtils.smoothstep(p, 0, 0.2) : 1 - easedTail
    this.phase.value = p + tail * 0.7
    this.dissolve.value = -0.2 + easedTail * 1.4
    this.group.visible = fade > 0
    this.group.position.copy(hand)
    this.light.position.copy(hand)
    this.opacity.value = fade * (0.35 + p * 0.65)
    this.core.quaternion.copy(camera.quaternion)
    this.core.scale.setScalar(
      release === null ? 0.18 + p * 0.38 : 0.56 + easedTail * 0.65
    )
    this.light.intensity = fade * (release === null ? p * 3 : 3)
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

  clear() {
    this.group.visible = false
    this.light.intensity = 0
  }

  dispose() {
    this.clear()
    this.dust.dispose()
    this.geometry.dispose()
    this.glowMaterial.dispose()
    this.threadMaterial.dispose()
    this.group.clear()
    this.group.removeFromParent()
    this.light.removeFromParent()
  }
}
