import * as THREE from 'three'

const clamp = (value: number) => THREE.MathUtils.clamp(value, 0, 1)
const smooth = (value: number) => {
  const t = clamp(value)
  return t * t * (3 - 2 * t)
}
const shieldPop = (age: number) => {
  if (age < 0.04)
    return THREE.MathUtils.lerp(0.3, 1.46, 1 - (1 - clamp(age / 0.04)) ** 3)
  if (age < 0.1)
    return THREE.MathUtils.lerp(1.46, 0.95, smooth((age - 0.04) / 0.06))
  if (age < 0.15)
    return THREE.MathUtils.lerp(0.95, 1.17, smooth((age - 0.1) / 0.05))
  return THREE.MathUtils.lerp(1.17, 1, smooth((age - 0.15) / 0.12))
}

export const GUARD_EFFECT_DURATION = 1

export class SwordGuardEffect {
  readonly group = new THREE.Group()
  private readonly materials: THREE.MeshBasicMaterial[] = []
  private readonly glows: THREE.SpriteMaterial[] = []
  private readonly gather: THREE.Mesh[]
  private readonly handGlow: THREE.Sprite
  private readonly handHalo: THREE.Mesh
  private readonly targets: ReturnType<SwordGuardEffect['createTarget']>[]
  private readonly glowTexture: THREE.CanvasTexture
  private readonly viewOffset = new THREE.Vector3()
  private readonly cameraPosition = new THREE.Vector3()
  private readonly baseColor = new THREE.Color()
  private readonly flashColor = new THREE.Color('#fffdf2')

  constructor(positions: THREE.Vector3[]) {
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = 64
    const context = canvas.getContext('2d')!
    const gradient = context.createRadialGradient(32, 32, 0, 32, 32, 32)
    gradient.addColorStop(0, 'rgba(255,255,255,1)')
    gradient.addColorStop(0.15, 'rgba(255,255,255,0.6)')
    gradient.addColorStop(0.45, 'rgba(255,255,255,0.13)')
    gradient.addColorStop(1, 'rgba(255,255,255,0)')
    context.fillStyle = gradient
    context.fillRect(0, 0, 64, 64)
    this.glowTexture = new THREE.CanvasTexture(canvas)
    this.gather = [0.009, 0.025].map((width) => this.ring(width))
    this.group.add(...this.gather)
    this.handGlow = this.glow(0.8)
    this.handHalo = this.ring(0.028)
    this.handHalo.rotation.set(0, 0, 0)
    this.group.add(this.handGlow, this.handHalo)
    this.targets = positions.map((position) => this.createTarget(position))
    this.setPalette('gold')
  }

  private material(opacity = 0) {
    const material = new THREE.MeshBasicMaterial({
      transparent: true,
      opacity,
      depthWrite: false,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      toneMapped: false,
    })
    this.materials.push(material)
    return material
  }

  private ring(width: number, arc = Math.PI * 2) {
    const mesh = new THREE.Mesh(
      new THREE.RingGeometry(1 - width, 1, 112, 1, 0, arc),
      this.material()
    )
    mesh.rotation.x = -Math.PI / 2
    mesh.position.y = 0.035
    return mesh
  }

  private glow(size: number) {
    const material = new THREE.SpriteMaterial({
      map: this.glowTexture,
      transparent: true,
      opacity: 0,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      toneMapped: false,
    })
    this.glows.push(material)
    const sprite = new THREE.Sprite(material)
    sprite.scale.setScalar(size)
    return sprite
  }

  private shield() {
    const points = [
      [-0.39, 0.28],
      [0, 0.43],
      [0.39, 0.28],
      [0.35, -0.14],
      [0.22, -0.36],
      [0, -0.56],
      [-0.22, -0.36],
      [-0.35, -0.14],
      [-0.39, 0.28],
    ]
    const group = new THREE.Group()
    const curve = new THREE.CatmullRomCurve3(
      points.map(([x, y]) => new THREE.Vector3(x, y, 0)),
      false,
      'centripetal'
    )
    const outline = new THREE.Mesh(
      new THREE.TubeGeometry(curve, 64, 0.013, 5, false),
      this.material()
    )
    const glow = new THREE.Mesh(
      new THREE.TubeGeometry(curve, 64, 0.043, 5, false),
      this.material()
    )
    const inner = new THREE.Mesh(outline.geometry, this.material())
    inner.scale.setScalar(0.81)
    const echo = new THREE.Mesh(outline.geometry, this.material())
    const plate = new THREE.Mesh(
      new THREE.ShapeGeometry(
        new THREE.Shape(
          curve
            .getPoints(64)
            .map((point) => new THREE.Vector2(point.x, point.y))
        )
      ),
      this.material()
    )
    plate.position.z = -0.005
    const stem = new THREE.Mesh(
      new THREE.PlaneGeometry(0.025, 0.43),
      this.material()
    )
    const cross = new THREE.Mesh(
      new THREE.PlaneGeometry(0.3, 0.025),
      this.material()
    )
    cross.position.y = 0.075
    group.add(plate, echo, glow, outline, inner, stem, cross)
    return { group, outline, glow, inner, stem, cross, echo, plate }
  }

  private createTarget(position: THREE.Vector3) {
    const group = new THREE.Group()
    group.position.copy(position)
    const crest = this.shield()
    const halo = this.glow(1.65)
    const arcs = [
      this.ring(0.02, Math.PI * 0.85),
      this.ring(0.01, Math.PI * 0.65),
    ]
    const rise = this.ring(0.03)
    const motes = Array.from({ length: 12 }, () => this.glow(0.095))
    group.add(crest.group, halo, ...arcs, rise, ...motes)
    this.group.add(group)
    return {
      group,
      crest,
      halo,
      arcs,
      rise,
      motes,
      distance: Math.hypot(position.x, position.z),
      enabled: true,
    }
  }

