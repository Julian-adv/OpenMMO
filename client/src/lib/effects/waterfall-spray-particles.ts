import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { attribute, mix, texture, uniform, uv, vec3 } from 'three/tsl'
import { getCloudTexture } from '../shaders/water-types'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

const SPRAY_OPACITY_ATTR = 'aSprayOpacity'
const SPRAY_CLOUD_UV_ATTR = 'aSprayCloudUv'
const MAX_PARTICLES = 4400
const PARTICLE_SIZE = 0.48
const SPRAY_CLUSTER_MIN = 4
const SPRAY_CLUSTER_MAX = 5

export interface WaterfallSprayEmitter {
  id: string
  x: number
  y: number
  z: number
  dirX: number
  dirZ: number
  radius: number
  intensity: number
  launchSlope: number
}

interface SprayCluster {
  across: number
  along: number
  radius: number
  weight: number
}

interface SprayClusterSet {
  clusters: SprayCluster[]
  totalWeight: number
}

interface SprayParticle {
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

function createPool(): SprayParticle[] {
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

function createSprayTexture(): THREE.Texture {
  const size = 96
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')!
  const image = ctx.createImageData(size, size)
  const data = image.data
  const cx = size / 2
  const cy = size / 2

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = (x + 0.5 - cx) / cx
      const dy = (y + 0.5 - cy) / cy
      const r = Math.hypot(dx, dy)
      const wobble =
        0.08 * Math.sin(x * 0.52 + y * 0.31) +
        0.05 * Math.sin(x * 0.17 - y * 0.47)
      const edge = 0.72 + wobble
      const core = 1 - THREE.MathUtils.smoothstep(r, 0.2, edge)
      const rim =
        THREE.MathUtils.smoothstep(r, edge - 0.08, edge) *
        (1 - THREE.MathUtils.smoothstep(r, edge, edge + 0.06))
      const alpha = Math.max(0, Math.min(1, core * 0.56 + rim * 0.34))
      const shade = 0.75 + 0.25 * (1 - r)
      const i = (y * size + x) * 4
      data[i] = Math.round(92 * shade)
      data[i + 1] = Math.round(145 * shade)
      data[i + 2] = Math.round(176 * shade)
      data[i + 3] = Math.round(alpha * 255)
    }
  }

  ctx.putImageData(image, 0, 0)

  const tex = new THREE.CanvasTexture(canvas)
  tex.needsUpdate = true
  return tex
}

interface SprayUniforms {
  lightFactor: { value: number }
  dayWhiteness: { value: number }
  cloudReflectionMix: { value: number }
}

function createSprayMaterial(sprayTexture: THREE.Texture): {
  material: MeshBasicNodeMaterial
  uniforms: SprayUniforms
} {
  const mat = new MeshBasicNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.transparent = true
  mat.depthWrite = false
  mat.alphaTest = 0.03
  mat.blending = THREE.NormalBlending

  const texNode: N = texture(sprayTexture)
  const cloudTexNode: N = texture(getCloudTexture())
  const opacity: N = attribute(SPRAY_OPACITY_ATTR, 'float')
  const cloudOffset: N = attribute(SPRAY_CLOUD_UV_ATTR, 'vec2')
  const lightFactor: N = uniform(1)
  const dayWhiteness: N = uniform(0)
  const cloudReflectionMix: N = uniform(0.8)
  const uniforms: SprayUniforms = { lightFactor, dayWhiteness, cloudReflectionMix }
  mat.userData.uniforms = uniforms
  const cloudUV: N = uv().mul(0.34).add(cloudOffset)
  const sprayBase: N = texNode.rgb.mul(lightFactor)
  const cloudReflection: N = cloudTexNode.sample(cloudUV).rgb
  const cloudContrast: N = cloudReflection.mul(cloudReflection).mul(1.65)
  const reflectedColor: N = mix(
    sprayBase,
    cloudContrast.mul(lightFactor),
    cloudReflectionMix
  )
  mat.colorNode = mix(
    reflectedColor,
    vec3(0.92, 0.97, 1.0).mul(lightFactor),
    dayWhiteness.mul(0.55)
  )
  mat.opacityNode = texNode.a.mul(opacity)

  return { material: mat, uniforms }
}

