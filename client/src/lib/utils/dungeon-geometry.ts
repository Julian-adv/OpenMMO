/**
 * dungeon-geometry.ts — procedural mesh building for dungeon floors.
 *
 * Follows the housing pattern: collect GeoEntry quads/boxes per texture
 * index, merge into one mesh per texture (addMergedMeshes), reuse the
 * shared housing materials so no new WebGPU pipelines are compiled.
 *
 * Conventions (must mirror shared/src/dungeon):
 * - Group origin sits at (originX, floorY(depth), originZ); all geometry
 *   is local. Local y=0 is this floor's walking surface.
 * - No ceiling on underground floors: the isometric camera looks down ~35°,
 *   any current-floor ceiling would fully occlude the player. The void reads
 *   as cave dark. (The surface entrance is the one exception — it carries a
 *   gravel roof, always shown at depth 0; the player only descends the stairs,
 *   never stands inside, so it never needs to hide.)
 * - Camera-facing walls (south/west boundaries — solid at z+1 / x-1) are
 *   not emitted at all, mirroring housing's hidden "front" group: the
 *   player is always inside a dungeon.
 * - Stair shafts render both directions per floor: the up shaft you
 *   arrived by (rising to +floorHeight) and the down shaft (descending
 *   to -floorHeight). Adjacent floors build the identical world-space
 *   boxes for the shared shaft, so switching the rendered floor at the
 *   shaft midpoint is seamless.
 */
import * as THREE from 'three'
import { mergeVertices } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import {
  addMergedMeshes,
  bakedGeo,
  HOUSING_TEXTURES,
  type GeoEntry,
} from './house-geo-utils'
import { getHousingMaterial } from './housing-textures'
import {
  ENTRANCE_WALL_T,
  shaftCoverRun,
  type DungeonFloorLayout,
  type DungeonShaft,
} from '../managers/dungeonManager'

export const DUNGEON_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
)
/** Mossy plaster for the *surface* entrance building walls — distinct from the
 *  underground stone walls (DUNGEON_WALL_TEXTURE_IDX). */
export const DUNGEON_ENTRANCE_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/worn_mossy_plasterwall_1k'
)
export const DUNGEON_FLOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/grey_stone_path_1k'
)
export const DUNGEON_VOID_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.label === 'Void'
)
export const DUNGEON_CHEST_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/dark_wooden_planks_1k'
)
/** Grey roof tiles for the surface entrance roof. */
export const DUNGEON_CEILING_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/grey_roof_tiles_02_1k'
)
/** Stone blocks for the decorative entrance corner pillars (accent against the
 *  mossy-plaster entrance walls). */
export const DUNGEON_PILLAR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
)

const SLAB_THICKNESS = 0.15
/** Flat landing cells at shaft ends — must match dungeonManager.rampY. */
const LANDING_CELLS = 1.0
const STEP_RISE = 0.25
/** UV scale for the dungeon floor/stairs texture: <1 enlarges the stone pattern
 *  (one repeat spans 1/scale metres) to cut the visible tiling. Dungeon-only —
 *  housing bakes its own UVs, so the shared texture is unaffected there. */
const DUNGEON_FLOOR_UV_SCALE = 0.5

/** Wooden garage-door texture for the entrance door (mapped 0→1 across it). */
const DUNGEON_DOOR_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'wooden_garage_door_1k'
)
/** Door panel thickness (m). */
const DOOR_THICKNESS = 0.12
/** Door apex height above the entry ground (under the ABOVE=3.0 doorway). */
const DOOR_HEIGHT = 2.85
/** Height where the rectangular body ends and the pentagonal cap begins. */
const DOOR_SHOULDER = 1.9

export interface DungeonGeoCtx {
  grid: number
  /** Wall visual height (matches shared DUNGEON_WALL_HEIGHT). */
  wallHeight: number
  /** Vertical distance between floors (shared DUNGEON_FLOOR_HEIGHT). */
  floorHeight: number
  shaftW: number
  shaftLen: number
}

/** Box with housing-style face UVs derived from final (baked) position.
 *  `uvScale` <1 enlarges the texture pattern (fewer repeats); callers pass it
 *  for the floor/stairs, everything else tiles 1:1 in metres. */
function addBox(
  entries: GeoEntry[],
  textureIndex: number,
  w: number,
  h: number,
  d: number,
  cx: number,
  cy: number,
  cz: number,
  uvScale: number = 1
) {
  const geo = new THREE.BoxGeometry(w, h, d)
  const uv = geo.getAttribute('uv')
  const pos = geo.getAttribute('position')
  for (let vi = 0; vi < pos.count; vi++) {
    const px = pos.getX(vi) + cx
    const py = pos.getY(vi) + cy
    const pz = pos.getZ(vi) + cz
    const face = Math.floor(vi / 4)
    if (face <= 1) {
      uv.setXY(vi, pz, py) // ±X faces
    } else if (face <= 3) {
      uv.setXY(vi, px, pz) // ±Y faces
    } else {
      uv.setXY(vi, px, py) // ±Z faces
    }
  }
  // bakedGeo applies uvScale to every UV (its uvScaleX/Y params).
  entries.push({
    geo: bakedGeo(geo, cx, cy, cz, 0, uvScale, uvScale),
    textureIndex,
  })
}

