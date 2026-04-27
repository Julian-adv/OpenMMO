import * as THREE from 'three'
import type { RiverSegment } from './river-data'
import type { TerrainHeightManager } from '../managers/terrainHeightManager'
import { SEA_LEVEL } from '../components/game-scene/terrain-utils'

/**
 * Y for ribbon vertices in the sea-extension segment. Sits just above the
 * sea shader's surface (SEA_LEVEL=0) so the extension delta can't sink
 * below it regardless of the carved seafloor depth — heightmap sampling
 * would drop Y with the seabed and stamp a hollow "underwater geometry"
 * look near the mouth. 2 cm of clearance dodges z-fighting with the sea
 * quad while staying visually coplanar with the sea at this scale.
 */
const SEA_EXTEND_SURFACE_Y = SEA_LEVEL + 0.02

/**
 * Water surface offset (m) above the carved channel bed. The bake carves
 * the heightmap down to the river floor, so placing the ribbon a fixed
 * amount above the sampled ground puts the water at roughly the rim of
 * the plain elevation regardless of flow.
 */
export const RIVER_DEPTH_OFFSET_M = 0.5

/**
 * Scale applied to baked surface widths so the water ribbon covers the
 * gravel/sand band carved around the channel. Purely render-side; the bake
 * still uses the unscaled widths for terrain carving.
 */
const RIVER_WIDTH_SCALE = 1.5

/**
 * Extra meters added to each side of the ribbon on top of the scaled width,
 * so thin and wide rivers alike pick up a consistent "sits over the bank"
 * margin. With the depth-based edge fade in `river-material.ts`, fragments
 * past the carve taper sample natural ground (depth ≈ 0 → α 0) and
 * automatically read as transparent, so widening the pad just exposes
 * more soft-fade band without painting over dry land. 2m = +4m total
 * width — leaves room for the taper gradient even on the smallest
 * tributaries.
 */
const RIVER_WIDTH_PAD_M = 1.0

/**
 * Estuary alpha fade window (meters of surface Y). Below LOW the ribbon is
 * fully faded (vertex sits in open sea water), above HIGH it renders
 * unchanged. Between the two the `mouthFactor` attribute ramps from 1→0
 * using smoothstep, which the river shader multiplies into alpha so the
 * ribbon dissolves into the sea quad instead of terminating in a hard edge.
 *
 * Window is pushed seaward so the fade completes in deeper water rather
 * than at the shoreline. With the sea-extension step (`SEA_EXTEND_*`) the
 * ribbon has vertices well past the original polyline tip at negative
 * bed Y, so a LOW of −0.3 m (bed ≈ −0.9 m) lands the final alpha inside
 * that extension and keeps the visible river flowing out onto the water
 * rather than ending at the surf line.
 */
const MOUTH_FADE_Y_LOW = -0.7
const MOUTH_FADE_Y_HIGH = 0.4

/**
 * Chains whose last vertex sits below this surface Y are extended past
 * the Phase-4 polyline tip into the sea by `SEA_EXTEND_METERS`, added in
 * `SEA_EXTEND_STEPS` uniform steps. Each step scales width by
 * `1 - k/STEPS` so the extension tapers from full (fanned) width at the
 * polyline tip back to zero at its own tip — the land-side polyline
 * opens into a 부채꼴 via bake-time `RIVER_MOUTH_FAN_EXTRA`, then narrows
 * back into the sea here, producing the symmetric spindle-shaped delta
 * centered on the coastline. Also ensures the sea shader's foam-
 * suppression radius covers the whole extension.
 */
const SEA_EXTEND_TRIGGER_Y = 0.6
const SEA_EXTEND_METERS = 16
const SEA_EXTEND_STEPS = 8

/**
 * Mouth-region alpha ramp tuning. The wedge alpha follows a bell curve:
 *   • Polyline approach [n0 - SEA_RAMP_STEPS .. n0]: ramp from the
 *     upstream-sampled yMouthFactor toward fully opaque (alpha 1).
 *   • Wedge [n0+1 .. n0+SEA_EXTEND_STEPS]: ramp from opaque back to
 *     transparent over WEDGE_FADE_STEPS, with the remaining steps
 *     parked in the α=0 dead zone (per residual-edge memo, a width-
 *     tapered tip with smooth fade-to-zero leaves a thin dark seam).
 */
