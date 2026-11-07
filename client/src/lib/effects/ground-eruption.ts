import * as THREE from 'three'
import { MeshBasicNodeMaterial, MeshStandardNodeMaterial } from 'three/webgpu'
import {
  PI2,
  abs,
  atan,
  color,
  float,
  fract,
  max,
  mix,
  mx_fractal_noise_float,
  mx_noise_float,
  mx_worley_noise_vec2,
  positionGeometry,
  smoothstep,
  uniform,
  uv,
  vec3,
} from 'three/tsl'
import { createNoise2D, createRng } from '../utils/simplex-noise'
import {
  GRAVITY,
  ZERO_SCALE,
  ballistic,
  dustMaterial,
  instancedMesh,
  landTime,
  rubble,
  type Rock,
} from './dormant-wake'
import { crystalShardMaterial } from './crystal-glow'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

const RISE_MS = 120
const HOLD_MS = 750
const SINK_MS = 800
const COOL_MS = 1400
const TAIL_MS = 2800
const TREMOR_MS = 600
const SPIKES = 9
const PEBBLES = 8
const DEBRIS = 30
const WISPS = 8
const DUST = 22
const SPARKS = 14
const GROUND_LIFT = 0.04

/** 1 below `lo`, easing to 0 at `hi`. */
export const drop = (lo: N, hi: N, x: N) => float(1).sub(smoothstep(lo, hi, x))

interface Spike {
  x: number
  z: number
  height: number
  width: number
  tiltX: number
  tiltZ: number
  turn: number
  delay: number
}

interface Puff {
  at: number
  life: number
  x: number
  y: number
  z: number
  vx: number
  vy: number
  vz: number
  drag: number
  size: number
  grow: number
  peak: number
  seed: number
  shade: number
  roll: number
}

export interface Spark {
  /** Launch time, in the same ms as `placeSparks`'s `elapsedMs`. */
  at: number
  x: number
  y: number
  z: number
  vx: number
  vy: number
  vz: number
  life: number
  size: number
}

/** A jagged basalt shard: base radius 1 at the origin, tip at +1 Y. */
export function shardGeometry() {
  const geometry = new THREE.CylinderGeometry(0.05, 1, 1, 8, 7)
  geometry.translate(0, 0.5, 0)
  const pos = geometry.getAttribute('position')
  const noise = createNoise2D(23)
  const moved = new Map<string, [number, number, number]>()
  for (let i = 0; i < pos.count; i++) {
    const [x, y, z] = [pos.getX(i), pos.getY(i), pos.getZ(i)]
    const key = `${x.toFixed(3)},${y.toFixed(3)},${z.toFixed(3)}`
    let p = moved.get(key)
    if (!p) {
      const a = Math.atan2(x, z)
      const bulge =
        1 +
        0.28 * noise(Math.cos(a) * 1.7 + y * 2.5, Math.sin(a) * 1.7) +
        0.12 * noise(a * 3, y * 7)
      // The point drifts off-axis so no two faces line up.
      p = [x * bulge + y * y * 0.12, y + noise(a, y * 3) * 0.04, z * bulge]
      moved.set(key, p)
    }
    pos.setXYZ(i, p[0], p[1], p[2])
  }
  const flat = geometry.toNonIndexed()
  geometry.dispose()
  flat.computeVertexNormals()
  return flat
}

/** Basalt with darker strata; `heat` lights magma in the base's seams. */
export function basalt(heat?: N) {
  const material = new MeshStandardNodeMaterial()
  material.flatShading = true
  material.metalness = 0
  const p = positionGeometry
  const grain = mx_fractal_noise_float(p.mul(5), 3, 2, 0.5)
  const strata = mx_noise_float(p.mul(vec3(1.5, 11, 1.5)))
  const tone = mix(color('#2e2925'), color('#6a5d52'), grain.mul(0.6).add(0.5))
  material.colorNode = mix(
    tone,
    color('#1f1b18'),
    smoothstep(0.15, 0.55, strata).mul(0.55)
  )
  material.roughnessNode = grain.mul(0.12).add(0.86)
  if (heat) {
    const seams = smoothstep(0.35, 0.7, float(1).sub(abs(strata)).pow(8))
    const base = drop(0, 0.2, p.y)
    material.emissiveNode = mix(color('#ff2a00'), color('#ffb347'), seams)
      .mul(seams.add(base.mul(0.35)))
      .mul(base)
      .mul(heat)
      .mul(1.6)
  }
  return material
}

