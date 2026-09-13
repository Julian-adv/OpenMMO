import * as THREE from 'three'

export const MARK_LOCK = 0.42
export const MARK_HOLD = 5
export const MARK_FADE = 0.4
export const MARK_END = MARK_LOCK + MARK_HOLD
export const MARK_PREVIEW_DURATION = MARK_END + MARK_FADE + 0.8

function softGlow() {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 128
  const context = canvas.getContext('2d')!
  const gradient = context.createRadialGradient(64, 64, 0, 64, 64, 64)
  gradient.addColorStop(0, '#ffffff55')
  gradient.addColorStop(0.35, '#ffffff22')
  gradient.addColorStop(1, '#ffffff00')
  context.fillStyle = gradient
  context.fillRect(0, 0, 128, 128)
  return new THREE.CanvasTexture(canvas)
}

export class BowMarkEffect {
  readonly group = new THREE.Group()
  private readonly face = new THREE.Group()
  private readonly texture = softGlow()
  private readonly material = new THREE.MeshBasicMaterial({
    color: '#f5dfa8',
    transparent: true,
    depthWrite: false,
    side: THREE.DoubleSide,
    toneMapped: false,
  })
  private readonly centerMaterial = this.material.clone()
  private readonly center = new THREE.Mesh(
    new THREE.PlaneGeometry(0.065, 0.065),
    this.centerMaterial
  )
  private readonly glow = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: this.texture,
      color: '#f5dfa8',
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      toneMapped: false,
    })
  )
  private readonly pieces = Array.from({ length: 4 }, (_, index) => {
    const piece = new THREE.Group()
    const arc = new THREE.Mesh(
      new THREE.RingGeometry(0.285, 0.315, 28, 1, -0.55, 1.1),
      this.material
    )
    const tick = new THREE.Mesh(
      new THREE.PlaneGeometry(0.19, 0.038),
      this.material
    )
    tick.position.x = 0.36
    const tip = new THREE.Mesh(
      new THREE.PlaneGeometry(0.048, 0.048),
      this.material
    )
    tip.position.x = 0.48
    tip.rotation.z = Math.PI / 4
    piece.add(arc, tick, tip)
    piece.rotation.z = (index * Math.PI) / 2
    return piece
  })

  constructor() {
    this.center.rotation.z = Math.PI / 4
    this.glow.scale.setScalar(1.25)
    this.face.add(...this.pieces, this.center, this.glow)
    this.group.add(this.face)
    this.group.visible = false
  }

  setColor(color: string) {
    this.material.color.set(color)
    this.centerMaterial.color.set(color)
    this.glow.material.color.set(color)
  }

  update(
    time: number,
    anchor: THREE.Vector3,
    camera: THREE.Camera,
    size: number,
    endAt = MARK_END
  ) {
    const enter = THREE.MathUtils.smoothstep(time, 0, MARK_LOCK)
    const fade = 1 - THREE.MathUtils.smoothstep(time, endAt, endAt + MARK_FADE)
    const alpha = THREE.MathUtils.smoothstep(time, 0, 0.12) * fade
    this.group.visible = time >= 0 && alpha > 0
    this.group.position.copy(anchor)
    this.group.position.y += Math.sin(time * 1.8) * 0.025 * enter
    this.group.quaternion.copy(camera.quaternion)
    const beat = Math.exp(-Math.pow((time - MARK_LOCK) / 0.075, 2))
    const pulse =
      Math.sin(((time - MARK_LOCK) * Math.PI * 2) / 1.2) *
      THREE.MathUtils.smoothstep(time, MARK_LOCK, MARK_LOCK + 0.25)
    this.face.scale.setScalar(size * (1 + beat * 0.075 + pulse * 0.1))
    this.face.rotation.z = (1 - enter) * 0.16
    this.pieces.forEach((piece, index) => {
      const angle = (index * Math.PI) / 2
      const offset = 0.19 * (1 - enter)
      piece.position.set(Math.cos(angle) * offset, Math.sin(angle) * offset, 0)
    })
    this.material.opacity = alpha * (0.83 + beat * 0.17)
    this.centerMaterial.opacity =
      alpha * (0.78 + Math.sin(time * 2) * 0.08 + beat * 0.14)
    this.center.scale.setScalar(1 + beat * 0.5)
    this.glow.material.opacity = alpha * (0.16 + beat * 0.4)
  }

  dispose() {
    this.face.traverse((object) => {
      if (object instanceof THREE.Mesh) object.geometry.dispose()
    })
    this.material.dispose()
    this.centerMaterial.dispose()
    this.glow.material.dispose()
    this.texture.dispose()
  }
}