const SEA_RAMP_STEPS = 3
const WEDGE_FADE_STEPS = 5

/**
 * Below this cosine of the interior angle between two adjacent segments,
 * the miter extension explodes (a perfect 180° reversal divides by zero).
 * Clamp to avoid vertex spikes; visible as a small bevel at sharp turns.
 */
const MIN_MITER_COSINE = 0.25

/**
 * Inside-corner halfWidth cap as a fraction of the local radius of
 * curvature R = L_segment / (2·sin(θ_half)). At halfWidth ≥ R consecutive
 * inside-bank vertices converge at the center of curvature and the strip
 * self-intersects (triangle flip + flow inversion). 0.4–0.8 trades
 * obvious bend pinching for residual overlap; 0.6 sits comfortably on
 * smooth curves without over-narrowing tight bends.
 */
const RIVER_BEND_SAFETY = 0.7

/** Half-width (in vertices) of the cubic-taper window that eases the
 *  inside-bank cap from each tight vertex to its neighbors, so the bend
 *  pinch fades out instead of snapping back to full width. */
const BEND_CAP_SMOOTH_RADIUS = 10

/** Laplacian (0.25/0.5/0.25) smoothing passes on the centerline. Endpoints
 *  stay anchored — multi-pass ghost-point smoothing diverges because each
 *  tile only sees one ghost per seam, so after pass 1 the neighbor's
 *  interior has moved but the local ghost still points at its pre-smooth
 *  value, drifting the two sides of the seam apart. */
const CENTERLINE_SMOOTH_PASSES = 8

interface Endpoint {
  seg: number
  forward: boolean
}

interface ChainLink {
  seg: number
  forward: boolean
}

interface FlowUvBasis {
  dirX: number
  dirZ: number
  offset: number
}

/** Squared 3D distance between two vertices in a flat positions array
 *  (3 floats per vertex). Used by the strip-diagonal picker. */
function distSq3(positions: number[], i: number, j: number): number {
  const ai = i * 3
  const aj = j * 3
  const dx = positions[ai] - positions[aj]
  const dy = positions[ai + 1] - positions[aj + 1]
  const dz = positions[ai + 2] - positions[aj + 2]
  return dx * dx + dy * dy + dz * dz
}

/** Endpoints are shared across tile files; the baker preserves float
 *  bit patterns on both sides of the midpoint-ownership split, so
 *  equal-precision decimal keys match bit-for-bit. */
export function endpointKey(x: number, z: number): string {
  return `${x.toFixed(3)},${z.toFixed(3)}`
}

/** GLSL-style Hermite smoothstep on the JS side. Used by the mouth-fade
 *  alpha math; matches the curve TSL would compute on the GPU side so
 *  baked vertex factors and shader-side ramps interpolate the same way. */
function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
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
      curForward = next.forward
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
 * - `flowUvDir` (vec2), `flowUvOffset` (float): per-triangle planar
 *   downstream UV basis. The geometry is expanded to non-indexed
 *   triangles so these values stay constant across each triangle; the
 *   shader uses `dot(worldXZ, flowUvDir) + flowUvOffset` for ripple V.
 * - `flowNorm` (float): per-vertex normalized flow (0..1).
 * - `edgeDist` (float): 0 at centerline, 1 at either bank.
 * - `mouthFactor` (float): 1 where the vertex sits at sea level, 0 inland;
 *   drives the estuary alpha fade in the shader. See MOUTH_FADE_Y_*.
 * - `centerlineXZ` (vec2): world XZ of the chain centerline at this
 *   vertex's cross-section (same on left/right). The shader uses this to
 *   compare the river surface against the centerline bed height for a
 *   stable depth fade.
 *
 * `externalContinuations` supplies the neighbor tile's adjacent-segment
 * other-endpoint for each shared seam point, keyed by `endpointKey`. When
 * a chain tip sits on a tile seam the ghost point lets the tangent
 * averaging span the split — without it, each tile treats its own end as
 * a hard endpoint and computes a tangent from its inward segment only.
 * The two tiles then bevel the ribbon differently at the same centerline
 * point, leaving a visible gap/kink ~1 m wide along one bank — the "cut"
 * the player sees when standing at the seam.
 */