export class WaterfallSprayParticles {
  readonly group = new THREE.Group()
  private emitters: WaterfallSprayEmitter[] = []
  private pool = createPool()
  private freeSlots: number[] = []
  private liveCount = 0
  private clusterCache = new WeakMap<WaterfallSprayEmitter, SprayClusterSet>()
  private mesh: THREE.InstancedMesh
  private sprayTex: THREE.Texture
  private opacityAttr: THREE.InstancedBufferAttribute
  private cloudUvAttr: THREE.InstancedBufferAttribute
  private uniforms: SprayUniforms
  private spawnAccumulator = 0
  private totalIntensity = 0
  private readonly tmpMatrix = new THREE.Matrix4()
  private readonly tmpPos = new THREE.Vector3()
  private readonly tmpScale = new THREE.Vector3()
  private readonly zeroMatrix = new THREE.Matrix4().makeScale(0, 0, 0)

  constructor() {
    this.group.name = 'waterfall-spray'
    this.sprayTex = createSprayTexture()

    const geom = new THREE.PlaneGeometry(PARTICLE_SIZE, PARTICLE_SIZE)
    this.opacityAttr = new THREE.InstancedBufferAttribute(
      new Float32Array(MAX_PARTICLES),
      1
    )
    this.cloudUvAttr = new THREE.InstancedBufferAttribute(
      new Float32Array(MAX_PARTICLES * 2),
      2
    )
    geom.setAttribute(SPRAY_OPACITY_ATTR, this.opacityAttr)
    geom.setAttribute(SPRAY_CLOUD_UV_ATTR, this.cloudUvAttr)

    const { material, uniforms } = createSprayMaterial(this.sprayTex)
    this.uniforms = uniforms
    this.mesh = new THREE.InstancedMesh(geom, material, MAX_PARTICLES)
    this.mesh.frustumCulled = false
    this.mesh.castShadow = false
    this.mesh.receiveShadow = false
    this.mesh.renderOrder = 2

    for (let i = MAX_PARTICLES - 1; i >= 0; i--) {
      this.mesh.setMatrixAt(i, this.zeroMatrix)
      this.freeSlots.push(i)
    }

    this.group.add(this.mesh)
  }

  setEmitters(emitters: WaterfallSprayEmitter[]) {
    this.emitters = emitters
    let total = 0
    for (const e of emitters) total += e.intensity
    this.totalIntensity = total
  }

  setLightFactor(value: number) {
    if (this.uniforms.lightFactor.value === value) return
    this.uniforms.lightFactor.value = value
  }

  setDayWhiteness(value: number) {
    if (this.uniforms.dayWhiteness.value === value) return
    this.uniforms.dayWhiteness.value = value
  }

  setCloudReflectionMix(value: number) {
    if (this.uniforms.cloudReflectionMix.value === value) return
    this.uniforms.cloudReflectionMix.value = value
  }

  update(deltaTime: number, camera: THREE.Camera | undefined) {
    if (!camera) return

    if (this.emitters.length === 0 && this.liveCount === 0) {
      this.spawnAccumulator = 0
      return
    }

    const dt = Math.min(deltaTime, 0.1)
    let spawnedThisFrame = 0
    if (this.emitters.length > 0 && this.totalIntensity > 0) {
      const spawnRate = Math.min(2800, 520 + this.totalIntensity * 230)
      this.spawnAccumulator += dt * spawnRate
      while (this.spawnAccumulator >= 1) {
        this.spawnAccumulator -= 1
        if (this.spawn()) spawnedThisFrame++
      }
    } else {
      this.spawnAccumulator = 0
    }

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
        this.freeSlots.push(i)
        this.liveCount--
        continue
      }

      p.vy -= 1.45 * dt
      p.vx += Math.sin(p.age * 7 + p.phase) * 0.34 * dt
      p.vz += Math.cos(p.age * 6 + p.phase * 1.7) * 0.34 * dt

      p.x += p.vx * dt
      p.y += p.vy * dt
      p.z += p.vz * dt

      const t = p.age / p.maxAge
      let opacity: number
      if (t < 0.12) opacity = t / 0.12
      else if (t > 0.42) opacity = 1 - (t - 0.42) / 0.58
      else opacity = 1
      opacityArr[i] = Math.max(0, opacity) * 0.6