const _roofMat = new THREE.Matrix4()

/**
 * Gabled (맞배지붕) roof over a rectangular footprint, centered at (cx, cz).
 * The ridge runs along the run axis (the long, `runLen` direction); the two
 * slopes face the lateral (`latW`) sides and a triangular gable closes each
 * run-axis end. Eaves overhang by `oh` on the lateral sides and by `endOh`
 * past each gable end. Pushes GeoEntry slabs for the caller to merge.
 * `omitEndSign` (−1/+1) skips that run-axis end's gable triangle — used at the
 * entry end, where the front wall supplies the gable instead.
 */
function addGableRoof(
  entries: GeoEntry[],
  texIdx: number,
  alongZ: boolean,
  cx: number,
  cz: number,
  runLen: number,
  latW: number,
  baseY: number,
  rise: number,
  oh: number,
  endOh: number,
  thick: number,
  omitEndSign: number = 0
) {
  const halfLat = latW / 2
  const ridgeLen = runLen + endOh * 2
  const slopeAngle = Math.atan2(rise, halfLat)
  const eaveDropY = (oh * rise) / halfLat
  const slopeLen =
    ((halfLat + oh) * Math.sqrt(halfLat * halfLat + rise * rise)) / halfLat

  // Two slope slabs. Built ridge-along-X, then rotated to Z for along-Z shafts.
  // The ridge end is mitered so the two slabs' outer faces meet flush at the
  // peak instead of leaving a gap (same technique as the house gabled roof).
  const ridgeExt = (thick * rise) / halfLat
  const totalSlopeLen = slopeLen + ridgeExt
  for (const side of [-1, 1] as const) {
    const geo = new THREE.BoxGeometry(ridgeLen, thick, totalSlopeLen)
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(i, uv.getX(i) * ridgeLen, uv.getY(i) * totalSlopeLen)
    }
    // Pull the inner (underside) vertices at the ridge end outward by ridgeExt
    // so the slab's top edge forms the peak with no overlap or gap.
    const pos = geo.getAttribute('position')
    const innerY = -thick / 2
    const ridgeEndZ = (-side * totalSlopeLen) / 2
    for (let i = 0; i < pos.count; i++) {
      if (
        Math.abs(pos.getY(i) - innerY) < 1e-3 &&
        Math.abs(pos.getZ(i) - ridgeEndZ) < 1e-3
      ) {
        pos.setZ(i, ridgeEndZ + side * ridgeExt)
      }
    }
    geo.translate(0, thick / 2, (-side * ridgeExt) / 2)
    _roofMat.makeRotationX(side * slopeAngle)
    geo.applyMatrix4(_roofMat)
    if (alongZ) {
      _roofMat.makeRotationY(Math.PI / 2)
      geo.applyMatrix4(_roofMat)
    }
    const perpCenter = (side * (halfLat + oh)) / 2
    const yCenter = baseY + (rise - eaveDropY) / 2
    const tx = cx + (alongZ ? perpCenter : 0)
    const tz = cz + (alongZ ? 0 : perpCenter)
    _roofMat.makeTranslation(tx, yCenter, tz)
    geo.applyMatrix4(_roofMat)
    entries.push({ geo, textureIndex: texIdx })
  }

  // Triangular gable wall at each run-axis end (base at baseY, apex at ridge).
  for (const endSign of [-1, 1] as const) {
    if (endSign === omitEndSign) continue
    const shape = new THREE.Shape()
    shape.moveTo(-halfLat, 0)
    shape.lineTo(halfLat, 0)
    shape.lineTo(0, rise)
    shape.closePath()
    const geo = new THREE.ShapeGeometry(shape) // XY plane, normal +Z
    if (alongZ) {
      _roofMat.makeRotationY(endSign === 1 ? 0 : Math.PI)
    } else {
      _roofMat.makeRotationY(endSign === 1 ? Math.PI / 2 : -Math.PI / 2)
    }
    geo.applyMatrix4(_roofMat)
    const tx = cx + (alongZ ? 0 : (endSign * runLen) / 2)
    const tz = cz + (alongZ ? (endSign * runLen) / 2 : 0)
    _roofMat.makeTranslation(tx, baseY, tz)
    geo.applyMatrix4(_roofMat)
    entries.push({ geo, textureIndex: texIdx })
  }
}

