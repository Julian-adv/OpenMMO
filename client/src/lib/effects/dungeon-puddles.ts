import * as THREE from 'three'
import {
  MeshBasicNodeMaterial,
  MeshStandardNodeMaterial,
  type Node,
} from 'three/webgpu'
import {
  abs,
  atan,
  attribute,
  cameraViewMatrix,
  cos,
  float,
  length,
  max,
  min,
  mix,
  positionLocal,
  sin,
  smoothstep,
  uniform,
  uv,
  vec2,
  vec3,
  vec4,
} from 'three/tsl'
import { valueNoise } from '../shaders/tsl-noise'
import {
  dungeonPuddleBounds,
  type DungeonDrip,
  type DungeonPuddle,
} from '../utils/dungeon-puddles'

const SURFACE_Y = 0.012
const MAX_INTERVAL_MULTIPLIER = 6

interface ScheduledDrip {
  source: DungeonDrip
  puddleIndex: number
  slot: number
  randomSeed: number
  lastImpact: number
  nextImpact: number
}

function nextDripInterval(drip: ScheduledDrip) {
  drip.randomSeed = (Math.imul(drip.randomSeed, 1664525) + 1013904223) >>> 0
  return (
    drip.source.period *
    (1 + (drip.randomSeed / 4294967296) * (MAX_INTERVAL_MULTIPLIER - 1))
  )
}

export class DungeonPuddles {
  readonly group = new THREE.Group()
  private readonly time = uniform(0)
  private readonly animated = uniform(1)
  private readonly drops: THREE.InstancedMesh
  private readonly drips: ScheduledDrip[]
  private readonly rippleTimings: THREE.InstancedBufferAttribute[]
  private readonly dropStarts: THREE.InstancedBufferAttribute
  private readonly impacts: DungeonDrip[] = []
  private readonly fallTime: number