export function sparkMaterial() {
  const material = new MeshBasicNodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.blending = THREE.AdditiveBlending
  material.toneMapped = false
  const core = float(1).sub(uv().sub(0.5).length().mul(2)).clamp(0, 1)
  material.colorNode = mix(color('#ff4a10'), color('#ffe2a0'), core)
  material.opacityNode = core.pow(2)
  return material
}

const sparkDummy = new THREE.Object3D()
const sparkTint = new THREE.Color()

/** Fly each additive spark at `elapsedMs`; it fades as it falls. */
export function placeSparks(
  mesh: THREE.InstancedMesh,
  sparks: Spark[],
  elapsedMs: number,
  camera: THREE.Camera
) {
  for (let i = 0; i < sparks.length; i++) {
    const s = sparks[i]
    const age = (elapsedMs - s.at) / s.life
    const t = (elapsedMs - s.at) / 1000
    const y = s.y + s.vy * t - 0.5 * GRAVITY * t * t
    if (age < 0 || age >= 1 || y < 0) {
      mesh.setMatrixAt(i, ZERO_SCALE)
      continue
    }
    sparkDummy.position.set(s.x + s.vx * t, y, s.z + s.vz * t)
    sparkDummy.quaternion.copy(camera.quaternion)
    sparkDummy.scale.setScalar(s.size * (1 - age * 0.5))
    sparkDummy.updateMatrix()
    mesh.setMatrixAt(i, sparkDummy.matrix)
    // Additive: a dimmer colour is the fade.
    const glow = (1 - age) * 1.8
    sparkTint.setRGB(glow, glow, glow)
    mesh.setColorAt(i, sparkTint)
  }
  mesh.instanceMatrix.needsUpdate = true
  mesh.instanceColor!.needsUpdate = true
}

/** What every pooled eruption shares; only the mark and the shards carry
 *  per-eruption uniforms. */
export class GroundEruptionAssets {
  readonly markGeometry = new THREE.PlaneGeometry(2, 2)
  readonly shardGeometry = shardGeometry()
  readonly rockGeometry = rubble()
  readonly planeGeometry = new THREE.PlaneGeometry(1, 1)
  readonly rockMaterial = basalt()
  readonly dustMaterial = dustMaterial()
  readonly sparkMaterial = sparkMaterial()
  readonly crystalMaterial = crystalShardMaterial()

  dispose() {
    this.crystalMaterial.dispose()
    this.markGeometry.dispose()
    this.shardGeometry.dispose()
    this.rockGeometry.dispose()
    this.planeGeometry.dispose()
    this.rockMaterial.dispose()
    this.dustMaterial.dispose()
    this.sparkMaterial.dispose()
  }
}

/** The ground splitting where a distant target stood, then basalt shards
 *  bursting up through it. Times are ms from the swing; the mark stays put. */
export class GroundEruptionEffect {
  readonly group = new THREE.Group()
  private charge = uniform(0)
  private cool = uniform(0)
  private heat = uniform(0)
  private flood = uniform(0)
  private crystal = uniform(0)
  private markMaterial = new MeshBasicNodeMaterial()
  private mark: THREE.Mesh
  private shardMaterial = basalt(this.heat)
  private shardMesh: THREE.InstancedMesh
  private rockMesh: THREE.InstancedMesh
  private dustMesh: THREE.InstancedMesh
  private sparkMesh: THREE.InstancedMesh
  private particleMeshes: THREE.InstancedMesh[]
  private spikes: Spike[] = []
  private pebbles: { x: number; z: number; size: number; phase: number }[] = []
  private debris: Rock[] = []
  private puffs: Puff[] = []
  private sparks: Spark[] = []
  private impactMs = 1
  private lingerMs = 0
  private sparkless = false
  private dummy = new THREE.Object3D()
  private roll = new THREE.Quaternion()
  private forward = new THREE.Vector3(0, 0, 1)
  private point = new THREE.Vector3()
  private tint = new THREE.Color()

