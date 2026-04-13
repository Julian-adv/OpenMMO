import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { attribute, texture } from 'three/tsl'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

const PARTICLE_OPACITY_ATTR = 'aFireOpacity'
const MAX_PARTICLES = 32
const SPAWN_RATE = 24 // particles per second
const PARTICLE_SIZE = 0.12

interface FireParticle {
  alive: boolean
  age: number
  maxAge: number
  x: number
  y: number
  z: number
  vx: number
  vy: number
  vz: number
  baseScale: number
  phase: number
}

function createPool(): FireParticle[] {
  return Array.from({ length: MAX_PARTICLES }, () => ({
    alive: false,
    age: 0,
    maxAge: 0,
    x: 0,
    y: 0,
    z: 0,
    vx: 0,
    vy: 0,
    vz: 0,
    baseScale: 0,
    phase: 0,
  }))
}

/** Generate a soft radial gradient texture for fire particles. */
function createFireTexture(): THREE.Texture {
  const size = 64
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')!

  const gradient = ctx.createRadialGradient(
    size / 2,
    size / 2,
    0,
    size / 2,
    size / 2,
    size / 2
  )
  gradient.addColorStop(0, 'rgba(255, 255, 255, 1)')
  gradient.addColorStop(0.3, 'rgba(255, 200, 80, 0.8)')
  gradient.addColorStop(0.6, 'rgba(255, 100, 20, 0.4)')
  gradient.addColorStop(1, 'rgba(255, 40, 0, 0)')

  ctx.fillStyle = gradient
  ctx.fillRect(0, 0, size, size)

  const tex = new THREE.CanvasTexture(canvas)
  tex.needsUpdate = true
  return tex
}

function createFireMaterial(fireTexture: THREE.Texture): MeshBasicNodeMaterial {
  const mat = new MeshBasicNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.transparent = true
  mat.depthWrite = false
  mat.blending = THREE.AdditiveBlending

  const texNode: N = texture(fireTexture)
  const opacity: N = attribute(PARTICLE_OPACITY_ATTR, 'float')
  mat.colorNode = texNode.rgb
  mat.opacityNode = texNode.a.mul(opacity)

  return mat
}

/**
 * Fire particle system that attaches to a torch object.
 * Call update() each frame with deltaTime (seconds) and camera.
 * The group lives in world space — position is synced from the torch tip bone.
 */
export class TorchFireParticles {
  readonly group = new THREE.Group()
  private pool = createPool()
  private mesh: THREE.InstancedMesh
  private fireTex: THREE.Texture
  private opacityAttr: THREE.InstancedBufferAttribute
  private spawnAccumulator = 0
  private readonly tmpMatrix = new THREE.Matrix4()
  private readonly tmpPos = new THREE.Vector3()
  private readonly tmpScale = new THREE.Vector3()
  private readonly zeroMatrix = new THREE.Matrix4().makeScale(0, 0, 0)
  /** World-space position of the torch tip, updated externally. */
  private readonly tipWorld = new THREE.Vector3()

  constructor() {
    this.fireTex = createFireTexture()
    const geom = new THREE.PlaneGeometry(PARTICLE_SIZE, PARTICLE_SIZE)
    const opacityArray = new Float32Array(MAX_PARTICLES)
    this.opacityAttr = new THREE.InstancedBufferAttribute(opacityArray, 1)
    geom.setAttribute(PARTICLE_OPACITY_ATTR, this.opacityAttr)

    this.mesh = new THREE.InstancedMesh(
      geom,
      createFireMaterial(this.fireTex),
      MAX_PARTICLES
    )
    this.mesh.frustumCulled = false
    this.mesh.castShadow = false
    this.mesh.receiveShadow = false

    for (let i = 0; i < MAX_PARTICLES; i++) {
      this.mesh.setMatrixAt(i, this.zeroMatrix)
    }

    this.group.add(this.mesh)
  }

  /** Set the world-space torch tip position (call before update). */
  setTipPosition(worldPos: THREE.Vector3) {
    this.tipWorld.copy(worldPos)
  }

  update(deltaTime: number, camera: THREE.Camera | undefined) {
    if (!camera) return

    const dt = Math.min(deltaTime, 0.1)
    const tx = this.tipWorld.x
    const ty = this.tipWorld.y
    const tz = this.tipWorld.z

    // Spawn new particles
    this.spawnAccumulator += dt
    const spawnInterval = 1 / SPAWN_RATE
    while (this.spawnAccumulator >= spawnInterval) {
      this.spawnAccumulator -= spawnInterval
      this.spawn(tx, ty, tz)
    }

    // Update existing particles
    const opacityArr = this.opacityAttr.array as Float32Array
    const camQuat = camera.quaternion

    for (let i = 0; i < this.pool.length; i++) {
      const p = this.pool[i]
      if (!p.alive) continue

      p.age += dt
      if (p.age >= p.maxAge) {
        p.alive = false
        this.mesh.setMatrixAt(i, this.zeroMatrix)
        opacityArr[i] = 0
        continue
      }

      const t = p.age / p.maxAge

      // Accelerate upward, add turbulence
      p.vy += 0.8 * dt
      p.vx += Math.sin(p.age * 12 + p.phase) * 0.3 * dt
      p.vz += Math.cos(p.age * 10 + p.phase * 1.3) * 0.3 * dt

      p.x += p.vx * dt
      p.y += p.vy * dt
      p.z += p.vz * dt

      // Opacity: quick fade in, hold, fade out
      let opacity: number
      if (t < 0.1) opacity = t / 0.1
      else if (t > 0.4) opacity = 1.0 - (t - 0.4) / 0.6
      else opacity = 1.0

      opacityArr[i] = opacity

      // Scale shrinks over lifetime
      const scale = p.baseScale * (1.0 - t * 0.6)
      this.tmpPos.set(p.x, p.y, p.z)
      this.tmpScale.set(scale, scale, scale)
      this.tmpMatrix.compose(this.tmpPos, camQuat, this.tmpScale)
      this.mesh.setMatrixAt(i, this.tmpMatrix)
    }

    this.mesh.instanceMatrix.needsUpdate = true
    this.opacityAttr.needsUpdate = true
  }

  private spawn(cx: number, cy: number, cz: number) {
    const slot = this.pool.findIndex((p) => !p.alive)
    if (slot === -1) return

    const p = this.pool[slot]
    // Spawn with slight random offset around torch tip
    const spread = 0.02
    p.x = cx + (Math.random() - 0.5) * spread
    p.y = cy + (Math.random() - 0.5) * spread
    p.z = cz + (Math.random() - 0.5) * spread

    p.vx = (Math.random() - 0.5) * 0.1
    p.vy = 0.2 + Math.random() * 0.15
    p.vz = (Math.random() - 0.5) * 0.1

    p.maxAge = 0.3 + Math.random() * 0.4
    p.baseScale = 0.8 + Math.random() * 0.5
    p.phase = Math.random() * Math.PI * 2
    p.age = 0
    p.alive = true
  }

  dispose() {
    this.mesh.geometry.dispose()
    if (this.mesh.material instanceof THREE.Material) {
      this.mesh.material.dispose()
    }
    this.fireTex.dispose()
    if (this.group.parent) {
      this.group.parent.remove(this.group)
    }
  }
}
