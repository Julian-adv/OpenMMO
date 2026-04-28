import * as THREE from 'three'

export interface FootprintRect {
  minX: number
  minZ: number
  maxX: number
  maxZ: number
}

export interface FootprintData {
  /** Ground-contact rectangles, model-local XZ coords. */
  rects: FootprintRect[]
  /** Lowest Y of the model in its local frame; offset from placement.y to the pier-bottom world Y. */
  minLocalY: number
}

const GRID_STEP = 1
/** Triangles within this distance of the model's minY count as ground contact. */
const Y_EPSILON = 0.3

/**
 * Auto-detect ground-contact footprint by rasterizing triangles near the
 * model's minY into a 1m grid, then extracting axis-aligned rects via
 * 4-connected component grouping. Returns rects in model-local XZ coords —
 * one per pier/abutment.
 */
export function detectFootprint(model: THREE.Object3D): FootprintData {
  const box = new THREE.Box3().setFromObject(model)
  const minY = box.min.y
  const cells = new Set<string>()

  const v0 = new THREE.Vector3()
  const v1 = new THREE.Vector3()
  const v2 = new THREE.Vector3()

  const meshBox = new THREE.Box3()
  model.updateMatrixWorld(true)
  model.traverse((child) => {
    if (!(child instanceof THREE.Mesh)) return
    const geom = child.geometry as THREE.BufferGeometry
    const pos = geom.attributes.position as THREE.BufferAttribute
    if (!pos) return

    if (!geom.boundingBox) geom.computeBoundingBox()
    meshBox.copy(geom.boundingBox!).applyMatrix4(child.matrixWorld)
    if (meshBox.min.y > minY + Y_EPSILON) return

    const idx = geom.index
    const triCount = idx ? Math.floor(idx.count / 3) : Math.floor(pos.count / 3)

    for (let t = 0; t < triCount; t++) {
      const i0 = idx ? idx.getX(t * 3) : t * 3
      const i1 = idx ? idx.getX(t * 3 + 1) : t * 3 + 1
      const i2 = idx ? idx.getX(t * 3 + 2) : t * 3 + 2

      v0.fromBufferAttribute(pos, i0).applyMatrix4(child.matrixWorld)
      v1.fromBufferAttribute(pos, i1).applyMatrix4(child.matrixWorld)
      v2.fromBufferAttribute(pos, i2).applyMatrix4(child.matrixWorld)

      const triMinY = Math.min(v0.y, v1.y, v2.y)
      if (triMinY > minY + Y_EPSILON) continue

      const triMinX = Math.floor(Math.min(v0.x, v1.x, v2.x) / GRID_STEP)
      const triMaxX = Math.ceil(Math.max(v0.x, v1.x, v2.x) / GRID_STEP)
      const triMinZ = Math.floor(Math.min(v0.z, v1.z, v2.z) / GRID_STEP)
      const triMaxZ = Math.ceil(Math.max(v0.z, v1.z, v2.z) / GRID_STEP)

      for (let z = triMinZ; z < triMaxZ; z++) {
        for (let x = triMinX; x < triMaxX; x++) {
          cells.add(`${x},${z}`)
        }
      }
    }
  })

  return { rects: extractRects(cells), minLocalY: minY }
}

function extractRects(cells: Set<string>): FootprintRect[] {
  const rects: FootprintRect[] = []
  while (cells.size > 0) {
    const start = cells.values().next().value!
    cells.delete(start)
    const [sx, sz] = start.split(',').map(Number)
    let minX = sx
    let maxX = sx
    let minZ = sz
    let maxZ = sz
    const stack: [number, number][] = [[sx, sz]]
    while (stack.length) {
      const [x, z] = stack.pop()!
      for (const [dx, dz] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        const nx = x + dx
        const nz = z + dz
        const k = `${nx},${nz}`
        if (cells.has(k)) {
          cells.delete(k)
          stack.push([nx, nz])
          if (nx < minX) minX = nx
          if (nx > maxX) maxX = nx
          if (nz < minZ) minZ = nz
          if (nz > maxZ) maxZ = nz
        }
      }
    }
    rects.push({
      minX,
      minZ,
      maxX: maxX + GRID_STEP,
      maxZ: maxZ + GRID_STEP,
    })
  }
  return rects
}

/**
 * Rotate a rect's AABB by a 90° step around the model origin (matches
 * THREE.Object3D rotation.y semantics: positive angle rotates +Z toward +X).
 */
export function rotateRect(
  r: FootprintRect,
  rotationDeg: number
): FootprintRect {
  const rot = ((rotationDeg % 360) + 360) % 360
  switch (rot) {
    case 90:
      return { minX: r.minZ, maxX: r.maxZ, minZ: -r.maxX, maxZ: -r.minX }
    case 180:
      return { minX: -r.maxX, maxX: -r.minX, minZ: -r.maxZ, maxZ: -r.minZ }
    case 270:
      return { minX: -r.maxZ, maxX: -r.minZ, minZ: r.minX, maxZ: r.maxX }
    default:
      return r
  }
}
