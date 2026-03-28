import {
  isCardinalMoveBlocked,
  isMovementBlocked,
  getFloorYBase,
  type RuntimePassability,
} from './housing-passability'

export interface PathWaypoint {
  x: number
  z: number
  floor: number
}

export interface PathResult {
  waypoints: PathWaypoint[]
  found: boolean
}

// 4-directional neighbors: dx, dz
const DIRS: ReadonlyArray<[number, number]> = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
]

const FLOOR_STRIDE = 4 // max 4 floor levels (0-3)
const Z_STRIDE = (16384 * 2 + 1) * FLOOR_STRIDE // 131076

function cellKey(x: number, z: number, floor: number): number {
  return (x + 16384) * Z_STRIDE + (z + 16384) * FLOOR_STRIDE + floor
}

// --- Binary min-heap keyed on f-value ---

interface HeapNode {
  x: number
  z: number
  floor: number
  g: number
  f: number
}

class MinHeap {
  private data: HeapNode[] = []

  get size() {
    return this.data.length
  }

  push(node: HeapNode) {
    this.data.push(node)
    this.bubbleUp(this.data.length - 1)
  }

  pop(): HeapNode | undefined {
    const top = this.data[0]
    const last = this.data.pop()
    if (this.data.length > 0 && last) {
      this.data[0] = last
      this.sinkDown(0)
    }
    return top
  }

  private bubbleUp(i: number) {
    const d = this.data
    while (i > 0) {
      const p = (i - 1) >> 1
      if (d[i].f >= d[p].f) break
      ;[d[i], d[p]] = [d[p], d[i]]
      i = p
    }
  }

  private sinkDown(i: number) {
    const d = this.data
    const n = d.length
    while (true) {
      let smallest = i
      const l = 2 * i + 1
      const r = 2 * i + 2
      if (l < n && d[l].f < d[smallest].f) smallest = l
      if (r < n && d[r].f < d[smallest].f) smallest = r
      if (smallest === i) break
      ;[d[i], d[smallest]] = [d[smallest], d[i]]
      i = smallest
    }
  }
}

// --- A* pathfinding ---

interface ClosedEntry {
  g: number
  parentX: number
  parentZ: number
  parentFloor: number
}

/**
 * Find a path on a virtual 1m world grid with floor-level awareness.
 * Uses floor-specific edge checks and stairwell transitions between floors.
 */
export function findPath(
  startX: number,
  startZ: number,
  startFloor: number,
  goalX: number,
  goalZ: number,
  goalFloor: number,
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  heightSampler: (x: number, z: number) => number,
  maxNodes = 200
): PathResult {
  const sx = Math.floor(startX)
  const sz = Math.floor(startZ)
  const gx = Math.floor(goalX)
  const gz = Math.floor(goalZ)

  if (sx === gx && sz === gz && startFloor === goalFloor) {
    return {
      waypoints: [{ x: goalX, z: goalZ, floor: goalFloor }],
      found: true,
    }
  }

  const open = new MinHeap()
  const closed = new Map<number, ClosedEntry>()

  const h = (x: number, z: number, floor: number) =>
    Math.abs(x - gx) + Math.abs(z - gz) + Math.abs(floor - goalFloor) * 2

  const startKey = cellKey(sx, sz, startFloor)
  open.push({ x: sx, z: sz, floor: startFloor, g: 0, f: h(sx, sz, startFloor) })
  closed.set(startKey, {
    g: 0,
    parentX: sx,
    parentZ: sz,
    parentFloor: startFloor,
  })

  let bestH = h(sx, sz, startFloor)
  let bestX = sx
  let bestZ = sz
  let bestFloor = startFloor
  let expanded = 0

  while (open.size > 0 && expanded < maxNodes) {
    const cur = open.pop()!
    expanded++

    if (cur.x === gx && cur.z === gz && cur.floor === goalFloor) {
      return {
        waypoints: reconstructPath(
          closed,
          sx,
          sz,
          startFloor,
          gx,
          gz,
          goalFloor,
          goalX,
          goalZ
        ),
        found: true,
      }
    }

    const curKey = cellKey(cur.x, cur.z, cur.floor)
    const entry = closed.get(curKey)
    if (entry && cur.g > entry.g) continue

    // Cardinal neighbors on same floor
    for (const [dx, dz] of DIRS) {
      const nx = cur.x + dx
      const nz = cur.z + dz
      const nKey = cellKey(nx, nz, cur.floor)

      const newG = cur.g + 1
      const existing = closed.get(nKey)
      if (existing && existing.g <= newG) continue

      // Water check (only on ground floor)
      if (cur.floor === 0 && heightSampler(nx + 0.5, nz + 0.5) < 0) continue

      if (
        isCardinalMoveBlocked(passabilityCache, cur.x, cur.z, dx, dz, cur.floor)
      )
        continue

      closed.set(nKey, {
        g: newG,
        parentX: cur.x,
        parentZ: cur.z,
        parentFloor: cur.floor,
      })
      const nH = h(nx, nz, cur.floor)
      open.push({ x: nx, z: nz, floor: cur.floor, g: newG, f: newG + nH })

      if (nH < bestH) {
        bestH = nH
        bestX = nx
        bestZ = nz
        bestFloor = cur.floor
      }
    }

    // Stairwell transitions
    for (const rp of passabilityCache.values()) {
      if (
        cur.x < rp.minX ||
        cur.x >= rp.maxX ||
        cur.z < rp.minZ ||
        cur.z >= rp.maxZ
      )
        continue
      for (const stair of rp.stairwells) {
        const localX = cur.x - rp.houseOriginX
        const localZ = cur.z - rp.houseOriginZ
        if (
          localX < stair.localMinX ||
          localX >= stair.localMaxX ||
          localZ < stair.localMinZ ||
          localZ >= stair.localMaxZ
        )
          continue

        let targetFloor: number | undefined
        if (cur.floor === stair.lowerFloor) targetFloor = stair.upperFloor
        else if (cur.floor === stair.upperFloor) targetFloor = stair.lowerFloor

        if (targetFloor === undefined) continue

        const nKey = cellKey(cur.x, cur.z, targetFloor)
        const newG = cur.g + 2 // extra cost for floor transition
        const existing = closed.get(nKey)
        if (existing && existing.g <= newG) continue

        closed.set(nKey, {
          g: newG,
          parentX: cur.x,
          parentZ: cur.z,
          parentFloor: cur.floor,
        })
        const nH = h(cur.x, cur.z, targetFloor)
        open.push({
          x: cur.x,
          z: cur.z,
          floor: targetFloor,
          g: newG,
          f: newG + nH,
        })

        if (nH < bestH) {
          bestH = nH
          bestX = cur.x
          bestZ = cur.z
          bestFloor = targetFloor
        }
      }
    }
  }

  // Partial path to closest node
  if (bestX !== sx || bestZ !== sz || bestFloor !== startFloor) {
    return {
      waypoints: reconstructPath(
        closed,
        sx,
        sz,
        startFloor,
        bestX,
        bestZ,
        bestFloor,
        bestX + 0.5,
        bestZ + 0.5
      ),
      found: false,
    }
  }

  return { waypoints: [], found: false }
}

