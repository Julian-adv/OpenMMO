import * as THREE from 'three'
import type { RiverSegment } from './river-data'
import type { TerrainHeightManager } from '../managers/terrainHeightManager'

/**
 * Water surface offset (m) above the carved channel bed. The bake carves
 * the heightmap down to the river floor, so placing the ribbon a fixed
 * amount above the sampled ground puts the water at roughly the rim of
 * the plain elevation regardless of flow.
 */
const RIVER_DEPTH_OFFSET_M = 0.6

/**
 * Below this cosine of the interior angle between two adjacent segments,
 * the miter extension explodes (a perfect 180° reversal divides by zero).
 * Clamp to avoid vertex spikes; visible as a small bevel at sharp turns.
 */
const MIN_MITER_COSINE = 0.25

interface Endpoint {
  seg: number
  forward: boolean
}

interface ChainLink {
  seg: number
  forward: boolean
}

/** Endpoints are shared across tile files; the baker preserves float
 *  bit patterns on both sides of the midpoint-ownership split, so
 *  equal-precision decimal keys match bit-for-bit. */
function endpointKey(x: number, z: number): string {
  return `${x.toFixed(3)},${z.toFixed(3)}`
}

function normalizedDelta(
  x1: number,
  z1: number,
  x2: number,
  z2: number
): [number, number] {
  const dx = x2 - x1
  const dz = z2 - z1
  const len = Math.hypot(dx, dz)
  if (len < 1e-6) return [0, 0]
  return [dx / len, dz / len]
}

function buildChains(segs: RiverSegment[]): ChainLink[][] {
  if (segs.length === 0) return []

  const byEndpoint = new Map<string, Endpoint[]>()
  const push = (k: string, ep: Endpoint) => {
    const list = byEndpoint.get(k)
    if (list) list.push(ep)
    else byEndpoint.set(k, [ep])
  }
  for (let i = 0; i < segs.length; i++) {
    push(endpointKey(segs[i].ax, segs[i].az), { seg: i, forward: true })
    push(endpointKey(segs[i].bx, segs[i].bz), { seg: i, forward: false })
  }

  // Chain tips are endpoints of degree 1, interior of degree 2. Junctions
  // (degree ≥ 3) split the chain so each branch is its own polyline.
  const visited = new Array<boolean>(segs.length).fill(false)
  const chains: ChainLink[][] = []

  const walk = (startSeg: number, startForward: boolean) => {
    const chain: ChainLink[] = []
    let curSeg = startSeg
    let curForward = startForward
    while (!visited[curSeg]) {
      visited[curSeg] = true
      chain.push({ seg: curSeg, forward: curForward })
      const s = segs[curSeg]
      const nextKey = curForward
        ? endpointKey(s.bx, s.bz)
        : endpointKey(s.ax, s.az)
      const candidates = byEndpoint.get(nextKey) ?? []
      let next: Endpoint | null = null
      let nextCount = 0
      for (const ep of candidates) {
        if (ep.seg === curSeg) continue
        nextCount++
        if (!visited[ep.seg]) next = ep
      }
      if (nextCount !== 1 || !next) break
      curSeg = next.seg
      curForward = !next.forward
    }
    if (chain.length > 0) chains.push(chain)
  }

  for (const list of byEndpoint.values()) {
    if (list.length !== 1) continue
    const { seg, forward } = list[0]
    if (!visited[seg]) walk(seg, forward)
  }
  for (let i = 0; i < segs.length; i++) {
    if (!visited[i]) walk(i, true)
  }
  return chains
}

export interface RiverGeometryResult {
  geometry: THREE.BufferGeometry
  vertexCount: number
}

/**
 * Build a BufferGeometry for a tile's river ribbons. Produces a triangle
 * strip per chain with mitered joins. Vertex attributes:
 *
 * - `position` (vec3): world-space, Y from heightmap + offset.
 * - `uv` (vec2): U = cross-ribbon (0 left … 1 right), V = cumulative chain
 *   length (meters) for texture scrolling.
 * - `flowDir` (vec2): segment-local tangent (XZ normalized).
 * - `flowNorm` (float): per-vertex normalized flow (0..1).
 * - `edgeDist` (float): 0 at centerline, 1 at either bank.
 */
