import * as THREE from 'three'
import {
  TORCH_BASE_INTENSITY,
  TORCH_BASE_DISTANCE,
  TORCH_BASE_DECAY,
  TORCH_BASE_POSITION,
  TORCH_SHADOW_FAR,
  TORCH_SHADOW_MAP_SIZE,
  TORCH_SHADOW_BIAS,
} from '../utils/torchFlicker'

export const LIGHT_WAKE = 0.8
export const LIGHT_COOLDOWN = LIGHT_WAKE
export const LIGHT_FADE = 1.2
export const LIGHT_DURATION = 120

function glowTexture() {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 128
  const context = canvas.getContext('2d')!
  const gradient = context.createRadialGradient(64, 64, 0, 64, 64, 64)
  gradient.addColorStop(0, '#fff')
  gradient.addColorStop(0.1, '#fffef2ed')
  gradient.addColorStop(0.3, '#fff4c45c')
  gradient.addColorStop(0.6, '#ffedb91a')
  gradient.addColorStop(1, '#ffedb900')
  context.fillStyle = gradient
  context.fillRect(0, 0, 128, 128)
  return new THREE.CanvasTexture(canvas)
}

function placeCastOrb(position: THREE.Vector3, time: number) {
  const orbit = THREE.MathUtils.smoothstep(time, 0, 0.62)
  const angle = orbit * Math.PI * 2
  const radius = 0.64
  position.set(
    Math.sin(angle) * radius,
    1.1 + orbit * 0.22 + Math.sin(angle) * 0.08,
    Math.cos(angle) * radius
  )
}

export class RadianceEffect {
  readonly group = new THREE.Group()
  private readonly casting = new THREE.Group()
  readonly light = new THREE.PointLight(
    '#fff2cd',
    0,
    TORCH_BASE_DISTANCE,
    TORCH_BASE_DECAY
  )
  private readonly texture = glowTexture()
  private readonly glow = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: this.texture,
      color: '#fff2cd',
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      toneMapped: false,
    })
  )
  private readonly core = new THREE.Mesh(
    new THREE.SphereGeometry(0.085, 16, 12),
    new THREE.MeshBasicMaterial({
      color: '#fffef6',
      transparent: true,
      toneMapped: false,
    })
  )
  private readonly motes = Array.from({ length: 16 }, () => {
    const mote = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: this.texture,
        color: '#fff0c2',
        transparent: true,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        toneMapped: false,
      })
    )
    return mote
  })

  constructor(lighting = true) {
    this.light.castShadow = true
    this.light.shadow.mapSize.set(TORCH_SHADOW_MAP_SIZE, TORCH_SHADOW_MAP_SIZE)
    this.light.shadow.camera.near = 0.2
    this.light.shadow.camera.far = TORCH_SHADOW_FAR
    this.light.shadow.bias = TORCH_SHADOW_BIAS
    this.light.shadow.normalBias = 0.04
    this.light.position.y = TORCH_BASE_POSITION.y
    this.casting.add(this.core, this.glow, ...this.motes)
    this.group.add(this.casting)
    if (lighting) this.group.add(this.light)
  }

  setColor(value: string) {
    const color =
      value === 'gold' ? '#ffe0a0' : value === 'silver' ? '#dfedff' : '#fff2cd'
    this.light.color.set(color)
    this.glow.material.color.set(color)
    this.motes.forEach((mote) => mote.material.color.set(color))
  }

  update(
    time: number,
    power: number,
    visible: boolean,
    switchedOffAt: number | null
  ) {
    const sample = switchedOffAt === null ? time : Math.min(time, switchedOffAt)
    const rise = THREE.MathUtils.smoothstep(sample, 0, LIGHT_WAKE)
    const fade =
      1 -
      THREE.MathUtils.smoothstep(
        sample,
        LIGHT_DURATION - LIGHT_FADE,
        LIGHT_DURATION
      )
    const toggleFade =
      switchedOffAt === null
        ? 1
        : 1 -
          THREE.MathUtils.smoothstep(
            time,
            switchedOffAt,
            switchedOffAt + LIGHT_FADE
          )
    const strength = rise * fade * toggleFade
    const pop = 1 + 0.18 * Math.exp(-Math.pow((sample - 0.45) / 0.12, 2))
    const shimmer = 1 + Math.sin(time * 2.3) * 0.014
    this.group.visible = visible && strength > 0
    this.light.intensity =
      TORCH_BASE_INTENSITY * power * strength * pop * shimmer
    const appear = THREE.MathUtils.smoothstep(sample, 0, 0.08)
    const orbStrength =
      appear *
      (1 - THREE.MathUtils.smoothstep(sample, 0.62, LIGHT_WAKE)) *
      toggleFade
    this.casting.visible = sample < LIGHT_WAKE && orbStrength > 0
    if (!this.casting.visible) return
    placeCastOrb(this.core.position, sample)
    this.glow.position.copy(this.core.position)
    this.glow.scale.setScalar(Math.sqrt(appear))
    this.glow.material.opacity = 0.75 * orbStrength
    this.core.scale.setScalar(0.9 * Math.sqrt(appear))
    this.core.material.opacity = orbStrength
    const trailStrength =
      orbStrength * THREE.MathUtils.smoothstep(sample, 0.06, 0.2)
    this.motes.forEach((mote, index) => {
      const portion = 1 - index / this.motes.length
      placeCastOrb(mote.position, Math.max(0, sample - (index + 1) * 0.008))
      mote.scale.setScalar(0.04 + portion * 0.05)
      mote.material.opacity = trailStrength * portion * 0.45
    })
  }

  dispose() {
    this.texture.dispose()
    this.light.shadow.dispose()
    this.light.dispose()
    this.core.geometry.dispose()
    this.core.material.dispose()
    this.glow.material.dispose()
    this.motes.forEach((mote) => mote.material.dispose())
  }
}