function reconstructPath(
  closed: Map<number, ClosedEntry>,
  sx: number,
  sz: number,
  sFloor: number,
  ex: number,
  ez: number,
  eFloor: number,
  finalX: number,
  finalZ: number
): PathWaypoint[] {
  const path: PathWaypoint[] = []
  let cx = ex
  let cz = ez
  let cf = eFloor

  while (cx !== sx || cz !== sz || cf !== sFloor) {
    path.push({ x: cx + 0.5, z: cz + 0.5, floor: cf })
    const entry = closed.get(cellKey(cx, cz, cf))
    if (!entry) break
    cx = entry.parentX
    cz = entry.parentZ
    cf = entry.parentFloor
  }

  path.reverse()

  if (path.length > 0) {
    const last = path[path.length - 1]
    path[path.length - 1] = { x: finalX, z: finalZ, floor: last.floor }
  }

  return path
}

/**
 * Greedy line-of-sight path smoothing.
 * Only smooths within the same floor level.
 */
export function smoothPath(
  waypoints: PathWaypoint[],
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  heightSampler: (x: number, z: number) => number
): PathWaypoint[] {
  if (waypoints.length <= 2) return waypoints

  const result: PathWaypoint[] = [waypoints[0]]
  let anchor = 0

  while (anchor < waypoints.length - 1) {
    let farthest = anchor + 1

    for (let probe = anchor + 2; probe < waypoints.length; probe++) {
      // Don't smooth across floor transitions
      if (waypoints[probe].floor !== waypoints[anchor].floor) break
      if (
        isLinePassable(
          waypoints[anchor],
          waypoints[probe],
          passabilityCache,
          heightSampler
        )
      ) {
        farthest = probe
      } else {
        break
      }
    }

    result.push(waypoints[farthest])
    anchor = farthest
  }

  return result
}

/** Check if a straight line between two same-floor points is passable. */
function isLinePassable(
  from: PathWaypoint,
  to: PathWaypoint,
  passabilityCache: ReadonlyMap<string, RuntimePassability>,
  heightSampler: (x: number, z: number) => number
): boolean {
  const dx = to.x - from.x
  const dz = to.z - from.z
  const dist = Math.sqrt(dx * dx + dz * dz)
  const steps = Math.ceil(dist / 0.5)

  // Determine Y for this floor level
  const floorY =
    from.floor > 0
      ? getFloorYBase(passabilityCache, from.x, from.z, from.floor)
      : undefined
  const y = floorY ?? heightSampler(from.x, from.z)

  for (let i = 1; i <= steps; i++) {
    const t = i / steps
    const mx = from.x + dx * t
    const mz = from.z + dz * t
    const prevT = (i - 1) / steps
    const px = from.x + dx * prevT
    const pz = from.z + dz * prevT

    if (from.floor === 0 && heightSampler(mx, mz) < 0) return false
    if (isMovementBlocked(passabilityCache, px, pz, mx, mz, y)) return false
  }

  return true
}