/**
 * Half-door leaf outline: the hinge-side half of the door, in the leaf's local
 * XY plane with the hinge edge at x=0, the centre split at x=halfW, and the
 * bottom at y=0. The outer (hinge) edge runs straight up to the shoulder, then
 * a two-segment arc (bend1 lower, bend2 upper) on a quarter-ellipse curves over
 * to the apex on the centre split — giving a rounded dome rather than a steep
 * pointed gable. Two mirrored leaves form the full door:
 *
 *          _apex_      _apex_
 *        bend2  |      |  bend2   ← rounded cap (split down the middle)
 *       bend1   |      |   bend1
 *   shoulder    |      |    shoulder ← body top
 *       |       |      |       |    ← rectangular body
 *    hinge --- split  split --- hinge
 */
/**
 * Intermediate angles (deg) of the cap arc between the shoulder (0°) and the
 * apex (90°). Shared by the door leaves and the entrance arch so their curves
 * coincide exactly.
 */
const DOOR_CAP_ANGLES_DEG = [30, 60]

/**
 * A point on the cap's quarter-ellipse measured from the outer edge: x =
 * halfW·(1−cosθ) runs 0→halfW, y = shoulder + capH·sinθ runs shoulder→shoulder+capH.
 */
function capArcPoint(
  halfW: number,
  capH: number,
  shoulder: number,
  deg: number
): { x: number; y: number } {
  const t = (deg * Math.PI) / 180
  return { x: halfW * (1 - Math.cos(t)), y: shoulder + capH * Math.sin(t) }
}

function halfDoorLeafShape(
  halfW: number,
  h: number,
  shoulder: number
): THREE.Shape {
  const s = new THREE.Shape()
  const capH = h - shoulder
  const bend1 = capArcPoint(halfW, capH, shoulder, DOOR_CAP_ANGLES_DEG[0])
  const bend2 = capArcPoint(halfW, capH, shoulder, DOOR_CAP_ANGLES_DEG[1])
  s.moveTo(0, 0) // bottom hinge corner
  s.lineTo(halfW, 0) // bottom centre (split)
  s.lineTo(halfW, h) // apex (top of split)
  s.lineTo(bend2.x, bend2.y) // upper cap bend
  s.lineTo(bend1.x, bend1.y) // lower cap bend
  s.lineTo(0, shoulder) // hinge shoulder
  s.closePath()
  return s
}

/**
 * Front entrance wall filling the entry opening above the door's rounded cap.
 * Built as an extruded shape, bottom→top: the dome curve (matching the door cap
 * exactly) as its arched underside, vertical sides up to the wall top, then a
 * gabled peak rising `gableRise` to the roof ridge — so this single wall fills
 * both the arch spandrel and the front gable triangle (which the roof therefore
 * omits), all in the wall texture. Rotated/translated onto the entry plane and
 * pushed as a wall-textured GeoEntry to merge with the other entrance walls.
 * `entryLow` = the entry sits at the low-coordinate end.
 */
function addEntranceArch(
  entries: GeoEntry[],
  alongZ: boolean,
  cr: { x: number; w: number; z: number; d: number },
  entryLow: boolean,
  ctx: DungeonGeoCtx,
  top: number,
  gableRise: number
) {
  const W = ctx.shaftW
  const halfW = W / 2
  const shoulder = DOOR_SHOULDER
  const capH = DOOR_HEIGHT - shoulder
  const p1 = capArcPoint(halfW, capH, shoulder, DOOR_CAP_ANGLES_DEG[0])
  const p2 = capArcPoint(halfW, capH, shoulder, DOOR_CAP_ANGLES_DEG[1])
  const T = ENTRANCE_WALL_T

  // Trace the gabled top + sides, then the dome underside right→left.
  const shape = new THREE.Shape()
  shape.moveTo(0, shoulder)
  shape.lineTo(0, top)
  shape.lineTo(halfW, top + gableRise) // gable apex (roof ridge)
  shape.lineTo(W, top)
  shape.lineTo(W, shoulder)
  shape.lineTo(W - p1.x, p1.y) // right lower bend
  shape.lineTo(W - p2.x, p2.y) // right upper bend
  shape.lineTo(halfW, DOOR_HEIGHT) // apex
  shape.lineTo(p2.x, p2.y) // left upper bend
  shape.lineTo(p1.x, p1.y) // left lower bend
  shape.closePath()

  // ExtrudeGeometry is non-indexed; the other entrance walls (Box/Shape) are
  // indexed, and mergeGeometries needs all-or-none indexed. mergeVertices
  // returns an indexed copy so the arch merges with them.
  const raw = new THREE.ExtrudeGeometry(shape, {
    depth: T,
    bevelEnabled: false,
  })
  const geo = mergeVertices(raw)
  raw.dispose()
  const m = new THREE.Matrix4()
  if (alongZ) {
    // shapeX→X (lateral), shapeY→Y, thickness→Z, centred on the entry plane.
    const entryZ = entryLow ? cr.z : cr.z + cr.d
    m.makeTranslation(cr.x, 0, entryZ - T / 2)
    geo.applyMatrix4(m)
  } else {
    // Rotate so shapeX→+Z (lateral), thickness→−X; then place on the entry plane.
    m.makeRotationY(-Math.PI / 2)
    geo.applyMatrix4(m)
    const entryX = entryLow ? cr.x : cr.x + cr.w
    m.makeTranslation(entryX + T / 2, 0, cr.z)
    geo.applyMatrix4(m)
  }
  entries.push({ geo, textureIndex: DUNGEON_ENTRANCE_WALL_TEXTURE_IDX })
}

