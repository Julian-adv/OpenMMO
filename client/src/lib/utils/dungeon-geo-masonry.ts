import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import type { GeoEntry } from './house-geo-utils'
import { HOUSING_TEXTURES } from './housing-textures'
import { DUNGEON_FLOOR_UV_SCALE, WALL_THICKNESS } from './dungeon-geo-constants'
import { quadMeshBuilder } from './dungeon-geo-primitives'

const BRICK_WIDTH = 4 / 9
const BRICK_HEIGHT = 2 / 9
const JOINT_INSET = 0.008
const TILE_SIZE = 1 / (4 * DUNGEON_FLOOR_UV_SCALE)
const TILES_PER_CELL = Math.round(1 / TILE_SIZE)
const DAMAGE_LIFT = 0.006
const SOIL_TEXTURE = HOUSING_TEXTURES.findIndex(
  (entry) => entry.glb === 'red_laterite_soil_stones_1k'
)
const BRICK_FACES = [
  [0.004, 0.009, 0.495, 0.246],
  [0.506, 0.009, 0.995, 0.246],
  [0.129, 0.258, 0.554, 0.495],
  [0.561, 0.258, 0.933, 0.495],
  [0.004, 0.505, 0.495, 0.749],
  [0.506, 0.505, 0.995, 0.749],
  [0.129, 0.758, 0.554, 0.992],
  [0.561, 0.758, 0.933, 0.992],
]

function noise(seed: number, x: number, y: number) {
  let hash = seed ^ Math.imul(x, 374761393) ^ Math.imul(y, 668265263)
  hash = Math.imul(hash ^ (hash >>> 13), 1274126177)
  return ((hash ^ (hash >>> 16)) >>> 0) / 4294967296
}

function addWhiteVertexColors(geo: THREE.BufferGeometry) {
  geo.setAttribute(
    'color',
    new THREE.BufferAttribute(
      new Float32Array(geo.getAttribute('position').count * 3).fill(1),
      3
    )
  )
}

export function buildMasonryWallGhost(
  alongX: boolean,
  lo: number,
  hi: number,
  boundary: number,
  height: number
): THREE.BufferGeometry {
  const surface = quadMeshBuilder(0.45)
  const point = (along: number, y: number) =>
    new THREE.Vector3(alongX ? along : boundary, y, alongX ? boundary : along)
  for (const side of [-1, 1]) {
    surface.addQuad(
      point(lo, 0),
      point(hi, 0),
      point(hi, height),
      point(lo, height),
      new THREE.Vector3(alongX ? 0 : side, 0, alongX ? side : 0)
    )
  }
  const entries: GeoEntry[] = []
  surface.finish(entries, 0)
  const geo = entries[0].geo
  addWhiteVertexColors(geo)
  return geo
}