  constructor(private assets: GroundEruptionAssets) {
    const p = uv().sub(0.5).mul(2)
    const r = p.length()
    const cells = mx_worley_noise_vec2(vec3(p.mul(2.4), 7.1), 0.9)
    // Only some cell walls split, so the cracks branch instead of tiling.
    const opened = smoothstep(0.05, 0.35, mx_noise_float(vec3(p.mul(1.8), 4.4)))
    const web = cells.y.sub(cells.x).mul(0.2).add(float(1).sub(opened).mul(0.2))
    const jag = atan(p.y, p.x).add(
      mx_noise_float(vec3(p.mul(2.2), 3.7)).mul(0.5)
    )
    const spoke = abs(fract(jag.div(PI2).mul(6)).sub(0.5))
      .mul(PI2.div(6))
      .mul(r)
    const gap = web.min(spoke).div(mix(1.4, 0.7, r))
    const grown = drop(
      this.charge.sub(0.08),
      this.charge.add(0.04),
      r.add(mx_noise_float(vec3(p.mul(4), 9.2)).mul(0.08))
    )
    const fissure = drop(0.008, 0.03, gap).mul(grown)
    const lip = drop(0.02, 0.07, gap).mul(grown)
    // Lingering magma floods the cracks wider.
    const molten = drop(0, mix(0.008, 0.05, this.flood), gap)
      .mul(grown)
      .mul(drop(0.5, 1.05, r).mul(0.6).add(0.4))
    const inside = drop(0.7, 1, r)
    const flicker = mx_noise_float(vec3(p.mul(5), this.charge.mul(9)))
      .mul(0.25)
      .add(0.85)
    const glow = max(this.charge.mul(this.charge), this.heat)
      .mul(flicker)
      .mul(float(1).sub(this.cool))
    const hot = mix(
      mix(color('#b81d00'), color('#ffbe5c'), molten.mul(glow)),
      mix(color('#1d34c8'), color('#a8d4ff'), molten.mul(glow)),
      this.crystal
    )
    this.markMaterial.transparent = true
    this.markMaterial.depthWrite = false
    this.markMaterial.toneMapped = false
    this.markMaterial.polygonOffset = true
    this.markMaterial.polygonOffsetFactor = -2
    this.markMaterial.colorNode = mix(
      color('#140d09'),
      hot.mul(glow.mul(1.6)),
      molten.mul(glow).add(fissure.mul(glow).mul(0.12)).min(1)
    )
    // Scorched soil around near-black fissures.
    const soil = float(1).sub(r).mul(0.25).add(0.1).mul(this.charge)
    this.markMaterial.opacityNode = max(fissure.mul(0.95), lip.mul(0.45))
      .max(soil)
      .mul(inside)
      .mul(drop(0.35, 0.8, this.cool))
    this.mark = new THREE.Mesh(assets.markGeometry, this.markMaterial)
    this.mark.rotation.x = -Math.PI / 2
    this.mark.position.y = GROUND_LIFT
    this.mark.renderOrder = 2

    this.shardMesh = instancedMesh(
      assets.shardGeometry,
      this.shardMaterial,
      SPIKES
    )
    this.rockMesh = instancedMesh(
      assets.rockGeometry,
      assets.rockMaterial,
      PEBBLES + DEBRIS
    )
    this.dustMesh = instancedMesh(
      assets.planeGeometry,
      assets.dustMaterial,
      WISPS + DUST
    )
    this.sparkMesh = instancedMesh(
      assets.planeGeometry,
      assets.sparkMaterial,
      SPARKS
    )
    this.shardMesh.castShadow = true
    this.rockMesh.castShadow = true
    this.particleMeshes = [
      this.shardMesh,
      this.rockMesh,
      this.dustMesh,
      this.sparkMesh,
    ]
    this.group.add(this.mark, ...this.particleMeshes)
    this.clear()
  }