export interface DoorLeaf {
  pivot: THREE.Group
  /** rotation.y when shut (leaf flush across its half of the doorway). */
  closedAngle: number
  /** rotation.y when fully open (swung outward); within ±90° of closed. */
  openAngle: number
}

/**
 * Open angle for a leaf: closedAngle ± 90°, choosing the sign whose swing
 * points the leaf outward (a ≤90° swing, so a linear lerp never wraps the long
 * way round). A leaf's +x' axis points to (cosφ, 0, −sinφ) after rotation.y=φ.
 */
function leafOpenAngle(
  closedAngle: number,
  outX: number,
  outZ: number
): number {
  // The two candidates are 180° apart, so their outward dot products are exact
  // negatives — a single sign test on the +90° candidate picks the outward one.
  const plus = closedAngle + Math.PI / 2
  const dotPlus = Math.cos(plus) * outX - Math.sin(plus) * outZ
  return dotPlus >= 0 ? plus : closedAngle - Math.PI / 2
}

/**
 * Double entrance doors, split down the middle and swinging open to both sides
 * like a house door. Two pivot Groups (each rotating about its outer hinge),
 * each carrying a half-heptagon leaf mesh; positioned at the open (entry) end
 * of the covered footprint, at ground level (local y=0). The two leaves' UVs
 * map the left/right halves of the garage-door image so they reconstruct one
 * door when shut. Caller animates each `pivot.rotation.y` between the returned
 * closed/open angles (open swings outward, away from the deep end).
 */
function buildEntranceDoors(
  entranceShaft: DungeonShaft,
  cr: { x: number; w: number; z: number; d: number },
  ctx: DungeonGeoCtx
): DoorLeaf[] {
  const halfW = ctx.shaftW / 2
  const alongZ = entranceShaft.alongZ
  const nonrev = !entranceShaft.reversed
  // Outward (toward the entry/outside, away from the deep end).
  const outX = alongZ ? 0 : nonrev ? -1 : 1
  const outZ = alongZ ? (nonrev ? -1 : 1) : 0
  // Entry (open) end is the low-coordinate end unless the shaft runs reversed.
  const entryZ = nonrev ? cr.z : cr.z + cr.d
  const entryX = nonrev ? cr.x : cr.x + cr.w

  const mat = getHousingMaterial(DUNGEON_DOOR_TEXTURE_IDX)

  // The two leaves hinge on opposite lateral jambs (low / high) and meet at the
  // doorway centre. `uHinge` is the image U at the hinge edge; both leaves run
  // to U=0.5 at the split, so the low leaf maps [0,0.5] and the high leaf [1,0.5].
  const specs = alongZ
    ? [
        { hingeX: cr.x, hingeZ: entryZ, closedAngle: 0, uHinge: 0 },
        {
          hingeX: cr.x + cr.w,
          hingeZ: entryZ,
          closedAngle: Math.PI, // +x' points back toward the centre
          uHinge: 1,
        },
      ]
    : [
        { hingeX: entryX, hingeZ: cr.z, closedAngle: -Math.PI / 2, uHinge: 0 },
        {
          hingeX: entryX,
          hingeZ: cr.z + cr.d,
          closedAngle: Math.PI / 2,
          uHinge: 1,
        },
      ]

  const leaves: DoorLeaf[] = []
  for (const spec of specs) {
    const shape = halfDoorLeafShape(halfW, DOOR_HEIGHT, DOOR_SHOULDER)
    const geo = new THREE.ExtrudeGeometry(shape, {
      depth: DOOR_THICKNESS,
      bevelEnabled: false,
    })
    geo.translate(0, 0, -DOOR_THICKNESS / 2) // centre thickness on the hinge plane
    // ExtrudeGeometry UVs are in shape (meter) coords. Map this leaf to its
    // half of the image: u runs from uHinge (hinge edge) to 0.5 (centre split),
    // v spans the full height. (Thin side faces get squished UVs — barely seen.)
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) {
      const u = spec.uHinge + (0.5 - spec.uHinge) * (uv.getX(i) / halfW)
      uv.setXY(i, u, uv.getY(i) / DOOR_HEIGHT)
    }
    uv.needsUpdate = true

    const pivot = new THREE.Group()
    pivot.name = 'dungeon_entrance_door'
    // Tag for the click raycaster (inputHandler) to recognise a dungeon door.
    pivot.userData.dungeonDoor = true
    pivot.position.set(spec.hingeX, 0, spec.hingeZ)
    pivot.rotation.y = spec.closedAngle
    pivot.add(new THREE.Mesh(geo, mat))
    leaves.push({
      pivot,
      closedAngle: spec.closedAngle,
      openAngle: leafOpenAngle(spec.closedAngle, outX, outZ),
    })
  }
  return leaves
}

