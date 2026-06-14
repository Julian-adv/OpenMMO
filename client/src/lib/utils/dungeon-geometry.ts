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
 * - No ceiling: the isometric camera looks down ~35°, any current-floor
 *   ceiling would fully occlude the player. The void reads as cave dark.
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
import {
  addMergedMeshes,
  bakedGeo,
  HOUSING_TEXTURES,
  type GeoEntry,
} from './house-geo-utils'
import type {
  DungeonFloorLayout,
  DungeonShaft,
} from '../managers/dungeonManager'

export const DUNGEON_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
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

const SLAB_THICKNESS = 0.15
/** Flat landing cells at shaft ends — must match dungeonManager.rampY. */
const LANDING_CELLS = 1.0
const STEP_RISE = 0.25

export interface DungeonGeoCtx {
  grid: number
  /** Wall visual height (matches shared DUNGEON_WALL_HEIGHT). */
  wallHeight: number
  /** Vertical distance between floors (shared DUNGEON_FLOOR_HEIGHT). */
  floorHeight: number
  shaftW: number
  shaftLen: number
}

/** Box with housing-style face UVs derived from final (baked) position. */
function addBox(
  entries: GeoEntry[],
  textureIndex: number,
  w: number,
  h: number,
  d: number,
  cx: number,
  cy: number,
  cz: number
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
  entries.push({ geo: bakedGeo(geo, cx, cy, cz, 0, 1, 1), textureIndex })
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
  includeBottomLanding: boolean
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
      addBox(entries, DUNGEON_FLOOR_TEXTURE_IDX, ctx.shaftW, h, runLenAbs, latCenter, cy, runC)
    } else {
      addBox(entries, DUNGEON_FLOOR_TEXTURE_IDX, runLenAbs, h, ctx.shaftW, runC, cy, latCenter)
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
  // side (faces south). Vertical span covers the full descent.
  const wallTex = DUNGEON_WALL_TEXTURE_IDX
  const wallH = topY - bottomY + ctx.wallHeight
  const wallCy = bottomY + wallH / 2
  if (shaft.alongZ) {
    addBox(entries, wallTex, 0.1, wallH, r.d, r.x + r.w + 0.05, wallCy, r.z + r.d / 2)
  } else {
    addBox(entries, wallTex, r.w, wallH, 0.1, r.x + r.w / 2, wallCy, r.z - 0.05)
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
          z + 0.5
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
 * Above-ground entrance structure: a stone parapet around the shaft
 * opening plus a near-black cap floating just above the terrain so the
 * opening reads as a pit (the terrain mesh itself has no hole — the
 * actual shaft below only becomes visible in underground render mode).
 * The parapet leaves the entry end open; descending players sink behind
 * the side walls. Local to (originX, entranceY, originZ) like floors.
 */
export function buildDungeonEntranceGroup(
  entranceShaft: DungeonShaft,
  ctx: DungeonGeoCtx
): THREE.Group {
  const entries: GeoEntry[] = []
  const r = shaftRect(entranceShaft, ctx)

  // Pit cap slightly above the terrain surface.
  addBox(
    entries,
    DUNGEON_VOID_TEXTURE_IDX,
    r.w,
    0.05,
    r.d,
    r.x + r.w / 2,
    0.06,
    r.z + r.d / 2
  )

  // Parapet: low stone walls on the two run-axis sides and the far
  // (deep) end; the entry end stays open. Slight outset so walking the
  // shaft never clips them.
  const H = 1.0
  const T = 0.25
  const entry = shaftStepCell(entranceShaft, ctx, 0, 0)
  if (entranceShaft.alongZ) {
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, T, H, r.d + T, r.x - T / 2, H / 2, r.z + r.d / 2)
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, T, H, r.d + T, r.x + r.w + T / 2, H / 2, r.z + r.d / 2)
    const farZ = entry.z === r.z ? r.z + r.d + T / 2 : r.z - T / 2
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, r.w + T * 2, H, T, r.x + r.w / 2, H / 2, farZ)
  } else {
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, r.w + T, H, T, r.x + r.w / 2, H / 2, r.z - T / 2)
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, r.w + T, H, T, r.x + r.w / 2, H / 2, r.z + r.d + T / 2)
    const farX = entry.x === r.x ? r.x + r.w + T / 2 : r.x - T / 2
    addBox(entries, DUNGEON_WALL_TEXTURE_IDX, T, H, r.d + T * 2, farX, H / 2, r.z + r.d / 2)
  }

  const group = new THREE.Group()
  addMergedMeshes(group, entries)
  return group
}

/** Dispose merged geometries (materials are shared — never disposed). */
export function disposeDungeonGroup(group: THREE.Group) {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh) obj.geometry.dispose()
  })
}