export function buildMasonryWall(
  alongX: boolean,
  lo: number,
  hi: number,
  boundary: number,
  inward: number,
  height: number,
  seed: number
): THREE.BufferGeometry {
  const geos: THREE.BufferGeometry[] = []
  const backing = new THREE.BoxGeometry(hi - lo, height, WALL_THICKNESS)
  const backingIndices = Array.from(backing.getIndex()!.array)
  backing.setIndex([
    ...backingIndices.slice(0, 24),
    ...backingIndices.slice(30),
  ])
  backing.translate((lo + hi) / 2, height / 2, -WALL_THICKNESS / 2)
  const backingPositions = backing.getAttribute('position')
  const backingNormals = backing.getAttribute('normal')
  const backingUVs = backing.getAttribute('uv')
  for (let i = 0; i < backingPositions.count; i++) {
    const along = backingPositions.getX(i)
    const across = boundary + inward * backingPositions.getZ(i)
    const y = backingPositions.getY(i)
    const top = Math.abs(backingNormals.getY(i)) > 0.5
    const end = Math.abs(backingNormals.getX(i)) > 0.5
    backingUVs.setXY(
      i,
      (top ? (alongX ? along : across) : end ? across : along) * 0.45,
      (top ? (alongX ? across : along) : y) * 0.45
    )
  }
  geos.push(backing)
  const mortar = quadMeshBuilder()
  const addJoint = (x0: number, x1: number, y0: number, y1: number) => {
    if (x1 <= x0 || y1 <= y0) return
    mortar.addQuad(
      new THREE.Vector3(x0, y0, 0),
      new THREE.Vector3(x1, y0, 0),
      new THREE.Vector3(x1, y1, 0),
      new THREE.Vector3(x0, y1, 0),
      new THREE.Vector3(0, 0, 1)
    )
  }

  const wallSeed = seed ^ Math.round(boundary * 7919) ^ (alongX ? 8191 : 0)
  for (let row = 0; row * BRICK_HEIGHT < height; row++) {
    const bottom = row * BRICK_HEIGHT
    const top = Math.min(height, (row + 1) * BRICK_HEIGHT)
    const shift = (row % 2) * BRICK_WIDTH * 0.5
    for (
      let column = Math.floor((lo - shift) / BRICK_WIDTH);
      column * BRICK_WIDTH + shift < hi;
      column++
    ) {
      const start = column * BRICK_WIDTH + shift
      const left = Math.max(lo, start)
      const right = Math.min(hi, start + BRICK_WIDTH)
      if (right - left < 0.000001) continue
      const edge = left === lo || right === hi || top === height || row === 0
      const sample = noise(wallSeed, column, row)
      const depth = edge
        ? 0
        : sample < 0.02
          ? -0.012
          : sample > 0.96
            ? 0.015 + ((sample - 0.96) / 0.04) * 0.025
            : 0
      const insetX = Math.min(JOINT_INSET, (right - left) / 4)
      const insetY = Math.min(JOINT_INSET, (top - bottom) / 4)
      const x0 = left + (left === lo ? 0 : insetX)
      const x1 = right - (right === hi ? 0 : insetX)
      const y0 = bottom + (row === 0 ? 0 : insetY)
      const y1 = top - (top === height ? 0 : insetY)
      const brick =
        depth === 0
          ? new THREE.PlaneGeometry(x1 - x0, y1 - y0)
          : new THREE.BoxGeometry(x1 - x0, y1 - y0, 1)
      if (depth !== 0)
        brick.setIndex(Array.from(brick.getIndex()!.array).slice(0, 30))
      brick.translate((x0 + x1) / 2, (y0 + y1) / 2, 0)
      const positions = brick.getAttribute('position')
      const normals = brick.getAttribute('normal')
      const uv = brick.getAttribute('uv')
      const face =
        BRICK_FACES[
          Math.floor(noise(wallSeed + 1, column, row) * BRICK_FACES.length)
        ]
      const mirrored = noise(wallSeed + 2, column, row) > 0.5
      const shade = 0.78 + noise(wallSeed + 3, column, row) * 0.2
      const warmth = (noise(wallSeed + 4, column, row) - 0.5) * 0.04
      const colors = new THREE.BufferAttribute(
        new Float32Array(positions.count * 3),
        3
      )
      for (let i = 0; i < positions.count; i++) {
        colors.setXYZ(i, shade + warmth, shade, shade - warmth)
        positions.setZ(i, positions.getZ(i) > 0 ? depth : 0)
        const u = THREE.MathUtils.clamp(
          (positions.getX(i) - start - JOINT_INSET) /
            (BRICK_WIDTH - 2 * JOINT_INSET),
          0,
          1
        )
        const v = THREE.MathUtils.clamp(
          (positions.getY(i) - row * BRICK_HEIGHT - JOINT_INSET) /
            (BRICK_HEIGHT - 2 * JOINT_INSET),
          0,
          1
        )
        const side = Math.abs(normals.getZ(i)) < 0.5
        const faceU = mirrored ? 1 - u : u
        uv.setXY(
          i,
          (face[0] + (face[2] - face[0]) * (side ? faceU * 0.2 : faceU)) / 2,
          (1 - face[3] + (face[3] - face[1]) * v) / 2
        )
      }
      brick.setAttribute('color', colors)
      geos.push(brick)
      addJoint(left, x0, bottom, top)
      addJoint(x1, right, bottom, top)
      addJoint(x0, x1, bottom, y0)
      addJoint(x0, x1, y1, top)
    }
  }
  const mortarEntries: GeoEntry[] = []
  mortar.finish(mortarEntries, 0)
  const mortarUVs = mortarEntries[0].geo.getAttribute('uv')
  for (let i = 0; i < mortarUVs.count; i++) mortarUVs.setXY(i, 0.235, 0.375)
  geos.push(mortarEntries[0].geo)

  for (const part of geos) {
    if (!part.hasAttribute('color')) addWhiteVertexColors(part)
  }
  const geo = mergeGeometries(geos, false)!
  const backingCount = backing.getIndex()!.count
  geo.addGroup(0, backingCount, 1)
  geo.addGroup(backingCount, geo.getIndex()!.count - backingCount, 0)
  for (const part of geos) part.dispose()
  const positions = geo.getAttribute('position')
  for (let i = 0; i < positions.count; i++) {
    const along = positions.getX(i)
    const across = boundary + inward * positions.getZ(i)
    positions.setXYZ(
      i,
      alongX ? along : across,
      positions.getY(i),
      alongX ? across : along
    )
  }
  if ((alongX && inward < 0) || (!alongX && inward > 0)) {
    const indices = geo.getIndex()!
    for (let i = 0; i < indices.count; i += 3) {
      const first = indices.getX(i)
      indices.setX(i, indices.getX(i + 2))
      indices.setX(i + 2, first)
    }
  }
  geo.computeVertexNormals()
  geo.computeBoundingBox()
  return geo
}