function shaftRect(shaft: DungeonShaft, ctx: DungeonGeoCtx) {
  return shaft.alongZ
    ? { x: shaft.x, z: shaft.z, w: ctx.shaftW, d: ctx.shaftLen }
    : { x: shaft.x, z: shaft.z, w: ctx.shaftLen, d: ctx.shaftW }
}

function shaftContains(
  shaft: DungeonShaft,
  ctx: DungeonGeoCtx,
  x: number,
  z: number
): boolean {
  const r = shaftRect(shaft, ctx)
  return x >= r.x && x < r.x + r.w && z >= r.z && z < r.z + r.d
}

/** Cell at run position i (0 = entry/shallow end), lateral offset wOff. */
export function shaftStepCell(
  shaft: DungeonShaft,
  ctx: DungeonGeoCtx,
  i: number,
  wOff: number
): { x: number; z: number } {
  const run = shaft.reversed ? ctx.shaftLen - 1 - i : i
  return shaft.alongZ
    ? { x: shaft.x + wOff, z: shaft.z + run }
    : { x: shaft.x + run, z: shaft.z + wOff }
}

/**
 * Stair geometry for one shaft, local to the floor group. `topY`/`bottomY`
 * are local Y of the shallow and deep landings. Adds the steps plus flat
 * landing platforms at both ends (the far landing belongs to the
 * neighbouring floor's slab, which isn't rendered — without a platform
 * you'd stand on visual void before the floor switch).
 */
function collectShaftStairs(
  entries: GeoEntry[],
  shaft: DungeonShaft,
  ctx: DungeonGeoCtx,
  topY: number,
  bottomY: number,
  includeTopLanding: boolean,
  includeBottomLanding: boolean,
  includeWall = true
) {
  const rise = topY - bottomY
  const runStart = LANDING_CELLS
  const runLen = ctx.shaftLen - LANDING_CELLS * 2
  const stepCount = Math.max(1, Math.round(rise / STEP_RISE))
  const stepRise = rise / stepCount
  const stepDepth = runLen / stepCount

  // Run-axis basis: position of run coordinate t (cells from entry end),
  // lateral center of the shaft.
  const r = shaftRect(shaft, ctx)
  const latCenter = shaft.alongZ ? r.x + r.w / 2 : r.z + r.d / 2
  const runAt = (t: number) => {
    const raw = shaft.reversed ? ctx.shaftLen - t : t
    return (shaft.alongZ ? r.z : r.x) + raw
  }
  const addRunBox = (t0: number, t1: number, h: number, cy: number) => {
    const a = runAt(t0)
    const b = runAt(t1)
    const runC = (a + b) / 2
    const runLenAbs = Math.abs(b - a)
    if (shaft.alongZ) {
      addBox(
        entries,
        DUNGEON_FLOOR_TEXTURE_IDX,
        ctx.shaftW,
        h,
        runLenAbs,
        latCenter,
        cy,
        runC,
        DUNGEON_FLOOR_UV_SCALE
      )
    } else {
      addBox(
        entries,
        DUNGEON_FLOOR_TEXTURE_IDX,
        runLenAbs,
        h,
        ctx.shaftW,
        runC,
        cy,
        latCenter,
        DUNGEON_FLOOR_UV_SCALE
      )
    }
  }

  if (includeTopLanding) {
    addRunBox(0, LANDING_CELLS, SLAB_THICKNESS, topY - SLAB_THICKNESS / 2)
  }
  // Solid steps: each box rises from the deep landing up to its tread.
  for (let i = 0; i < stepCount; i++) {
    const t0 = runStart + i * stepDepth
    const t1 = t0 + stepDepth
    const treadY = topY - (i + 0.5) * stepRise
    const h = treadY - bottomY
    addRunBox(t0, t1, h, bottomY + h / 2)
  }
  if (includeBottomLanding) {
    addRunBox(
      ctx.shaftLen - LANDING_CELLS,
      ctx.shaftLen,
      SLAB_THICKNESS,
      bottomY - SLAB_THICKNESS / 2
    )
  }

  // Shaft side walls (back-facing side only, camera rule as for walls):
  // along-Z shafts keep the east side (faces west), along-X the north
  // side (faces south). Vertical span covers the full descent. Skipped for
  // the surface entrance, which supplies its own non-protruding pit walls.
  if (includeWall) {
    const wallTex = DUNGEON_WALL_TEXTURE_IDX
    const wallH = topY - bottomY + ctx.wallHeight
    const wallCy = bottomY + wallH / 2
    if (shaft.alongZ) {
      addBox(
        entries,
        wallTex,
        0.1,
        wallH,
        r.d,
        r.x + r.w + 0.05,
        wallCy,
        r.z + r.d / 2
      )
    } else {
      addBox(
        entries,
        wallTex,
        r.w,
        wallH,
        0.1,
        r.x + r.w / 2,
        wallCy,
        r.z - 0.05
      )
    }
  }
}