export function buildRiverGeometry(
  segments: RiverSegment[],
  heightManager: TerrainHeightManager | null
): RiverGeometryResult {
  const chains = buildChains(segments)

  const positions: number[] = []
  const uvs: number[] = []
  const flowDirs: number[] = []
  const flowNorms: number[] = []
  const edgeDists: number[] = []
  const indices: number[] = []

  const sampleY = (x: number, z: number): number => {
    if (!heightManager) return 0
    return heightManager.getHeightAtWorldPosition(x, z) + RIVER_DEPTH_OFFSET_M
  }

  for (const chain of chains) {
    if (chain.length === 0) continue

    const n = chain.length
    const px = new Array<number>(n + 1)
    const pz = new Array<number>(n + 1)
    const widths = new Array<number>(n + 1)
    const flows = new Array<number>(n + 1)
    for (let i = 0; i < n; i++) {
      const link = chain[i]
      const s = segments[link.seg]
      const ax = link.forward ? s.ax : s.bx
      const az = link.forward ? s.az : s.bz
      const bx = link.forward ? s.bx : s.ax
      const bz = link.forward ? s.bz : s.az
      const wa = link.forward ? s.widthA : s.widthB
      const wb = link.forward ? s.widthB : s.widthA
      const fa = link.forward ? s.flowNormA : s.flowNormB
      const fb = link.forward ? s.flowNormB : s.flowNormA
      if (i === 0) {
        px[0] = ax
        pz[0] = az
        widths[0] = wa
        flows[0] = fa
      }
      px[i + 1] = bx
      pz[i + 1] = bz
      widths[i + 1] = wb
      flows[i + 1] = fb
    }

    const baseVertex = positions.length / 3
    let cumulativeLen = 0
    for (let i = 0; i <= n; i++) {
      const [pTx, pTz] =
        i > 0 ? normalizedDelta(px[i - 1], pz[i - 1], px[i], pz[i]) : [0, 0]
      const [nTx, nTz] =
        i < n ? normalizedDelta(px[i], pz[i], px[i + 1], pz[i + 1]) : [0, 0]
      const tx = i === 0 ? nTx : i === n ? pTx : (pTx + nTx) * 0.5
      const tz = i === 0 ? nTz : i === n ? pTz : (pTz + nTz) * 0.5
      const tLen = Math.hypot(tx, tz)
      const txN = tLen > 1e-6 ? tx / tLen : 0
      const tzN = tLen > 1e-6 ? tz / tLen : 1
      const nx = -tzN
      const nz = txN

      // Miter extension = half-width / cos(theta/2). Clamp so 180°
      // reversals don't spike vertex positions to infinity.
      let miter = 1
      if (i > 0 && i < n) {
        const dot = pTx * nTx + pTz * nTz
        const cosHalf = Math.sqrt(Math.max(0, (1 + dot) * 0.5))
        if (cosHalf > MIN_MITER_COSINE) miter = 1 / cosHalf
      }

      const halfWidth = widths[i] * 0.5 * miter
      const leftX = px[i] + nx * halfWidth
      const leftZ = pz[i] + nz * halfWidth
      const rightX = px[i] - nx * halfWidth
      const rightZ = pz[i] - nz * halfWidth

      if (i > 0) {
        cumulativeLen += Math.hypot(px[i] - px[i - 1], pz[i] - pz[i - 1])
      }
      const v = cumulativeLen

      positions.push(leftX, sampleY(leftX, leftZ), leftZ)
      uvs.push(0, v)
      flowDirs.push(txN, tzN)
      flowNorms.push(flows[i])
      edgeDists.push(1)
      positions.push(rightX, sampleY(rightX, rightZ), rightZ)
      uvs.push(1, v)
      flowDirs.push(txN, tzN)
      flowNorms.push(flows[i])
      edgeDists.push(1)
    }

    for (let i = 0; i < n; i++) {
      const a = baseVertex + 2 * i
      const b = baseVertex + 2 * i + 1
      const c = baseVertex + 2 * (i + 1)
      const d = baseVertex + 2 * (i + 1) + 1
      indices.push(a, b, c)
      indices.push(b, d, c)
    }
  }

  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(positions, 3)
  )
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2))
  geometry.setAttribute(
    'flowDir',
    new THREE.Float32BufferAttribute(flowDirs, 2)
  )
  geometry.setAttribute(
    'flowNorm',
    new THREE.Float32BufferAttribute(flowNorms, 1)
  )
  geometry.setAttribute(
    'edgeDist',
    new THREE.Float32BufferAttribute(edgeDists, 1)
  )
  geometry.setIndex(indices)
  geometry.computeBoundingSphere()
  geometry.computeBoundingBox()

  return { geometry, vertexCount: positions.length / 3 }
}