      const scale = p.baseScale * (0.95 + t * 0.45)
      this.tmpPos.set(p.x, p.y, p.z)
      this.tmpScale.set(scale, scale, scale)
      this.tmpMatrix.compose(this.tmpPos, camQuat, this.tmpScale)
      this.mesh.setMatrixAt(i, this.tmpMatrix)
    }

    if (this.liveCount > 0 || spawnedThisFrame > 0) {
      this.mesh.instanceMatrix.needsUpdate = true
      this.opacityAttr.needsUpdate = true
    }
    if (spawnedThisFrame > 0) {
      this.cloudUvAttr.needsUpdate = true
    }
  }

  dispose() {
    this.mesh.geometry.dispose()
    if (this.mesh.material instanceof THREE.Material) {
      this.mesh.material.dispose()
    }
    this.sprayTex.dispose()
    if (this.group.parent) {
      this.group.parent.remove(this.group)
    }
  }

  private pickEmitter(): WaterfallSprayEmitter | null {
    if (this.emitters.length === 0) return null
    if (this.totalIntensity <= 0) return this.emitters[0]

    let roll = Math.random() * this.totalIntensity
    for (const emitter of this.emitters) {
      roll -= emitter.intensity
      if (roll <= 0) return emitter
    }
    return this.emitters[this.emitters.length - 1]
  }

  private ensureClusters(emitter: WaterfallSprayEmitter): SprayClusterSet {
    const cached = this.clusterCache.get(emitter)
    if (cached) return cached

    const clusterCount =
      SPRAY_CLUSTER_MIN +
      Math.floor(Math.random() * (SPRAY_CLUSTER_MAX - SPRAY_CLUSTER_MIN + 1))
    const clusters: SprayCluster[] = []
    let totalWeight = 0
    for (let i = 0; i < clusterCount; i++) {
      const lane = i / Math.max(1, clusterCount - 1) - 0.5
      const weight = 0.85 + Math.random() * 0.9
      totalWeight += weight
      clusters.push({
        across:
          lane * emitter.radius * 1.45 +
          (Math.random() - 0.5) * emitter.radius * 0.22,
        along:
          Math.pow(Math.random(), 0.9) * emitter.radius * 0.62 -
          emitter.radius * 0.1,
        radius: emitter.radius * (0.1 + Math.random() * 0.1),
        weight,
      })
    }
    const set: SprayClusterSet = { clusters, totalWeight }
    this.clusterCache.set(emitter, set)
    return set
  }

  private pickCluster(emitter: WaterfallSprayEmitter): SprayCluster {
    const { clusters, totalWeight } = this.ensureClusters(emitter)
    let roll = Math.random() * totalWeight
    for (const cluster of clusters) {
      roll -= cluster.weight
      if (roll <= 0) return cluster
    }
    return clusters[clusters.length - 1]
  }

  private spawn(): boolean {
    const emitter = this.pickEmitter()
    if (!emitter) return false

    const slot = this.freeSlots.pop()
    if (slot === undefined) return false

    const p = this.pool[slot]
    const cloudUv = this.cloudUvAttr.array as Float32Array
    const acrossX = -emitter.dirZ
    const acrossZ = emitter.dirX
    const cluster = this.pickCluster(emitter)
    const angle = Math.random() * Math.PI * 2
    const dist = Math.pow(Math.random(), 0.75) * cluster.radius
    const ovalAcross = Math.cos(angle) * dist * (0.85 + Math.random() * 0.35)
    const ovalAlong = Math.sin(angle) * dist * (0.45 + Math.random() * 0.35)
    const stray = Math.random() < 0.04
    const across =
      cluster.across +
      ovalAcross +
      (stray ? (Math.random() - 0.5) * emitter.radius * 0.42 : 0)
    const downstreamTail = Math.pow(Math.random(), 0.8) * emitter.radius * 0.38
    const upstreamJitter = Math.random() * emitter.radius * 0.12
    const along =
      cluster.along +
      ovalAlong +
      downstreamTail -
      upstreamJitter +
      (stray ? (Math.random() - 0.5) * emitter.radius * 0.28 : 0)
    const lift = Math.random() * Math.random() * 0.36

    p.x = emitter.x + acrossX * across + emitter.dirX * along
    p.y = emitter.y + lift
    p.z = emitter.z + acrossZ * across + emitter.dirZ * along

    const burst = 0.22 + Math.random() * 0.42 + emitter.intensity * 0.025
    const sideBurst =
      (Math.random() - 0.5) * (0.18 + Math.random() * 0.32)
    p.vx = emitter.dirX * burst + acrossX * sideBurst
    p.vz = emitter.dirZ * burst + acrossZ * sideBurst
    p.vy =
      burst * emitter.launchSlope +
      0.22 +
      Math.random() * 0.22 +
      emitter.intensity * 0.012

    p.maxAge = 1.75 + Math.random() * 1.05
    p.baseScale = 0.38 + Math.random() * 0.34
    p.phase = Math.random() * Math.PI * 2
    p.age = 0
    p.alive = true
    this.liveCount++
    cloudUv[slot * 2] = Math.random()
    cloudUv[slot * 2 + 1] = Math.random()
    return true
  }
}