/**
 * Build the renderable group for one dungeon floor. The caller positions
 * it at (originX, floorY(depth), originZ) in world space.
 */
export function buildDungeonFloorGroup(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx
): THREE.Group {
  const grid = ctx.grid
  const carvedAt = (x: number, z: number) =>
    x >= 0 && x < grid && z >= 0 && z < grid && layout.carved[x + z * grid]

  const entries: GeoEntry[] = []

  // Down-shaft hole: slab is omitted over the shaft except its entry row.
  const down = layout.downShaft
  const downEntry = down ? shaftStepCell(down, ctx, 0, 0) : null
  const inDownHole = (x: number, z: number): boolean => {
    if (!down || !shaftContains(down, ctx, x, z)) return false
    const onEntryRow = down.alongZ ? z === downEntry!.z : x === downEntry!.x
    return !onEntryRow
  }
  // Note: serde Option<T> arrives as undefined (not null) over wasm.
  const inAnyShaft = (x: number, z: number): boolean =>
    shaftContains(layout.upShaft, ctx, x, z) ||
    (down != null && shaftContains(down, ctx, x, z))

  // --- Floor slab: row-run boxes over carved cells minus the down hole.
  for (let z = 0; z < grid; z++) {
    let runStart = -1
    for (let x = 0; x <= grid; x++) {
      const solidFloor = x < grid && carvedAt(x, z) && !inDownHole(x, z)
      if (solidFloor && runStart < 0) runStart = x
      if (!solidFloor && runStart >= 0) {
        const len = x - runStart
        addBox(
          entries,
          DUNGEON_FLOOR_TEXTURE_IDX,
          len,
          SLAB_THICKNESS,
          1,
          runStart + len / 2,
          -SLAB_THICKNESS / 2,
          z + 0.5,
          DUNGEON_FLOOR_UV_SCALE
        )
        runStart = -1
      }
    }
  }

  // --- Back walls (camera-away sides only): north edges (solid at z-1)
  // merged into x-runs, east edges (solid at x+1) merged into z-runs.
  // Shaft cells are skipped — their taller side walls are built with the
  // stairs.
  for (let z = 0; z < grid; z++) {
    let runStart = -1
    for (let x = 0; x <= grid; x++) {
      const hasWall =
        x < grid && carvedAt(x, z) && !carvedAt(x, z - 1) && !inAnyShaft(x, z)
      if (hasWall && runStart < 0) runStart = x
      if (!hasWall && runStart >= 0) {
        const len = x - runStart
        addBox(
          entries,
          DUNGEON_WALL_TEXTURE_IDX,
          len,
          ctx.wallHeight,
          0.1,
          runStart + len / 2,
          ctx.wallHeight / 2,
          z - 0.05
        )
        runStart = -1
      }
    }
  }
  for (let x = 0; x < grid; x++) {
    let runStart = -1
    for (let z = 0; z <= grid; z++) {
      const hasWall =
        z < grid && carvedAt(x, z) && !carvedAt(x + 1, z) && !inAnyShaft(x, z)
      if (hasWall && runStart < 0) runStart = z
      if (!hasWall && runStart >= 0) {
        const len = z - runStart
        addBox(
          entries,
          DUNGEON_WALL_TEXTURE_IDX,
          0.1,
          ctx.wallHeight,
          len,
          x + 1 + 0.05,
          ctx.wallHeight / 2,
          runStart + len / 2
        )
        runStart = -1
      }
    }
  }

  // --- Treasure chest (final floor): a squat dark-wood box with a lid
  // ridge, sitting on the chest cell.
  if (layout.chest) {
    const [cx, cz] = layout.chest
    const x = cx + 0.5
    const z = cz + 0.5
    addBox(entries, DUNGEON_CHEST_TEXTURE_IDX, 0.9, 0.5, 0.6, x, 0.25, z)
    addBox(entries, DUNGEON_CHEST_TEXTURE_IDX, 0.96, 0.14, 0.66, x, 0.55, z)
    addBox(entries, DUNGEON_FLOOR_TEXTURE_IDX, 0.98, 0.04, 0.1, x, 0.45, z)
  }

  // --- Stairs: the up shaft descends from the floor above (+floorHeight
  // → 0); the down shaft from here to the floor below (0 → -floorHeight).
  collectShaftStairs(
    entries,
    layout.upShaft,
    ctx,
    ctx.floorHeight,
    0,
    true, // top landing: neighbour floor's slab is not rendered
    false // bottom landing: this floor's slab covers the exit row
  )
  if (down) {
    collectShaftStairs(entries, down, ctx, 0, -ctx.floorHeight, false, true)
  }

  const group = new THREE.Group()
  addMergedMeshes(group, entries)
  return group
}