  /** `crystal` raises glowing black-violet crystal instead of basalt. */
  start(
    impactMs: number,
    radius: number,
    seed: number,
    lingerMs = 0,
    crystal = false
  ) {
    const rand = createRng(seed)
    this.crystal.value = crystal ? 1 : 0
    this.shardMesh.material = crystal
      ? this.assets.crystalMaterial
      : this.shardMaterial
    this.sparkless = crystal
    this.impactMs = Math.max(1, impactMs)
    this.lingerMs = lingerMs
    this.mark.scale.set(radius, radius, 1)
    this.mark.rotation.z = rand() * Math.PI * 2
    this.group.visible = true
    const k = radius / 1.5

    this.spikes.length = 0
    this.spikes.push({
      x: 0,
      z: 0,
      height: (2.3 + rand() * 0.5) * k,
      width: 0.55 * k,
      tiltX: (rand() - 0.5) * 0.2,
      tiltZ: (rand() - 0.5) * 0.2,
      turn: rand() * Math.PI * 2,
      delay: 0,
    })
    for (let i = 1; i < SPIKES; i++) {
      const outer = i > 5
      const a = (i / (SPIKES - 1)) * Math.PI * 2 + rand() * 0.7
      const d = (outer ? 0.95 + rand() * 0.3 : 0.45 + rand() * 0.4) * k
      const lean = outer ? 0.55 + rand() * 0.3 : 0.2 + rand() * 0.25
      this.spikes.push({
        x: Math.cos(a) * d,
        z: Math.sin(a) * d,
        height: (outer ? 0.55 + rand() * 0.4 : 1.1 + rand() * 0.8) * k,
        width: (outer ? 0.2 + rand() * 0.1 : 0.3 + rand() * 0.15) * k,
        // Lean away from the centre, as if the ground split open.
        tiltX: Math.sin(a) * lean,
        tiltZ: -Math.cos(a) * lean,
        turn: rand() * Math.PI * 2,
        delay: (outer ? 60 : 20) + rand() * 80,
      })
    }
    for (let i = 0; i < SPIKES; i++) {
      const v = 0.8 + rand() * 0.35
      this.tint.setRGB(v, v * 0.97, v * 0.93)
      this.shardMesh.setColorAt(i, this.tint)
    }
    this.shardMesh.instanceColor!.needsUpdate = true

    this.pebbles.length = 0
    for (let i = 0; i < PEBBLES; i++) {
      const a = rand() * Math.PI * 2
      const d = (0.3 + rand() * 0.6) * radius
      this.pebbles.push({
        x: Math.cos(a) * d,
        z: Math.sin(a) * d,
        size: 0.07 + rand() * 0.08,
        phase: rand() * Math.PI * 2,
      })
    }
    this.debris.length = 0
    for (let i = 0; i < DEBRIS; i++) {
      const a = rand() * Math.PI * 2
      const big = i < 8
      const out = big ? 1.5 + rand() * 2 : 2 + rand() * 4.5
      this.debris.push({
        at: this.impactMs + rand() * 90,
        x: Math.cos(a) * 0.3 * k,
        y: 0.2 + rand() * 0.8,
        z: Math.sin(a) * 0.3 * k,
        vx: Math.cos(a) * out,
        vy: big ? 2.5 + rand() * 3 : 4 + rand() * 5,
        vz: Math.sin(a) * out,
        size: big ? 0.25 + rand() * 0.2 : 0.07 + rand() * 0.14,
        land: 0,
        stretch: new THREE.Vector3(1 + rand() * 0.7, 0.55 + rand() * 0.4, 1),
        spin: new THREE.Vector3(rand(), rand(), rand()).multiplyScalar(10),
      })
    }
    for (const r of this.debris) r.land = landTime(r)
    for (let i = 0; i < PEBBLES + DEBRIS; i++) {
      // Some clods of earth among the stone.
      const earth = rand() < 0.35
      const v = 0.75 + rand() * 0.4
      this.tint.setRGB(
        (earth ? 0.78 : 1) * v,
        (earth ? 0.62 : 0.97) * v,
        (earth ? 0.45 : 0.93) * v
      )
      this.rockMesh.setColorAt(i, this.tint)
    }
    this.rockMesh.instanceColor!.needsUpdate = true

    this.puffs.length = 0
    for (let i = 0; i < WISPS; i++) {
      const a = rand() * Math.PI * 2
      const d = rand() * radius * 0.85
      this.puffs.push({
        at: this.impactMs * (0.35 + rand() * 0.55),
        life: 900 + rand() * 500,
        x: Math.cos(a) * d,
        y: 0.05,
        z: Math.sin(a) * d,
        vx: 0,
        vy: 0.5 + rand() * 0.4,
        vz: 0,
        drag: 1,
        size: 0.35 + rand() * 0.25,
        grow: 0.6,
        peak: 0.35,
        seed: rand(),
        shade: 0.85,
        roll: rand() * Math.PI * 2,
      })
    }
    for (let i = 0; i < DUST; i++) {
      const ring = i < 14
      const a = (i / (ring ? 14 : DUST - 14)) * Math.PI * 2 + rand() * 0.5
      const out = ring ? 4 + rand() * 2.5 : 0.6 + rand() * 0.8
      this.puffs.push({
        at: this.impactMs + rand() * (ring ? 60 : 200),
        life: ring ? 1100 + rand() * 600 : 1800 + rand() * 900,
        x: Math.cos(a) * 0.5 * k,
        y: ring ? 0.15 : 0.4 + rand() * 1.2,
        z: Math.sin(a) * 0.5 * k,
        vx: Math.cos(a) * out,
        vy: ring ? 0.25 + rand() * 0.3 : 1.2 + rand() * 1.2,
        vz: Math.sin(a) * out,
        drag: ring ? 3.2 : 1.4,
        size: ring ? 0.7 + rand() * 0.4 : 1.1 + rand() * 0.8,
        grow: ring ? 1.4 : 2.2,
        peak: ring ? 0.75 : 0.6,
        seed: rand(),
        shade: 0.75 + rand() * 0.25,
        roll: rand() * Math.PI * 2,
      })
    }

    this.sparks.length = 0
    for (let i = 0; i < SPARKS; i++) {
      const a = rand() * Math.PI * 2
      const out = 1 + rand() * 3
      this.sparks.push({
        at: this.impactMs,
        x: Math.cos(a) * 0.2,
        y: 0.2,
        z: Math.sin(a) * 0.2,
        vx: Math.cos(a) * out,
        vy: 3 + rand() * 5,
        vz: Math.sin(a) * out,
        life: 350 + rand() * 450,
        size: 0.05 + rand() * 0.06,
      })
    }
  }