  setTarget(
    index: number,
    position: THREE.Vector3 | null,
    resetDistance = false
  ) {
    const target = this.targets[index]
    if (!target) return
    target.enabled = position !== null
    if (position) {
      target.group.position.copy(position)
      if (resetDistance)
        target.distance = Math.min(20, Math.hypot(position.x, position.z))
    }
  }

  setPalette(palette: string) {
    const color = palette === 'silver' ? '#d5e6f4' : '#f2dfa9'
    this.baseColor.set(color)
    this.materials.forEach((material) => material.color.set(color))
    this.glows.forEach((material) => material.color.set(color))
  }

  update(
    time: number,
    radius: number,
    camera: THREE.Camera,
    shieldPosition: THREE.Vector3 | null
  ) {
    camera.getWorldPosition(this.cameraPosition)
    this.group.worldToLocal(this.cameraPosition)
    const charge = (shieldPosition ? 1 : 0) * (1 - smooth(time / 0.24))
    if (shieldPosition) this.handGlow.position.copy(shieldPosition)
    this.handGlow.material.opacity = charge * 0.25
    this.handGlow.scale.setScalar(0.6 + charge * 0.4)
    if (shieldPosition) this.handHalo.position.copy(shieldPosition)
    this.handHalo.quaternion.copy(camera.quaternion)
    this.handHalo.scale.setScalar(0.25 + (1 - charge) * 0.18)
    this.opacity(this.handHalo, charge * 0.45)
    this.gather.forEach((ring, i) => {
      ring.scale.setScalar(0.7 + (1 - smooth(time / 0.18)) * 0.35 + i * 0.13)
      this.opacity(ring, charge * (i === 0 ? 0.65 : 0.11))
    })
    const endFade = 1 - smooth((time - 0.35) / (GUARD_EFFECT_DURATION - 0.35))
    for (const target of this.targets) {
      const age = time
      target.group.visible =
        target.enabled && target.distance <= radius && age >= 0 && endFade > 0
      if (!target.group.visible) continue
      const appear = 1 - (1 - clamp(age / 0.02)) ** 3
      const firstFlash = appear * (1 - smooth((age - 0.02) / 0.08))
      const secondFlash =
        smooth((age - 0.1) / 0.04) * (1 - smooth((age - 0.15) / 0.12))
      const flash = firstFlash + secondFlash * 0.55
      const burst = appear * (1 - smooth((age - 0.18) / 0.32))
      const crest = target.crest
      this.viewOffset
        .copy(this.cameraPosition)
        .sub(target.group.position)
        .normalize()
        .multiplyScalar(0.65)
      crest.group.position.copy(this.viewOffset)
      crest.group.position.y += 1.35
      crest.group.scale.setScalar(0.9 * shieldPop(age))
      crest.group.quaternion.copy(camera.quaternion)
      const strength = appear * endFade * 0.75
      crest.outline.material.color
        .copy(this.baseColor)
        .lerp(this.flashColor, flash)
      this.opacity(crest.outline, strength + flash * 0.4)
      this.opacity(crest.inner, strength * 0.25 + flash * 0.4)
      this.opacity(crest.glow, strength * 0.035 + flash * 0.25)
      this.opacity(crest.stem, strength * 0.8)
      this.opacity(crest.cross, strength * 0.8)
      this.opacity(crest.plate, flash * 0.24)
      crest.echo.scale.setScalar(1 + smooth(age / 0.24) * 0.65)
      this.opacity(crest.echo, appear * (1 - smooth(age / 0.24)) * 0.16)
      target.halo.position.copy(crest.group.position)
      target.halo.scale.setScalar(1.2 + flash * 0.65)
      target.halo.material.opacity =
        burst * 0.035 + strength * 0.025 + flash * 0.7
      target.rise.position.y = 0.08 + clamp(age / 0.4) * 1.1
      target.rise.scale.setScalar(0.58 + clamp(age) * 0.12)
      this.opacity(target.rise, burst * 0.27)
      target.arcs.forEach((arc, i) => {
        arc.scale.setScalar(0.57 + i * 0.1)
        arc.rotation.z = time * (i === 0 ? 0.45 : -0.33) + i * Math.PI
        this.opacity(arc, appear * endFade * (0.15 + burst * 0.25))
      })
      target.motes.forEach((mote, i) => {
        const travel = age * 0.65 + i / target.motes.length
        const phase = travel % 1
        const angle = i * 2.399 + age * 0.2
        mote.position.set(
          Math.cos(angle) * 0.56,
          0.15 + phase * 1.8,
          Math.sin(angle) * 0.56
        )
        mote.material.opacity =
          Math.sin(phase * Math.PI) * appear * endFade * (burst * 0.55 + 0.12)
      })
    }
  }

  private opacity(mesh: THREE.Mesh, value: number) {
    ;(mesh.material as THREE.MeshBasicMaterial).opacity = Math.max(0, value)
    mesh.visible = value > 0.001
  }

  dispose() {
    const geometries = new Set<THREE.BufferGeometry>()
    this.group.traverse((object) => {
      if (object instanceof THREE.Mesh) geometries.add(object.geometry)
    })
    geometries.forEach((geometry) => geometry.dispose())
    this.materials.forEach((material) => material.dispose())
    this.glows.forEach((material) => material.dispose())
    this.glowTexture.dispose()
    this.group.removeFromParent()
  }
}
