import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'
import { color, mx_noise_float, uniform, uv, vec3 } from 'three/tsl'
import { createRng } from '../utils/simplex-noise'
import { EARTH_ROAR_STRIKE_MS } from '../data/monsterSkills'
import {
  dustMaterial,
  instancedMesh,
  landTime,
  placeDust,
  placeRocks,
  rockMaterial,
  rubble,
  tintRocks,
  type Dust,
  type Rock,
} from './dormant-wake'

/** The dust wall reaches the roar's edge this long after the roar. */
const WAVE_MS = 700
const END_MS = EARTH_ROAR_STRIKE_MS + 2600
/** Reach is measured this far outside the body (`ENGAGE_OFFSET_M`). */
const ENGAGE = 0.8
const HEIGHT = 9
const WALL_BOTTOM = -1.5
const WALL_HEIGHT = 6.5
const GATHER_DUST = 30
const BURST_DUST = 36
const PEBBLES = 24
const ROCKS = 22

/** A body's `halfWidth|front|back` (monsters.csv `footprint`). */
export interface BodyFootprint {
  halfWidth: number
  front: number
  back: number
}

export function parseFootprint(footprint: string | undefined): BodyFootprint {
  const [halfWidth = 0, front = 0, back = 0] = (footprint ?? '')
    .split('|')
    .map(Number)
  return { halfWidth, front, back }
}

/** Terraclash's roar: dust and scree while it gathers itself, then a dust
 *  wall rolling out as far as the roar reaches. */
export class EarthRoarEffect {
  readonly group = new THREE.Group()
  private planeGeometry = new THREE.PlaneGeometry(1, 1)
  private rockGeometry = rubble()
  private wallGeometry = new THREE.CylinderGeometry(
    1,
    1,
    WALL_HEIGHT,
    96,
    1,
    true
  )
  private dustMaterial = dustMaterial()
  private rockMaterial = rockMaterial()
  private wallMaterial = new MeshBasicNodeMaterial()
  private wallFade = uniform(0)
  private dustMesh: THREE.InstancedMesh
  private rockMesh: THREE.InstancedMesh
  private wall: THREE.Mesh
  private dust: Dust[] = []
  private rocks: Rock[] = []
  private elapsedMs = END_MS
  private halfX: number
  private halfZ: number

  /** `reach` is measured from the body, as the server does. */
  constructor(
    private reach: number,
    body: BodyFootprint
  ) {
    this.halfX = body.halfWidth
    this.halfZ = (body.front + body.back) / 2

    const m = this.wallMaterial
    m.transparent = true
    m.depthWrite = false
    m.side = THREE.DoubleSide
    m.colorNode = color('#9a8a70')
    const lumps = mx_noise_float(vec3(uv().x.mul(48), uv().y.mul(3), 0))
      .mul(0.5)
      .add(0.5)
    // Thick at the ground, thinning upward.
    m.opacityNode = uv()
      .y.oneMinus()
      .pow(1.2)
      .mul(lumps.mul(0.65).add(0.35))
      .mul(this.wallFade)
      .clamp(0, 1)

    this.dustMesh = instancedMesh(
      this.planeGeometry,
      this.dustMaterial,
      GATHER_DUST + BURST_DUST
    )
    this.rockMesh = instancedMesh(
      this.rockGeometry,
      this.rockMaterial,
      PEBBLES + ROCKS
    )
    this.rockMesh.castShadow = true
    this.wall = new THREE.Mesh(this.wallGeometry, this.wallMaterial)
    this.wall.position.y = WALL_BOTTOM + WALL_HEIGHT / 2
    this.wall.frustumCulled = false
    this.wall.renderOrder = 2
    this.group.add(this.dustMesh, this.rockMesh, this.wall)
    this.group.visible = false
  }