  /** False once the shards have sunk and the dust settled. */
  update(elapsedMs: number, camera: THREE.Camera): boolean {
    const since = elapsedMs - this.impactMs
    const linger = this.lingerMs
    if (since >= TAIL_MS + linger) {
      this.clear()
      return false
    }
    const settled = Math.max(0, since - linger)
    this.charge.value = THREE.MathUtils.clamp(elapsedMs / this.impactMs, 0, 1)
    this.heat.value =
      since < 0 ? 0 : 1 - THREE.MathUtils.smoothstep(settled, 0, COOL_MS)
    this.cool.value =
      since < 0 ? 0 : THREE.MathUtils.smoothstep(settled, 200, TAIL_MS - 300)
    this.flood.value =
      linger > 0 && since >= 0
        ? THREE.MathUtils.smoothstep(since, 0, 500) *
          (1 - THREE.MathUtils.smoothstep(settled, 0, COOL_MS))
        : 0
    // Past the tail only a lingering mark is left to animate.
    const particles = since < TAIL_MS
    for (const mesh of this.particleMeshes) mesh.visible = particles
    this.sparkMesh.visible &&= !this.sparkless
    if (!particles) return true
    this.updateSpikes(since)
    this.updateRocks(elapsedMs, since)
    this.updatePuffs(elapsedMs, camera)
    placeSparks(this.sparkMesh, this.sparks, elapsedMs, camera)
    return true
  }

  private updateSpikes(since: number) {
    for (let i = 0; i < this.spikes.length; i++) {
      const s = this.spikes[i]
      const t = since - s.delay
      if (t < 0 || t >= RISE_MS + HOLD_MS + SINK_MS) {
        this.shardMesh.setMatrixAt(i, ZERO_SCALE)
        continue
      }
      // Punch up past full height, settle, then sink back into the ground.
      const up =
        t < RISE_MS
          ? Math.sin(((t / RISE_MS) * Math.PI) / 2) * 1.1
          : t < RISE_MS + 140
            ? 1.1 - 0.1 * ((t - RISE_MS) / 140)
            : 1
      const sink = Math.max(0, (t - RISE_MS - HOLD_MS) / SINK_MS)
      const shake = t < RISE_MS + 200 ? Math.sin(t * 0.25) * 0.02 : 0
      this.dummy.position.set(s.x + shake, -s.height * sink * sink * 1.05, s.z)
      this.dummy.rotation.set(s.tiltX, s.turn, s.tiltZ)
      this.dummy.scale.set(s.width, s.height * up, s.width)
      this.dummy.updateMatrix()
      this.shardMesh.setMatrixAt(i, this.dummy.matrix)
    }
    this.shardMesh.instanceMatrix.needsUpdate = true
  }