/**
 * Surface entrance structure, rendered at depth 0 so the terrain hole over
 * the shaft reads as a covered stairwell. Renders the descending stairs (so
 * the player visibly walks down the upper half before the floor-1 group takes
 * over at the shaft midpoint) plus stone walls on the two run-axis sides and
 * the far (deep) end, spanning from a dark pit floor one floor down
 * (−floorHeight) up to a raised parapet (+ABOVE), capped by a gabled gravel
 * roof — a small roofed shed over the stairs. The entry end stays open as
 * an ABOVE-tall doorway.
 *
 * The covered footprint is anchored at the entry (shallow) end — one landing
 * cell gap, then half the tread span toward the deep end — so the shed is a
 * compact porch over the upper stairs (shaftHoleRect insets the deep end to
 * match). The stairs themselves are still built full-length, so the lower half
 * continues descending under the terrain past the porch's far wall (which lands
 * on the shaft midpoint, where the depth-0↔1 swap hides this group anyway). The
 * anchored inset depends on `reversed` (which end is the entry).
 *
 * The whole group (roof included) is shown only at depth 0 via group
 * visibility. Local to (originX, entranceY, originZ) like floors.
 */
export interface DungeonEntranceGroup {
  group: THREE.Group
  /** Double-door leaves at the entry; caller lerps each rotation.y open/shut. */
  doors: DoorLeaf[]
}

