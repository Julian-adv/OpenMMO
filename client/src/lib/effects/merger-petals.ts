import * as THREE from 'three'
import { MeshBasicNodeMaterial } from 'three/webgpu'

export const MERGER_PETALS_DURATION = 13
const MAX_PETALS = 800
const PER_SQUARE_METRE = 4.1
const RADIUS = 4.5
const WALL_MARGIN = 0.6
const TOP = 7
const RELEASE = 4.5
const SHRINK_BELOW = 0.45
const COLORS = ['#fff6f2', '#ffd9e2', '#f7a8bd', '#ffe9b8']

const hash = (i: number, salt: number) => {
  const v = Math.sin(i * 127.1 + salt * 311.7) * 43758.5453
  return v - Math.floor(v)
}

/** Height above the floor of petal `i` at `elapsed` seconds; negative before
 *  its release and after it lands. */
export function petalHeight(i: number, elapsed: number) {
  const fallen = (elapsed - hash(i, 1) * RELEASE) * (0.85 + hash(i, 2) * 0.6)
  return fallen < 0 || fallen > TOP ? -1 : TOP - fallen
}

/** Floor the petals cover: a building's rectangle, else a circle. */
export type PetalArea = {
  halfWidth: number
  halfDepth: number
  /** Radians, as `ObjectPlacement.rotation`. */
  yaw: number
}

export function petalAreaOf(width: number, depth: number, yawDegrees: number) {
  return {
    halfWidth: width / 2 - WALL_MARGIN,
    halfDepth: depth / 2 - WALL_MARGIN,
    yaw: (yawDegrees * Math.PI) / 180,
  }
}

export class MergerPetals {
  readonly group = new THREE.Group()
  private geometry = new THREE.CircleGeometry(0.5, 8).scale(0.09, 0.15, 1)
  private material = new MeshBasicNodeMaterial()
  private petals: THREE.InstancedMesh
  private dummy = new THREE.Object3D()

  constructor() {
    this.material.side = THREE.DoubleSide
    this.petals = new THREE.InstancedMesh(
      this.geometry,
      this.material,
      MAX_PETALS
    )
    this.petals.frustumCulled = false
    const tint = new THREE.Color()
    for (let i = 0; i < MAX_PETALS; i++)
      this.petals.setColorAt(i, tint.set(COLORS[i % COLORS.length]))
    this.group.add(this.petals)
    this.group.visible = false
  }

  /** `center` is the area's middle on the floor. */
  update(elapsed: number, center: THREE.Vector3, area: PetalArea | null) {
    this.group.visible = true
    this.group.position.copy(center)
    const squareMetres = area
      ? area.halfWidth * area.halfDepth * 4
      : Math.PI * RADIUS ** 2
    const petals = Math.min(
      MAX_PETALS,
      Math.round(squareMetres * PER_SQUARE_METRE)
    )
    const cos = area ? Math.cos(area.yaw) : 1
    const sin = area ? Math.sin(area.yaw) : 0
    let count = 0
    for (let i = 0; i < petals; i++) {
      const height = petalHeight(i, elapsed)
      if (height < 0) continue
      const angle = hash(i, 4) * Math.PI * 2
      let x = Math.cos(angle) * Math.sqrt(hash(i, 3)) * RADIUS
      let z = Math.sin(angle) * Math.sqrt(hash(i, 3)) * RADIUS
      if (area) {
        const localX = (hash(i, 3) * 2 - 1) * area.halfWidth
        const localZ = (hash(i, 4) * 2 - 1) * area.halfDepth
        x = localX * cos + localZ * sin
        z = localZ * cos - localX * sin
      }
      const sway = elapsed * (1.1 + hash(i, 5)) + hash(i, 6) * 6.28
      this.dummy.position.set(
        x + Math.sin(sway) * 0.3,
        height,
        z + Math.cos(sway * 0.8) * 0.3
      )
      this.dummy.rotation.set(
        sway * 1.7,
        sway * 0.9 + angle,
        Math.sin(sway) * 1.2
      )
      this.dummy.scale.setScalar(Math.min(1, height / SHRINK_BELOW))
      this.dummy.updateMatrix()
      this.petals.setMatrixAt(count++, this.dummy.matrix)
    }
    this.petals.count = count
    this.petals.instanceMatrix.needsUpdate = true
  }

  clear() {
    this.group.visible = false
  }

  dispose() {
    this.petals.dispose()
    this.geometry.dispose()
    this.material.dispose()
    this.group.removeFromParent()
  }
}