  /** Lay out one roar from the boss standing at `origin`, `atMs` into it. */
  start(origin: THREE.Vector3, atMs = 0) {
    const rand = createRng(Math.floor(origin.x * 131 + origin.z * 17))
    this.group.position.copy(origin)
    this.group.visible = true
    this.elapsedMs = atMs
    const rim = (k: number) => {
      const a = rand() * Math.PI * 2
      const x = Math.cos(a) * this.halfX * k
      return [x, Math.sin(a) * this.halfZ * k, a] as const
    }

    this.dust.length = 0
    for (let i = 0; i < GATHER_DUST; i++) {
      const [x, z, a] = rim(1 + rand() * 0.2)
      const out = 0.3 + rand() * 0.5
      this.dust.push({
        at: 600 + (i / GATHER_DUST) * (EARTH_ROAR_STRIKE_MS - 800),
        life: 1300 + rand() * 800,
        x,
        y: 0.2,
        z,
        vx: Math.cos(a) * out,
        vy: 0.3 + rand() * 0.4,
        vz: Math.sin(a) * out,
        size: 1.4 + rand() * 1.2,
        grow: 2 + rand() * 1.5,
        peak: 0.6 + rand() * 0.2,
        seed: rand(),
        shade: 0.75 + rand() * 0.2,
      })
    }
    for (let i = 0; i < BURST_DUST; i++) {
      const [x, z, a] = rim(1.05)
      const out = 5 + rand() * 6
      this.dust.push({
        at: EARTH_ROAR_STRIKE_MS + rand() * 250,
        life: 2000 + rand() * 800,
        x,
        y: 0.3,
        z,
        vx: Math.cos(a) * out,
        vy: 0.5 + rand() * 1.2,
        vz: Math.sin(a) * out,
        size: 3.5 + rand() * 2.5,
        grow: 5 + rand() * 3,
        peak: 0.9 + rand() * 0.1,
        seed: rand(),
        shade: 0.85 + rand() * 0.2,
      })
    }

    this.rocks.length = 0
    for (let i = 0; i < PEBBLES + ROCKS; i++) {
      const gathering = i < PEBBLES
      const h = gathering ? 0.3 + rand() * 0.6 : 0.05 + rand() * 0.2
      const a = rand() * Math.PI * 2
      const ox = Math.cos(a) * (1.05 - h * 0.5)
      const oz = Math.sin(a) * (1.05 - h * 0.5)
      const out = gathering ? 1 + rand() * 1.5 : 4 + rand() * 6
      const rock: Rock = {
        at: gathering
          ? 400 + rand() * (EARTH_ROAR_STRIKE_MS - 600)
          : EARTH_ROAR_STRIKE_MS + rand() * 150,
        x: ox * this.halfX,
        y: HEIGHT * h,
        z: oz * this.halfZ,
        vx: ox * out,
        vy: gathering ? rand() * 1.2 : 3 + rand() * 4,
        vz: oz * out,
        size: gathering ? 0.2 + rand() * 0.3 : 0.4 + rand() * 0.6,
        land: 0,
        stretch: new THREE.Vector3(1 + rand() * 0.6, 0.6 + rand() * 0.4, 1),
        spin: new THREE.Vector3(rand(), rand(), rand()).multiplyScalar(7),
      }
      rock.land = landTime(rock)
      this.rocks.push(rock)
    }
    tintRocks(this.rockMesh, rand, 0)
  }

  update(deltaTime: number, camera: THREE.Camera | undefined) {
    if (!this.group.visible || !camera) return
    this.elapsedMs += deltaTime * 1000
    if (this.elapsedMs >= END_MS) {
      this.group.visible = false
      return
    }
    this.updateWall()
    placeDust(this.dustMesh, this.dust, this.elapsedMs, camera)
    placeRocks(this.rockMesh, this.rocks, this.elapsedMs, 0.6)
  }

  private updateWall() {
    const t = (this.elapsedMs - EARTH_ROAR_STRIKE_MS) / WAVE_MS
    this.wall.visible = t >= 0 && t < 1
    if (!this.wall.visible) return
    const front = this.halfX + (ENGAGE + this.reach) * (1 - (1 - t) ** 2)
    this.wall.scale.set(front, 1, front)
    this.wallFade.value = Math.min(1, t / 0.1) * (1 - t * t)
  }

  dispose() {
    this.group.removeFromParent()
    this.planeGeometry.dispose()
    this.rockGeometry.dispose()
    this.wallGeometry.dispose()
    this.dustMaterial.dispose()
    this.rockMaterial.dispose()
    this.wallMaterial.dispose()
    this.dustMesh.dispose()
    this.rockMesh.dispose()
  }
}