export function buildDungeonEntranceGroup(
  entranceShaft: DungeonShaft,
  ctx: DungeonGeoCtx
): DungeonEntranceGroup {
  const entries: GeoEntry[] = []
  const r = shaftRect(entranceShaft, ctx)

  // Covered footprint: anchored at the entry (shallow) end with a one-cell
  // landing gap, then running half the tread span toward the deep end (the
  // remaining lower stairs continue under the terrain). shaftCoverRun is the
  // single source the terrain hole (shaftHoleRect) also uses, so the two stay
  // in lockstep.
  const { inset, coverLen } = shaftCoverRun(
    ctx.shaftLen,
    entranceShaft.reversed
  )
  const cr = entranceShaft.alongZ
    ? { x: r.x, w: r.w, z: r.z + inset, d: coverLen }
    : { x: r.x + inset, w: coverLen, z: r.z, d: r.d }

  // How far the walls descend — matches the up-shaft drop to floor 1.
  const depth = ctx.floorHeight
  // Headroom above the entry surface: walls rise this far above ground (a
  // raised parapet) and the gabled roof sits on top. The deep end clears
  // depth + ABOVE; the entry end is an ABOVE-tall doorway.
  const ABOVE = 3.0
  const T = ENTRANCE_WALL_T // wall thickness (shared with collision)
  const CT = 0.2 // roof slab thickness
  const OH = 0.5 // lateral roof eave overhang (past the ~0.35m corner pillars)
  const END_OH = 0.5 // run-axis (gable end) overhang past the corner pillars
  const RIDGE_RISE = 1.0 // gable peak height above the walls

  // Dark floor at the bottom of the visible shaft (backs the open pit so it
  // doesn't show through to the sky).
  addBox(
    entries,
    DUNGEON_VOID_TEXTURE_IDX,
    cr.w,
    0.05,
    cr.d,
    cr.x + cr.w / 2,
    -depth + 0.025,
    cr.z + cr.d / 2
  )

  // Mossy-plaster walls on the two run-axis sides and the far (deep) end,
  // spanning [−depth, +ABOVE]. The entry end stays open. Slight outset so
  // walking the shaft never clips them. (Surface building — distinct texture
  // from the underground stone walls.)
  const wallH = depth + ABOVE
  const wallCy = (ABOVE - depth) / 2 // center of the [−depth, +ABOVE] span
  const wallTex = DUNGEON_ENTRANCE_WALL_TEXTURE_IDX
  // Deep/far end is the high-coordinate end unless the shaft runs reversed.
  const farPositive = !entranceShaft.reversed
  if (entranceShaft.alongZ) {
    addBox(
      entries,
      wallTex,
      T,
      wallH,
      cr.d + T,
      cr.x - T / 2,
      wallCy,
      cr.z + cr.d / 2
    )
    addBox(
      entries,
      wallTex,
      T,
      wallH,
      cr.d + T,
      cr.x + cr.w + T / 2,
      wallCy,
      cr.z + cr.d / 2
    )
    const farZ = farPositive ? cr.z + cr.d + T / 2 : cr.z - T / 2
    addBox(
      entries,
      wallTex,
      cr.w + T * 2,
      wallH,
      T,
      cr.x + cr.w / 2,
      wallCy,
      farZ
    )
  } else {
    addBox(
      entries,
      wallTex,
      cr.w + T,
      wallH,
      T,
      cr.x + cr.w / 2,
      wallCy,
      cr.z - T / 2
    )
    addBox(
      entries,
      wallTex,
      cr.w + T,
      wallH,
      T,
      cr.x + cr.w / 2,
      wallCy,
      cr.z + cr.d + T / 2
    )
    const farX = farPositive ? cr.x + cr.w + T / 2 : cr.x - T / 2
    addBox(
      entries,
      wallTex,
      T,
      wallH,
      cr.d + T * 2,
      farX,
      wallCy,
      cr.z + cr.d / 2
    )
  }

  // Front wall over the door: fills the entry opening above the rounded cap
  // plus the front gable triangle (so the doorway reads as a fitted arch under
  // a gabled wall, all stone). entryLow = entry at the low-coord end.
  addEntranceArch(
    entries,
    entranceShaft.alongZ,
    cr,
    farPositive,
    ctx,
    ABOVE,
    RIDGE_RISE
  )

  // Decorative stone square pillars at the four footprint corners, protruding
  // PILLAR_PROTRUDE proud of the wall outer faces (centre offset diagonally
  // outward = wall outset + protrusion − half the pillar). Plain boxes from
  // the ground to just under the roof.
  const PILLAR_SIZE = 0.3
  const PILLAR_PROTRUDE = 0.1
  const pillarOff = T + PILLAR_PROTRUDE - PILLAR_SIZE / 2
  const pillarBase = -0.3 // sink slightly so it never floats over dipping terrain
  const pillarTop = ABOVE - 0.07 // stop short of the roof eaves
  const pillarH = pillarTop - pillarBase
  const pillarCy = (pillarTop + pillarBase) / 2
  for (const sx of [-1, 1] as const) {
    for (const sz of [-1, 1] as const) {
      const cornerX = sx < 0 ? cr.x : cr.x + cr.w
      const cornerZ = sz < 0 ? cr.z : cr.z + cr.d
      addBox(
        entries,
        DUNGEON_PILLAR_TEXTURE_IDX,
        PILLAR_SIZE,
        pillarH,
        PILLAR_SIZE,
        cornerX + sx * pillarOff,
        pillarCy,
        cornerZ + sz * pillarOff
      )
    }
  }

  // Gabled gravel-stone roof on top, ridge along the run axis. The gable
  // planes are the doorway edge (entry) and the far wall's *outer* face — so
  // END_OH overhangs past the actual walls on both ends, not the footprint
  // (the far wall is outset by T, which would otherwise eat the overhang).
  // The entry-end gable triangle is omitted — the front wall above supplies it.
  const roofShift = farPositive ? T / 2 : -T / 2
  const alongZ = entranceShaft.alongZ
  const [runDim, latDim] = alongZ ? [cr.d, cr.w] : [cr.w, cr.d]
  // Entry gable is the low-coord end when farPositive, else the high-coord end.
  const entryGableSign = farPositive ? -1 : 1
  addGableRoof(
    entries,
    DUNGEON_CEILING_TEXTURE_IDX,
    alongZ,
    cr.x + cr.w / 2 + (alongZ ? 0 : roofShift),
    cr.z + cr.d / 2 + (alongZ ? roofShift : 0),
    runDim + T,
    latDim,
    ABOVE,
    RIDGE_RISE,
    OH,
    END_OH,
    CT,
    entryGableSign
  )

  // Descending stairs (no side wall — the walls above supply the sides; no
  // landings — terrain covers the entry row, the dark pit floor backs the deep
  // end). Same world-space geometry as the floor-1 up-shaft.
  collectShaftStairs(
    entries,
    entranceShaft,
    ctx,
    0,
    -depth,
    false,
    false,
    false
  )

  // Double doors across the open entry end (kept separate from the merged
  // meshes so they can swing). Local to the same (origin, entranceY) frame.
  const doors = buildEntranceDoors(entranceShaft, cr, ctx)

  const group = new THREE.Group()
  addMergedMeshes(group, entries)
  for (const leaf of doors) group.add(leaf.pivot)
  return { group, doors }
}

/** Dispose merged geometries (materials are shared — never disposed). */
export function disposeDungeonGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) obj.geometry.dispose()
  })
}