export function buildRiverGeometry(
  segments: RiverSegment[],
  heightManager: TerrainHeightManager | null,
  externalContinuations?: ReadonlyMap<string, [number, number]>
): RiverGeometryResult {
  const chains = buildChains(segments)

  // Per-endpoint segment degree. Used at the smoothing pass to detect
  // junction tips (degree ≥ 3) and anchor them like tile-seam tips.
  const segmentDegree = new Map<string, number>()
  for (const s of segments) {
    const ka = endpointKey(s.ax, s.az)
    const kb = endpointKey(s.bx, s.bz)
    segmentDegree.set(ka, (segmentDegree.get(ka) ?? 0) + 1)
    segmentDegree.set(kb, (segmentDegree.get(kb) ?? 0) + 1)
  }

  const positions: number[] = []
  const uvs: number[] = []
  const flowDirs: number[] = []
  const flowNorms: number[] = []
  const edgeDists: number[] = []
  const mouthFactors: number[] = []
  // Signed perpendicular distance from the centerline at each vertex
  // (−halfWidth at left, +halfWidth at right). Interpolates linearly
  // within each triangle to the true world cross-distance of the
  // fragment — unlike `uv.x ∈ [0,1]` whose world-space gradient flips
  // direction between the two triangles of a rapidly flaring segment
  // (wide-fan diagonal), stamping opposite-flowing normal-map scroll
  // per triangle at the mouth. Feeding this to the shader's meshUV.x
  // keeps cross-channel texture frequency uniform in world meters.
  const crossMeters: number[] = []
  // World XZ of the chain centerline at this vertex's cross-section
  // (px[i], pz[i]). Both left/right vertices share it.
  const centerlineXZs: number[] = []
  const indices: number[] = []
  const flowUvBases: FlowUvBasis[] = []

  const sampleY = (x: number, z: number): number => {
    if (!heightManager) return 0
    return heightManager.getHeightAtWorldPosition(x, z) + RIVER_DEPTH_OFFSET_M
  }

  for (const chain of chains) {
    if (chain.length === 0) continue

    const n0 = chain.length
    const px: number[] = new Array<number>(n0 + 1)
    const pz: number[] = new Array<number>(n0 + 1)
    const widths: number[] = new Array<number>(n0 + 1)
    const flows: number[] = new Array<number>(n0 + 1)
    for (let i = 0; i < n0; i++) {
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

    // `buildChains` walks from degree-1 endpoints without regard to flow,
    // so ~half of all chains come out oriented upstream. Reverse the arrays
    // in place so index 0 is always the headwater and index n is the mouth;
    // this makes `flowDir` (segment tangent) point downstream consistently.
    if (flows[n0] < flows[0]) {
      px.reverse()
      pz.reverse()
      widths.reverse()
      flows.reverse()
    }

    // Ghost points — the neighbor tile's continuation of the chain past
    // each chain tip that sits on a tile seam. In the vertex loop the
    // tangent at i=0 / i=n is averaged against the ghost so both tiles
    // bevel the ribbon identically at the shared centerline. Tail ghost
    // also signals "not a real mouth" so the sea extension below skips
    // — otherwise both tiles would paint overlapping 16m deltas from
    // the same shared point.
    const headKey = n0 >= 1 ? endpointKey(px[0], pz[0]) : ''
    const tailKey = n0 >= 1 ? endpointKey(px[n0], pz[n0]) : ''
    const ghostPrev =
      headKey !== '' ? (externalContinuations?.get(headKey) ?? null) : null
    const ghostNextSeam =
      tailKey !== '' ? (externalContinuations?.get(tailKey) ?? null) : null

    // Anchor not only the chain tip but also the tip's immediate neighbor
    // when the tip is shared (tile-seam ghost or junction): sibling chains
    // see that neighbor's pre-smooth position and would tear the ribbon if
    // we moved it.
    const headIsJunction = (segmentDegree.get(headKey) ?? 0) >= 3
    const tailIsJunction = (segmentDegree.get(tailKey) ?? 0) >= 3
    if (n0 >= 2) {
      const startInterior =
        ghostPrev !== null || headIsJunction ? 2 : 1
      const endInterior =
        ghostNextSeam !== null || tailIsJunction ? n0 - 1 : n0
      if (startInterior < endInterior) {
        const tmpX = new Array<number>(n0 + 1)
        const tmpZ = new Array<number>(n0 + 1)
        for (let pass = 0; pass < CENTERLINE_SMOOTH_PASSES; pass++) {
          for (let i = startInterior; i < endInterior; i++) {
            tmpX[i] = 0.25 * px[i - 1] + 0.5 * px[i] + 0.25 * px[i + 1]
            tmpZ[i] = 0.25 * pz[i - 1] + 0.5 * pz[i] + 0.25 * pz[i + 1]
          }
          for (let i = startInterior; i < endInterior; i++) {
            px[i] = tmpX[i]
            pz[i] = tmpZ[i]
          }
        }
      }
    }

    // Extend the ribbon past the polyline tip into the sea when the chain
    // terminates below sea level, so the alpha fade has room to blend to
    // zero and the sea shader's foam-suppression radius covers the delta
    // itself, not just the carved channel. Keeps extension widths equal
    // to the last segment so the ribbon reads as a uniform sea-bound
    // delta instead of a tapering point.
    let n = n0
    if (
      n0 >= 1 &&
      ghostNextSeam === null &&
      sampleY(px[n0], pz[n0]) < SEA_EXTEND_TRIGGER_Y
    ) {
      const [exTx, exTz] = normalizedDelta(
        px[n0 - 1],
        pz[n0 - 1],
        px[n0],
        pz[n0]
      )
      for (let k = 1; k <= SEA_EXTEND_STEPS; k++) {
        const t = k / SEA_EXTEND_STEPS
        const d = SEA_EXTEND_METERS * t
        // Linear width taper: extension tip reaches zero width. The
        // land-side polyline already fans open via bake-time
        // `RIVER_MOUTH_FAN_EXTRA`; tapering back here produces a
        // symmetric spindle centered on the coastline instead of a
        // one-sided flag.
        const widthScale = 1 - t
        px.push(px[n0] + exTx * d)
        pz.push(pz[n0] + exTz * d)
        widths.push(widths[n0] * widthScale)
        flows.push(flows[n0])
        n++
      }
    }

    // Anchor the mouth ramp on a vertex a few steps upstream of n0 so
    // its alpha sits outside the y-fade range — sampling at n0 itself
    // (or n0-1) would inherit the near-sea-level alpha and the wedge
    // would never reach a fully opaque crest.
    const seaExtended = n > n0
    const mouthSourceI = Math.max(0, n0 - SEA_RAMP_STEPS)
    const mouthSourceFactor = seaExtended
      ? 1 -
        smoothstep(
          MOUTH_FADE_Y_LOW,
          MOUTH_FADE_Y_HIGH,
          sampleY(px[mouthSourceI], pz[mouthSourceI])
        )
      : 0

    // Per-vertex inside-bank cap and bend direction, hoisted so the
    // smoothing pass below can window over the result. Bounded to the real
    // polyline (i ≤ n0); extension vertices stay at the array's Infinity/0
    // defaults so the main loop reads them as "no cap, straight".
    const rawCaps: number[] = new Array(n + 1).fill(Infinity)
    const rawCrossSigns: number[] = new Array(n + 1).fill(0)
    for (let i = 0; i <= n0; i++) {
      const prevPt = i > 0 ? ([px[i - 1], pz[i - 1]] as const) : ghostPrev
      const nextPt = i < n0 ? ([px[i + 1], pz[i + 1]] as const) : ghostNextSeam
      if (!prevPt || !nextPt) continue
      const [pTx, pTz] = normalizedDelta(prevPt[0], prevPt[1], px[i], pz[i])
      const [nTx, nTz] = normalizedDelta(px[i], pz[i], nextPt[0], nextPt[1])
      const dot = pTx * nTx + pTz * nTz
      const sinHalf = Math.sqrt(Math.max(0, (1 - dot) * 0.5))
      if (sinHalf <= 1e-3) continue
      const Lprev = Math.hypot(prevPt[0] - px[i], prevPt[1] - pz[i])
      const Lnext = Math.hypot(nextPt[0] - px[i], nextPt[1] - pz[i])
      const radiusOfCurvature = Math.min(Lprev, Lnext) / (2 * sinHalf)
      rawCaps[i] = RIVER_BEND_SAFETY * radiusOfCurvature
      rawCrossSigns[i] = Math.sign(pTx * nTz - pTz * nTx)
    }

    // Cubic-taper smoothing: each bend broadcasts cap/pull(|k|/N) to
    // neighbors and the vertex takes the min, so caps ease into bends
    // along a smoothstep curve rather than snapping at the apex.
    const smoothedCaps: number[] = new Array(n + 1).fill(Infinity)
    const smoothedCrossSigns: number[] = new Array(n + 1).fill(0)
    for (let i = 0; i <= n0; i++) {
      let bestCap = Infinity
      let bestSign = rawCrossSigns[i]
      for (let k = -BEND_CAP_SMOOTH_RADIUS; k <= BEND_CAP_SMOOTH_RADIUS; k++) {
        const j = i + k
        if (j < 0 || j > n0) continue
        const cj = rawCaps[j]
        if (!Number.isFinite(cj)) continue
        const t = Math.abs(k) / BEND_CAP_SMOOTH_RADIUS
        const pull = 1 - smoothstep(0, 1, t)
        if (pull < 1e-6) continue
        const candidate = cj / pull
        if (candidate < bestCap) {
          bestCap = candidate
          bestSign = rawCrossSigns[j]
        }
      }
      smoothedCaps[i] = bestCap
      smoothedCrossSigns[i] = bestSign
    }

    const baseVertex = positions.length / 3
    let cumulativeLen = 0
    for (let i = 0; i <= n; i++) {
      // Prev/next polyline neighbor, falling back to the seam ghost at
      // chain tips. `ghostNextSeam` is null when a sea extension was
      // applied (mutex via the extension guard), so extension-tip vertices
      // correctly land here as a true endpoint.
      const prevPt = i > 0 ? ([px[i - 1], pz[i - 1]] as const) : ghostPrev
      const nextPt = i < n ? ([px[i + 1], pz[i + 1]] as const) : ghostNextSeam
      const hasPrev = prevPt !== null
      const hasNext = nextPt !== null
      const [pTx, pTz] = prevPt
        ? normalizedDelta(prevPt[0], prevPt[1], px[i], pz[i])
        : [0, 0]
      const [nTx, nTz] = nextPt
        ? normalizedDelta(px[i], pz[i], nextPt[0], nextPt[1])
        : [0, 0]
      // Average both tangents at interior vertices; a single-sided tip
      // uses whichever tangent exists (the absent one stays [0,0]).
      const avgScale = hasPrev && hasNext ? 0.5 : 1
      const tx = avgScale * (pTx + nTx)
      const tz = avgScale * (pTz + nTz)
      const tLen = Math.hypot(tx, tz)
      const txN = tLen > 1e-6 ? tx / tLen : 0
      const tzN = tLen > 1e-6 ? tz / tLen : 1
      const nx = -tzN
      const nz = txN

      // Miter extension = half-width / cos(theta/2). Clamp so 180°
      // reversals don't spike vertex positions to infinity.
      let miter = 1
      if (hasPrev && hasNext) {
        const dot = pTx * nTx + pTz * nTz
        const cosHalf = Math.sqrt(Math.max(0, (1 + dot) * 0.5))
        if (cosHalf > MIN_MITER_COSINE) miter = 1 / cosHalf
      }
      // crossSign codes the bend direction so only the inside bank gets
      // capped: +1 LEFT turn (inside = +nx), -1 RIGHT, 0 straight.
      const halfWidthCap = smoothedCaps[i]
      const crossSign = smoothedCrossSigns[i]

      // Widths already carry the estuary fan scaling from the bake (see
      // `apply_mouth_fan_widths` in shared/worldgen/tile_bake/context.rs),
      // so heightmap carve and splat sand band widen in lockstep with this
      // ribbon — applying any extra scale here would make the water plane
      // overhang the carved banks.
      const requestedHalfWidth =
        widths[i] * 0.5 * RIVER_WIDTH_SCALE + RIVER_WIDTH_PAD_M
      const miteredHalfWidth = requestedHalfWidth * miter
      // The bend-safety cap describes the final distance from the
      // centerline after mitering. Applying it before the miter lets a
      // sharp enough corner multiply the "safe" inside bank back past the
      // cap, which can reintroduce skinny flipped-looking triangles near
      // the bend tip.
      const insideHalfWidth = Math.min(miteredHalfWidth, halfWidthCap)
      const outsideHalfWidth = miteredHalfWidth
      const leftHalfWidth = crossSign > 0 ? insideHalfWidth : outsideHalfWidth
      const rightHalfWidth = crossSign < 0 ? insideHalfWidth : outsideHalfWidth

      // Extension vertices ride a fixed sea-surface Y — following the
      // heightmap out past the polyline tip drags the ribbon down with
      // the carved seafloor (e.g. Y ≈ −2 m over a continental shelf),
      // so the delta geometry dives underwater even though its alpha
      // fades to zero. Lock to `SEA_EXTEND_SURFACE_Y` (just above the
      // sea shader) so the ribbon sits on the sea surface like a real
      // estuary plume.
      const centerY = i > n0 ? SEA_EXTEND_SURFACE_Y : sampleY(px[i], pz[i])

      if (i > 0) {
        cumulativeLen += Math.hypot(px[i] - px[i - 1], pz[i] - pz[i - 1])
      }
      const v = cumulativeLen

      // `centerY` is sampled at the centerline and reused for both bank
      // vertices — sampling at each bank instead makes the ribbon rise
      // with the terrain outside the carved channel (ribbon buries into
      // hillsides going upstream) or bows if carve depth varies across
      // the width.

      // Inverted so 1 = mouth (transparent), 0 = inland (opaque).
      const yMouthFactor =
        1 - smoothstep(MOUTH_FADE_Y_LOW, MOUTH_FADE_Y_HIGH, centerY)
      // Two-stage mouth ramp: smoothstep upstream-sample → 0 across the
      // polyline approach, then 0 → 1 across the wedge. Outside the
      // mouth region, fall back to the natural y-fade.
      let mouthFactor: number
      if (!seaExtended || i < mouthSourceI) {
        mouthFactor = yMouthFactor
      } else if (i <= n0) {
        const rampT = (i - mouthSourceI) / (n0 - mouthSourceI)
        mouthFactor = mouthSourceFactor * (1 - smoothstep(0, 1, rampT))
      } else {
        const wedgeT = Math.min(1, (i - n0) / WEDGE_FADE_STEPS)
        mouthFactor = smoothstep(0, 1, wedgeT)
      }

      const leftX = px[i] + nx * leftHalfWidth
      const leftZ = pz[i] + nz * leftHalfWidth
      const rightX = px[i] - nx * rightHalfWidth
      const rightZ = pz[i] - nz * rightHalfWidth

      positions.push(leftX, centerY, leftZ)
      uvs.push(0, v)
      flowDirs.push(txN, tzN)
      flowNorms.push(flows[i])
      edgeDists.push(1)
      mouthFactors.push(mouthFactor)
      crossMeters.push(-leftHalfWidth)
      centerlineXZs.push(px[i], pz[i])
      positions.push(rightX, centerY, rightZ)
      uvs.push(1, v)
      flowDirs.push(txN, tzN)
      flowNorms.push(flows[i])
      edgeDists.push(1)
      mouthFactors.push(mouthFactor)
      crossMeters.push(rightHalfWidth)
      centerlineXZs.push(px[i], pz[i])
    }

    for (let i = 0; i < n; i++) {
      const a = baseVertex + 2 * i
      const b = baseVertex + 2 * i + 1
      const c = baseVertex + 2 * (i + 1)
      const d = baseVertex + 2 * (i + 1) + 1

      const quad = [a, b, c, d]
      let dirX = 0
      let dirZ = 0
      for (const vi of quad) {
        const fi = vi * 2
        dirX += flowDirs[fi]
        dirZ += flowDirs[fi + 1]
      }
      const dirLen = Math.hypot(dirX, dirZ)
      if (dirLen > 1e-6) {
        dirX /= dirLen
        dirZ /= dirLen
      } else {
        dirX = 0
        dirZ = 1
      }

      let offset = 0
      for (const vi of quad) {
        const pi = vi * 3
        const ui = vi * 2
        offset +=
          uvs[ui + 1] - (positions[pi] * dirX + positions[pi + 2] * dirZ)
      }
      offset /= quad.length
      const flowUvBasis = { dirX, dirZ, offset }

      // Pick shorter diagonal so per-triangle Y gradient stays symmetric
      // across bend direction. A fixed diagonal traverses the wide side
      // of the trapezoid on one curve direction and amplifies depth
      // mismatch through the 5 cm depthEdgeCut.
      if (distSq3(positions, b, c) <= distSq3(positions, a, d)) {
        indices.push(a, b, c)
        indices.push(b, d, c)
      } else {
        indices.push(a, b, d)
        indices.push(a, d, c)
      }
      flowUvBases.push(flowUvBasis, flowUvBasis)
    }
  }

  // Expand the indexed strip into independent triangles so each triangle
  // can carry a constant planar flow basis. Using interpolated per-vertex
  // tangents/anchors in the shader made long skinny bend triangles locally
  // invert or over-scale the V gradient; a triangle-constant basis keeps
  // downstream direction and apparent ripple speed stable.
  const expandedPositions: number[] = []
  const expandedUvs: number[] = []
  const expandedFlowDirs: number[] = []
  const expandedFlowUvDirs: number[] = []
  const expandedFlowUvOffsets: number[] = []
  const expandedFlowNorms: number[] = []
  const expandedEdgeDists: number[] = []
  const expandedMouthFactors: number[] = []
  const expandedCrossMeters: number[] = []
  const expandedCenterlineXZs: number[] = []

  const pushVertex = (
    i: number,
    flowUvDirX: number,
    flowUvDirZ: number,
    flowUvOffset: number
  ) => {
    const pi = i * 3
    const ui = i * 2
    expandedPositions.push(positions[pi], positions[pi + 1], positions[pi + 2])
    expandedUvs.push(uvs[ui], uvs[ui + 1])
    expandedFlowDirs.push(flowDirs[ui], flowDirs[ui + 1])
    expandedFlowUvDirs.push(flowUvDirX, flowUvDirZ)
    expandedFlowUvOffsets.push(flowUvOffset)
    expandedFlowNorms.push(flowNorms[i])
    expandedEdgeDists.push(edgeDists[i])
    expandedMouthFactors.push(mouthFactors[i])
    expandedCrossMeters.push(crossMeters[i])
    expandedCenterlineXZs.push(centerlineXZs[ui], centerlineXZs[ui + 1])
  }

  for (let ii = 0; ii < indices.length; ii += 3) {
    const ia = indices[ii]
    const ib = indices[ii + 1]
    const ic = indices[ii + 2]
    const { dirX, dirZ, offset } = flowUvBases[ii / 3]

    pushVertex(ia, dirX, dirZ, offset)
    pushVertex(ib, dirX, dirZ, offset)
    pushVertex(ic, dirX, dirZ, offset)
  }

  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(expandedPositions, 3)
  )
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(expandedUvs, 2))
  geometry.setAttribute(
    'flowDir',
    new THREE.Float32BufferAttribute(expandedFlowDirs, 2)
  )
  geometry.setAttribute(
    'flowUvDir',
    new THREE.Float32BufferAttribute(expandedFlowUvDirs, 2)
  )
  geometry.setAttribute(
    'flowUvOffset',
    new THREE.Float32BufferAttribute(expandedFlowUvOffsets, 1)
  )
  geometry.setAttribute(
    'flowNorm',
    new THREE.Float32BufferAttribute(expandedFlowNorms, 1)
  )
  geometry.setAttribute(
    'edgeDist',
    new THREE.Float32BufferAttribute(expandedEdgeDists, 1)
  )
  geometry.setAttribute(
    'mouthFactor',
    new THREE.Float32BufferAttribute(expandedMouthFactors, 1)
  )
  geometry.setAttribute(
    'crossMeters',
    new THREE.Float32BufferAttribute(expandedCrossMeters, 1)
  )
  geometry.setAttribute(
    'centerlineXZ',
    new THREE.Float32BufferAttribute(expandedCenterlineXZs, 2)
  )
  geometry.computeBoundingSphere()
  geometry.computeBoundingBox()

  return { geometry, vertexCount: expandedPositions.length / 3 }
}