export function masonryFloorDamageBuilder(
  textureIndex: number,
  seed: number,
  clear: (x: number, z: number) => boolean
) {
  const entries: GeoEntry[] = []
  const sides = quadMeshBuilder(DUNGEON_FLOOR_UV_SCALE)
  const soil = quadMeshBuilder(1.5)
  const v = (x: number, y: number, z: number) => new THREE.Vector3(x, y, z)
  const tile = (points: THREE.Vector2[], lift = 0.012, tilt = 0) => {
    const cx = points.reduce((sum, p) => sum + p.x, 0) / points.length
    const top = (p: THREE.Vector2) => lift + (p.x - cx) * tilt
    const geo = new THREE.ShapeGeometry(
      new THREE.Shape(points.map((p) => new THREE.Vector2(p.x, -p.y)))
    )
    geo.rotateX(-Math.PI / 2)
    const positions = geo.getAttribute('position')
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < positions.count; i++) {
      positions.setY(i, lift + (positions.getX(i) - cx) * tilt)
      uv.setXY(
        i,
        positions.getX(i) * DUNGEON_FLOOR_UV_SCALE,
        positions.getZ(i) * DUNGEON_FLOOR_UV_SCALE
      )
    }
    geo.computeVertexNormals()
    entries.push({ geo, textureIndex })
    for (let i = 0; i < points.length; i++) {
      const a = points[i]
      const b = points[(i + 1) % points.length]
      sides.addQuad(
        v(a.x, top(a), a.y),
        v(b.x, top(b), b.y),
        v(b.x, DAMAGE_LIFT, b.y),
        v(a.x, DAMAGE_LIFT, a.y),
        v(b.y - a.y, 0, a.x - b.x).normalize()
      )
    }
  }
  const addCell = (x: number, z: number) => {
    for (let dz = 0; dz < TILES_PER_CELL; dz++) {
      for (let dx = 0; dx < TILES_PER_CELL; dx++) {
        const tx = x * TILES_PER_CELL + dx
        const tz = z * TILES_PER_CELL + dz
        const x0 = tx * TILE_SIZE
        const z0 = tz * TILE_SIZE
        const patch = noise(seed + 11, Math.floor(tx / 3), Math.floor(tz / 3))
        const damage = noise(seed, tx, tz)
        const threshold = patch > 0.7 ? 0.42 : 0.1
        if (damage >= threshold) continue
        const canBreak = [0.005, TILE_SIZE - 0.005].every((u) =>
          [0.005, TILE_SIZE - 0.005].every((w) => clear(x0 + u, z0 + w))
        )
        if (!canBreak) continue
        const turns = Math.floor(noise(seed + 7, tx, tz) * 4)
        const points = (coords: number[][]) =>
          coords.map(([u, w]) => {
            for (let i = 0; i < turns; i++) [u, w] = [1 - w, u]
            return new THREE.Vector2(x0 + u * TILE_SIZE, z0 + w * TILE_SIZE)
          })
        soil.addQuad(
          v(x0, DAMAGE_LIFT, z0),
          v(x0 + TILE_SIZE, DAMAGE_LIFT, z0),
          v(x0 + TILE_SIZE, DAMAGE_LIFT, z0 + TILE_SIZE),
          v(x0, DAMAGE_LIFT, z0 + TILE_SIZE),
          v(0, 1, 0)
        )
        if (damage < threshold * 0.35) {
          tile(
            points([
              [0.08, 0.1],
              [0.36, 0.13],
              [0.13, 0.34],
            ]),
            0.012,
            0.03
          )
        } else if (damage < threshold * 0.7) {
          tile(
            points([
              [0, 0],
              [1, 0],
              [1, 0.32],
              [0.68, 0.48],
              [0.44, 0.72],
              [0, 1],
            ])
          )
          tile(
            points([
              [0.68, 0.81],
              [0.91, 0.72],
              [0.87, 0.94],
            ]),
            0.018,
            0.04
          )
        } else {
          tile(
            points([
              [0, 0],
              [1, 0],
              [1, 0.37],
              [0.54, 0.57],
              [0, 0.42],
            ])
          )
          tile(
            points([
              [0, 0.46],
              [0.54, 0.61],
              [1, 0.41],
              [1, 1],
              [0, 1],
            ]),
            0.02,
            0.035
          )
        }
      }
    }
  }
  const finish = (target: GeoEntry[]) => {
    if (entries.length === 0) return
    sides.finish(entries, textureIndex)
    soil.finish(entries, SOIL_TEXTURE)
    target.push(...entries)
  }
  return { addCell, finish }
}