  constructor(puddles: DungeonPuddle[], wallHeight: number) {
    this.group.name = 'dungeonPuddles'
    const height = wallHeight - 0.2
    const fallTime = Math.sqrt(height / 5)
    this.fallTime = fallTime
    const schedules = puddles.map((p, puddleIndex) =>
      p.drips.map((source, slot): ScheduledDrip => {
        const drip = {
          source,
          puddleIndex,
          slot,
          randomSeed: Math.floor(source.seed * 0x1000000) >>> 0,
          lastImpact: 0,
          nextImpact: 0,
        }
        const interval = nextDripInterval(drip)
        drip.lastImpact = fallTime - (source.phase / source.period) * interval
        if (drip.lastImpact > 0) drip.lastImpact -= interval
        drip.nextImpact = drip.lastImpact + interval
        return drip
      })
    )
    this.drips = schedules.flat()
    const geometry = new THREE.PlaneGeometry(1, 1)
    geometry.rotateX(-Math.PI / 2)
    const size = new THREE.InstancedBufferAttribute(
      new Float32Array(puddles.flatMap((p) => [p.width, p.depth])),
      2
    )
    geometry.setAttribute('aPuddleSize', size)
    const bounds = puddles.map(dungeonPuddleBounds)
    geometry.setAttribute(
      'aPuddleUv',
      new THREE.InstancedBufferAttribute(
        new Float32Array(
          puddles.flatMap((p, i) => {
            const b = bounds[i]
            return [
              0.5 + (b.minX - p.x) / p.width,
              0.5 - (b.maxZ - p.z) / p.depth,
              (b.maxX - b.minX) / p.width,
              (b.maxZ - b.minZ) / p.depth,
            ]
          })
        ),
        4
      )
    )
    geometry.setAttribute(
      'aPuddleSeed',
      new THREE.InstancedBufferAttribute(
        new Float32Array(puddles.map((p) => p.seed)),
        1
      )
    )
    geometry.setAttribute(
      'aPuddleShape',
      new THREE.InstancedBufferAttribute(
        new Float32Array(
          puddles.flatMap(({ shape }) => [
            shape.lobes,
            shape.irregularity,
            shape.phase,
            shape.skew,
          ])
        ),
        4
      )
    )
    const seed = attribute<'float'>('aPuddleSeed', 'float')
    const shape = attribute<'vec4'>('aPuddleShape', 'vec4')
    const uvTransform = attribute<'vec4'>('aPuddleUv', 'vec4')
    const puddleUv = uv().mul(uvTransform.zw).add(uvTransform.xy)
    const local = puddleUv.sub(0.5).mul(2)
    const angle = atan(local.y.negate(), local.x)
    const boundary = float(0.68)
      .add(sin(angle.mul(shape.x).add(shape.z)).mul(shape.y))
      .add(sin(angle.mul(shape.x.add(1)).sub(shape.z)).mul(0.04))
      .add(cos(angle.sub(shape.z.mul(0.7))).mul(shape.w))
    const noise = valueNoise(local.mul(7).add(seed))
    const edge = length(local).sub(boundary).add(noise.sub(0.5).mul(0.025))
    const damp = float(1).sub(smoothstep(-0.015, 0.025, edge))
    const water = float(1).sub(smoothstep(-0.045, -0.01, edge))
    const point = puddleUv
      .sub(0.5)
      .mul(attribute<'vec2'>('aPuddleSize', 'vec2'))
    let ripple: Node<'float'> = float(0)
    let slope: Node<'vec2'> = vec2(0)
    for (let index = 0; index < 2; index++) {
      const name = `aPuddleDrip${index}`
      geometry.setAttribute(
        name,
        new THREE.InstancedBufferAttribute(
          new Float32Array(
            puddles.flatMap((p, puddleIndex) => {
              const drip = schedules[puddleIndex][index]
              return drip
                ? [drip.source.x - p.x, p.z - drip.source.z, drip.lastImpact, 1]
                : [0, 0, 0, 0]
            })
          ),
          4
        )
      )
      const drip = attribute<'vec4'>(name, 'vec4')
      const age = this.time.sub(drip.z)
      const offset = point.sub(drip.xy)
      const radius = length(offset)
      const wave = radius.sub(max(age, 0).mul(0.65))
      const envelope = float(1)
        .sub(smoothstep(0.04, 0.17, abs(wave)))
        .mul(float(1).sub(smoothstep(0.15, 1.2, age)))
        .mul(smoothstep(0, 0.045, age))
        .mul(this.animated)
        .mul(drip.w)
      const ring = sin(wave.mul(80)).mul(envelope)
      ripple = ripple.add(ring)
      slope = slope.add(offset.div(max(radius, 0.01)).mul(ring.mul(0.09)))
    }

    const dampMaterial = new MeshBasicNodeMaterial({
      transparent: true,
      depthWrite: false,
      blending: THREE.MultiplyBlending,
      premultipliedAlpha: true,
      colorNode: vec3(0.55, 0.59, 0.6).mul(damp),
      opacityNode: damp,
    })
    const surfaceMaterial = new MeshStandardNodeMaterial({
      transparent: true,
      depthWrite: false,
      color: 0x293b3b,
      metalness: 0.12,
      roughnessNode: mix(float(0.12), float(0.24), noise),
      opacityNode: water.mul(0.5),
      normalNode: cameraViewMatrix
        .mul(vec4(slope.x, 1, slope.y.negate(), 0))
        .xyz.normalize(),
      emissiveNode: vec3(0.12, 0.16, 0.18).mul(max(ripple, 0).mul(0.16)),
    })
    const dampMesh = new THREE.InstancedMesh(
      geometry,
      dampMaterial,
      puddles.length
    )
    const surfaceMesh = new THREE.InstancedMesh(
      geometry.clone(),
      surfaceMaterial,
      puddles.length
    )
    this.rippleTimings = [0, 1].map((slot) => {
      const timing = surfaceMesh.geometry.getAttribute(
        `aPuddleDrip${slot}`
      ) as THREE.InstancedBufferAttribute
      timing.setUsage(THREE.DynamicDrawUsage)
      return timing
    })
    dampMesh.renderOrder = 1
    surfaceMesh.renderOrder = 2
    surfaceMesh.receiveShadow = true

    const dropGeometry = new THREE.SphereGeometry(1, 6, 4)
    dropGeometry.scale(0.02, 0.065, 0.02)
    this.dropStarts = new THREE.InstancedBufferAttribute(
      new Float32Array(this.drips.map((drip) => drip.nextImpact - fallTime)),
      1
    ).setUsage(THREE.DynamicDrawUsage)
    dropGeometry.setAttribute('aDripStart', this.dropStarts)
    const age = this.time.sub(attribute<'float'>('aDripStart', 'float'))
    const falling = min(max(age, 0), fallTime)
    const dropMaterial = new MeshStandardNodeMaterial({
      transparent: true,
      depthWrite: false,
      color: 0x9bafb5,
      roughness: 0.2,
      metalness: 0.1,
      opacityNode: age
        .greaterThanEqual(0)
        .and(age.lessThan(fallTime))
        .select(0.7, 0),
      positionNode: positionLocal.add(
        vec3(0, float(height).sub(falling.mul(falling).mul(5)), 0)
      ),
    })
    this.drops = new THREE.InstancedMesh(
      dropGeometry,
      dropMaterial,
      this.drips.length
    )
    this.drops.renderOrder = 3
    this.drops.visible = this.drips.length > 0
    const matrix = new THREE.Matrix4()
    bounds.forEach((b, i) => {
      const x = (b.minX + b.maxX) / 2
      const z = (b.minZ + b.maxZ) / 2
      matrix
        .makeScale(b.maxX - b.minX, 1, b.maxZ - b.minZ)
        .setPosition(x, SURFACE_Y, z)
      dampMesh.setMatrixAt(i, matrix)
      matrix.setPosition(x, SURFACE_Y + 0.002, z)
      surfaceMesh.setMatrixAt(i, matrix)
    })
    this.drips.forEach(({ source }, i) => {
      matrix.makeTranslation(source.x, SURFACE_Y, source.z)
      this.drops.setMatrixAt(i, matrix)
    })
    this.group.add(dampMesh, surfaceMesh, this.drops)
    for (const mesh of [dampMesh, surfaceMesh, this.drops]) {
      mesh.raycast = () => {}
      mesh.computeBoundingBox()
      if (mesh === this.drops && this.drips.length)
        mesh.boundingBox!.max.y += height
      mesh.boundingSphere = mesh.boundingBox!.getBoundingSphere(
        new THREE.Sphere()
      )
    }
  }

  update(dt: number, animate = true) {
    this.time.value += Math.min(Math.max(dt, 0), 0.1)
    this.animated.value = animate ? 1 : 0
    this.drops.visible = animate && this.drips.length > 0
    this.impacts.length = 0
    for (let index = 0; index < this.drips.length; index++) {
      const drip = this.drips[index]
      if (this.time.value < drip.nextImpact) continue
      drip.lastImpact = drip.nextImpact
      const timing = this.rippleTimings[drip.slot]
      timing.setZ(drip.puddleIndex, drip.lastImpact)
      timing.needsUpdate = true
      this.impacts.push(drip.source)
      drip.nextImpact += nextDripInterval(drip)
      this.dropStarts.setX(index, drip.nextImpact - this.fallTime)
      this.dropStarts.needsUpdate = true
    }
    return this.impacts
  }

  dispose() {
    for (const child of this.group.children) {
      const mesh = child as THREE.InstancedMesh<
        THREE.BufferGeometry,
        MeshBasicNodeMaterial | MeshStandardNodeMaterial
      >
      mesh.dispose()
      mesh.geometry.dispose()
      mesh.material.dispose()
    }
    this.group.clear()
    this.group.removeFromParent()
  }
}