  private updateRocks(elapsedMs: number, since: number) {
    // Pebbles on the mark rattle as the ground is about to give way.
    for (let i = 0; i < PEBBLES; i++) {
      const p = this.pebbles[i]
      if (since >= 0) {
        this.rockMesh.setMatrixAt(i, ZERO_SCALE)
        continue
      }
      const hop =
        -since < TREMOR_MS
          ? Math.abs(Math.sin(elapsedMs * 0.045 + p.phase)) *
            0.06 *
            (1 + since / TREMOR_MS)
          : 0
      this.dummy.position.set(p.x, p.size * 0.4 + hop, p.z)
      this.dummy.rotation.set(p.phase, p.phase * 2, 0)
      this.dummy.scale.setScalar(p.size)
      this.dummy.updateMatrix()
      this.rockMesh.setMatrixAt(i, this.dummy.matrix)
    }
    for (let i = 0; i < this.debris.length; i++) {
      const r = this.debris[i]
      const slot = PEBBLES + i
      const t = (elapsedMs - r.at) / 1000
      const rest = t < 0 ? 0 : ballistic(r, t, this.point)
      const sink = rest < 0.5 ? 0 : Math.min(1, (rest - 0.5) / 0.6)
      if (t < 0 || sink >= 1) {
        this.rockMesh.setMatrixAt(slot, ZERO_SCALE)
        continue
      }
      const turning = rest < 0 ? t : t - rest
      this.dummy.position.copy(this.point)
      this.dummy.position.y += r.size * 0.4 - sink * r.size
      this.dummy.rotation.set(
        r.spin.x * turning,
        r.spin.y * turning,
        r.spin.z * turning
      )
      this.dummy.scale.copy(r.stretch).multiplyScalar(r.size)
      this.dummy.updateMatrix()
      this.rockMesh.setMatrixAt(slot, this.dummy.matrix)
    }
    this.rockMesh.instanceMatrix.needsUpdate = true
  }

  private updatePuffs(elapsedMs: number, camera: THREE.Camera) {
    for (let i = 0; i < this.puffs.length; i++) {
      const d = this.puffs[i]
      const age = (elapsedMs - d.at) / d.life
      if (age < 0 || age >= 1) {
        this.dustMesh.setMatrixAt(i, ZERO_SCALE)
        continue
      }
      const t = (elapsedMs - d.at) / 1000
      const travel = (1 - Math.exp(-d.drag * t)) / d.drag
      const size = d.size * (1 + d.grow * Math.sqrt(age))
      this.dummy.position.set(
        d.x + d.vx * travel,
        d.y + d.vy * travel + size * 0.3,
        d.z + d.vz * travel
      )
      this.dummy.quaternion
        .copy(camera.quaternion)
        .multiply(this.roll.setFromAxisAngle(this.forward, d.roll + t * 0.4))
      this.dummy.scale.setScalar(size)
      this.dummy.updateMatrix()
      this.dustMesh.setMatrixAt(i, this.dummy.matrix)
      const alpha =
        d.peak *
        Math.min(1, age / 0.08) *
        (1 - THREE.MathUtils.smoothstep(age, 0.2, 1))
      this.tint.setRGB(alpha, d.seed, d.shade)
      this.dustMesh.setColorAt(i, this.tint)
    }
    this.dustMesh.instanceMatrix.needsUpdate = true
    this.dustMesh.instanceColor!.needsUpdate = true
  }

  clear() {
    this.group.visible = false
  }

  dispose() {
    this.markMaterial.dispose()
    this.shardMaterial.dispose()
    for (const m of [
      this.shardMesh,
      this.rockMesh,
      this.dustMesh,
      this.sparkMesh,
    ])
      m.dispose()
    this.group.clear()
  }
}
