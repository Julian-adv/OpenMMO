import * as THREE from 'three'
import {
  TILE_DIM,
  VERTS_PER_SIDE,
  PADDED_SIDE,
  tileKey,
  decodeHeight,
  type TerrainHeightState,
} from './terrain-height-types'

const _paddedHeights = new Float32Array(PADDED_SIDE * PADDED_SIDE) // reusable buffer
const EDGE_NEIGHBORS = [
  { dx: -1, dz: 0 },
  { dx: 0, dz: -1 },
  { dx: -1, dz: -1 },
] as const

export function getHeightAtCell(
  state: TerrainHeightState,
  tileX: number,
  tileZ: number,
  cellX: number,
  cellZ: number
): number {
  if (cellX >= VERTS_PER_SIDE) {
    return getHeightAtCell(state, tileX + 1, tileZ, cellX - TILE_DIM, cellZ)
  }
  if (cellZ >= VERTS_PER_SIDE) {
    return getHeightAtCell(state, tileX, tileZ + 1, cellX, cellZ - TILE_DIM)
  }
  if (cellX < 0) {
    return getHeightAtCell(state, tileX - 1, tileZ, cellX + TILE_DIM, cellZ)
  }
  if (cellZ < 0) {
    return getHeightAtCell(state, tileX, tileZ - 1, cellX, cellZ + TILE_DIM)
  }

  const data = state.heightmaps.get(tileKey(tileX, tileZ))
  if (!data) return 0
  return decodeHeight(data[cellZ * VERTS_PER_SIDE + cellX])
}

export function applyHeightToGeometry(
  state: TerrainHeightState,
  tileX: number,
  tileZ: number,
  geometry: THREE.BufferGeometry
) {
  const data = state.heightmaps.get(tileKey(tileX, tileZ))
  if (!data) return

  const posAttr = geometry.getAttribute('position') as THREE.BufferAttribute
  const positions = posAttr.array as Float32Array
  const normalAttr = geometry.getAttribute('normal') as THREE.BufferAttribute
  const normals = normalAttr.array as Float32Array

  const P = PADDED_SIDE
  const heights = _paddedHeights

  // Fill 65x65 vertices directly from heightmap data
  for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
    const srcRow = vz * VERTS_PER_SIDE
    const dstRow = (vz + 1) * P + 1
    for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
      heights[dstRow + vx] = decodeHeight(data[srcRow + vx])
    }
  }

  // Padding edges for normal computation at boundaries
  for (let i = 0; i < VERTS_PER_SIDE; i++) {
    heights[(i + 1) * P] = getHeightAtCell(state, tileX, tileZ, -1, i)
    heights[(i + 1) * P + (P - 1)] = getHeightAtCell(
      state,
      tileX,
      tileZ,
      VERTS_PER_SIDE,
      i
    )
    heights[i + 1] = getHeightAtCell(state, tileX, tileZ, i, -1)
    heights[(P - 1) * P + (i + 1)] = getHeightAtCell(
      state,
      tileX,
      tileZ,
      i,
      VERTS_PER_SIDE
    )
  }
  // Four padding corners
  heights[0] = getHeightAtCell(state, tileX, tileZ, -1, -1)
  heights[P - 1] = getHeightAtCell(state, tileX, tileZ, VERTS_PER_SIDE, -1)
  heights[(P - 1) * P] = getHeightAtCell(
    state,
    tileX,
    tileZ,
    -1,
    VERTS_PER_SIDE
  )
  heights[(P - 1) * P + (P - 1)] = getHeightAtCell(
    state,
    tileX,
    tileZ,
    VERTS_PER_SIDE,
    VERTS_PER_SIDE
  )

  // Set positions and compute analytical normals via central differences
  for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
    for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
      const vertexIndex = vz * VERTS_PER_SIDE + vx
      const pi = (vz + 1) * P + (vx + 1)

      const h = heights[pi]
      positions[vertexIndex * 3 + 1] = h

      const dhdx = heights[pi + 1] - heights[pi - 1]
      const dhdz = heights[pi + P] - heights[pi - P]

      const nx = -dhdx
      const ny = 2.0
      const nz = -dhdz
      const invLen = 1.0 / Math.sqrt(nx * nx + ny * ny + nz * nz)
      normals[vertexIndex * 3] = nx * invLen
      normals[vertexIndex * 3 + 1] = ny * invLen
      normals[vertexIndex * 3 + 2] = nz * invLen
    }
  }

  posAttr.needsUpdate = true
  normalAttr.needsUpdate = true

  // Bounding sphere still reflects the flat plane until recomputed; Mesh.raycast
  // uses it for early-reject, so isometric rays miss the elevated mesh otherwise.
  geometry.computeBoundingSphere()
}

export function refreshAdjacentTileEdges(
  state: TerrainHeightState,
  tileX: number,
  tileZ: number
) {
  const data = state.heightmaps.get(tileKey(tileX, tileZ))
  if (!data) return

  // Sync overlapping edge data to neighbors
  const leftData = state.heightmaps.get(tileKey(tileX - 1, tileZ))
  if (leftData) {
    for (let vz = 0; vz < VERTS_PER_SIDE; vz++) {
      leftData[vz * VERTS_PER_SIDE + TILE_DIM] = data[vz * VERTS_PER_SIDE + 0]
    }
  }

  const topData = state.heightmaps.get(tileKey(tileX, tileZ - 1))
  if (topData) {
    for (let vx = 0; vx < VERTS_PER_SIDE; vx++) {
      topData[TILE_DIM * VERTS_PER_SIDE + vx] = data[0 * VERTS_PER_SIDE + vx]
    }
  }

  const diagData = state.heightmaps.get(tileKey(tileX - 1, tileZ - 1))
  if (diagData) {
    diagData[TILE_DIM * VERTS_PER_SIDE + TILE_DIM] = data[0]
  }

  // Re-apply geometry for neighbors whose data was updated
  for (const { dx, dz } of EDGE_NEIGHBORS) {
    const nx = tileX + dx
    const nz = tileZ + dz
    const key = tileKey(nx, nz)
    const geo = state.geometries.get(key)
    if (geo && state.heightmaps.has(key)) {
      applyHeightToGeometry(state, nx, nz, geo)
    }
  }
}
