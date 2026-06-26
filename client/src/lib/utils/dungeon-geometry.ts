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
 * - Walls on all four sides are emitted as per-run meshes in their own
 *   sub-group (WALL_RUN_GROUP_NAME); the dungeon layer fades any run to a
 *   ghost while it occludes the player from the iso camera. Camera-facing
 *   south/west runs fade often; the far north/east runs only when the layout
 *   puts them between the camera and the player. Floor slab, chest and the
 *   down-shaft stairs stay merged into the shared opaque mesh.
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
  ENTRANCE_DOOR_DEPTH,
  ENTRANCE_DOOR_ID,
  encodeInteriorDoorId,
  shaftCoverRun,
  type DungeonDoorSeg,
  type DungeonFloorLayout,
  type DungeonShaft,
} from '../managers/dungeonManager'

export const DUNGEON_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'housing/medieval_blocks_03_1k'
)
/** Rock-wall-10 texture for the *connecting corridors* — distinct from the
 *  medieval-stone room walls (DUNGEON_WALL_TEXTURE_IDX). A cell gets this
 *  texture when it is carved floor that lies in no room (and no shaft). */
export const DUNGEON_CORRIDOR_WALL_TEXTURE_IDX = HOUSING_TEXTURES.findIndex(
  (e) => e.glb === 'dungeon/rock_wall_10_1k'
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

// Dungeon stone needs no explicit shadowSide: the housing materials are
// `side: FrontSide`, so three.js derives BackSide, and each solid's own thickness
// (0.15m slab, 0.1m walls) self-biases away acne. (FrontSide was tried to anchor
// contact shadows but acned the walls.) BackSide records the far face, so where a
// caster's bottom sits exactly at floor level the shadow peter-pans (no contact
// darkening); lifting the caster a hair pulls that recorded depth above the floor.
/** Vertical lift (m) off the floor for floor-contacting dungeon casters (stair
 *  prisms, back walls). The base seam is sub-texel under PCF, and behind a back
 *  wall is uncarved rock, so the gap reveals only void, never lit floor. */
const SHADOW_CONTACT_LIFT = 0.02

/** Name of the up-shaft stairs sub-group inside a floor group; the dungeon
 *  layer looks it up to fade it to a ghost when it occludes the player. */
export const UP_SHAFT_GROUP_NAME = 'upShaftStairs'

/** Scene-graph name of the wall-run sub-group (all four sides), for debugging.
 *  Unlike the up-shaft group it is never looked up by name — the layer caches
 *  the runs from the returned WallRun[] — so it stays module-private. */
const WALL_RUN_GROUP_NAME = 'wallRuns'

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
  (e) => e.glb === 'dungeon/wooden_garage_door_1k'
)
/** Door panel thickness (m). */
const DOOR_THICKNESS = 0.12
/** Door apex height above the entry ground (under the ABOVE=3.0 doorway). */
const DOOR_HEIGHT = 2.85
/** Height where the rectangular body ends and the pentagonal cap begins. */
const DOOR_SHOULDER = 1.9
/** Thickness of a dungeon wall-run mesh; its plane sits half this past the cell
 *  face it guards (a north wall at z − HALF, an east wall at x + 1 + HALF). */
const WALL_THICKNESS = 0.1
const WALL_HALF_THICKNESS = WALL_THICKNESS / 2

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
  bottomY: number,
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
    const yCenter = bottomY + (rise - eaveDropY) / 2
    const tx = cx + (alongZ ? perpCenter : 0)
    const tz = cz + (alongZ ? 0 : perpCenter)
    _roofMat.makeTranslation(tx, yCenter, tz)
    geo.applyMatrix4(_roofMat)
    entries.push({ geo, textureIndex: texIdx })
  }

  // Triangular gable wall at each run-axis end (base at bottomY, apex at ridge).
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
    _roofMat.makeTranslation(tx, bottomY, tz)
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
/** Append the door-cap dome underside (right→left) to an arch shape spanning
 *  [0,W] laterally, so the rounded door cap fits exactly beneath it. */
function traceArchDome(shape: THREE.Shape, W: number) {
  const halfW = W / 2
  const capH = DOOR_HEIGHT - DOOR_SHOULDER
  const p1 = capArcPoint(halfW, capH, DOOR_SHOULDER, DOOR_CAP_ANGLES_DEG[0])
  const p2 = capArcPoint(halfW, capH, DOOR_SHOULDER, DOOR_CAP_ANGLES_DEG[1])
  shape.lineTo(W - p1.x, p1.y) // right lower bend
  shape.lineTo(W - p2.x, p2.y) // right upper bend
  shape.lineTo(halfW, DOOR_HEIGHT) // apex
  shape.lineTo(p2.x, p2.y) // left upper bend
  shape.lineTo(p1.x, p1.y) // left lower bend
}

/**
 * Extrude an arch spandrel shape (laid out in lateral-X / up-Y) to thickness
 * `T`, convert to indexed (so it merges with the indexed wall geometry), place
 * it on the wall line and push it. `alongZ` ⇒ keep shapeX→X with thickness→Z
 * centred at z=`wallLine`; otherwise rotate shapeX→+Z with thickness→−X centred
 * at x=`wallLine`. `latStart` is the lateral origin (the opening's low edge).
 */
function extrudeAndPlaceArch(
  entries: GeoEntry[],
  shape: THREE.Shape,
  alongZ: boolean,
  latStart: number,
  wallLine: number,
  T: number,
  textureIndex: number
) {
  const raw = new THREE.ExtrudeGeometry(shape, {
    depth: T,
    bevelEnabled: false,
  })
  const geo = mergeVertices(raw)
  raw.dispose()
  const m = new THREE.Matrix4()
  if (alongZ) {
    m.makeTranslation(latStart, 0, wallLine - T / 2)
    geo.applyMatrix4(m)
  } else {
    m.makeRotationY(-Math.PI / 2)
    geo.applyMatrix4(m)
    m.makeTranslation(wallLine + T / 2, 0, latStart)
    geo.applyMatrix4(m)
  }
  entries.push({ geo, textureIndex })
}

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
  // Gabled top + sides, then the shared dome underside.
  const shape = new THREE.Shape()
  shape.moveTo(0, DOOR_SHOULDER)
  shape.lineTo(0, top)
  shape.lineTo(W / 2, top + gableRise) // gable apex (roof ridge)
  shape.lineTo(W, top)
  shape.lineTo(W, DOOR_SHOULDER)
  traceArchDome(shape, W)
  shape.closePath()
  // alongZ: entry plane is z; otherwise x. entryLow ⇒ the low-coordinate end.
  extrudeAndPlaceArch(
    entries,
    shape,
    alongZ,
    alongZ ? cr.x : cr.z,
    alongZ ? (entryLow ? cr.z : cr.z + cr.d) : entryLow ? cr.x : cr.x + cr.w,
    ENTRANCE_WALL_T,
    DUNGEON_ENTRANCE_WALL_TEXTURE_IDX
  )
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
 * Build one door leaf: a half-heptagon panel extruded to DOOR_THICKNESS and
 * hinged at its outer jamb, mapped to its half of the garage-door image (u from
 * the hinge edge to the centre split, v over the full height). Shared by the
 * surface entrance and the interior room doors. `outX/outZ` is the outward swing
 * direction (the side the leaves open toward). The pivot is left pickable for
 * the click raycaster; callers tag `pivot.userData.dungeonDoorKey` with the
 * door's (depth, id) so a click resolves which door to toggle.
 */
interface DoorLeafSpec {
  hingeX: number
  hingeZ: number
  closedAngle: number
  uHinge: number
}

/**
 * The two leaf hinge specs for a double door. `alongZ` ⇒ the door spans X at the
 * wall line z=`line` (low/high jambs at x=`latLow`/`latHigh`); otherwise it spans
 * Z at x=`line`. The leaves hinge on opposite jambs and meet at the centre split
 * (uHinge 0/1 → both map to U=0.5 there). Shared by the entrance (its along-Z
 * case ≙ a room's north wall) and the interior doors.
 */
function doorLeafSpecs(
  alongZ: boolean,
  latLow: number,
  latHigh: number,
  line: number
): DoorLeafSpec[] {
  return alongZ
    ? [
        { hingeX: latLow, hingeZ: line, closedAngle: 0, uHinge: 0 },
        { hingeX: latHigh, hingeZ: line, closedAngle: Math.PI, uHinge: 1 },
      ]
    : [
        { hingeX: line, hingeZ: latLow, closedAngle: -Math.PI / 2, uHinge: 0 },
        { hingeX: line, hingeZ: latHigh, closedAngle: Math.PI / 2, uHinge: 1 },
      ]
}

function makeDoorLeaf(
  spec: DoorLeafSpec,
  halfW: number,
  outX: number,
  outZ: number,
  mat: THREE.Material
): DoorLeaf {
  const shape = halfDoorLeafShape(halfW, DOOR_HEIGHT, DOOR_SHOULDER)
  const geo = new THREE.ExtrudeGeometry(shape, {
    depth: DOOR_THICKNESS,
    bevelEnabled: false,
  })
  geo.translate(0, 0, -DOOR_THICKNESS / 2) // centre thickness on the hinge plane
  // ExtrudeGeometry UVs are in shape (meter) coords. Map this leaf to its half
  // of the image: u runs from uHinge (hinge edge) to 0.5 (centre split), v spans
  // the full height. (Thin side faces get squished UVs — barely seen.)
  const uv = geo.getAttribute('uv')
  for (let i = 0; i < uv.count; i++) {
    const u = spec.uHinge + (0.5 - spec.uHinge) * (uv.getX(i) / halfW)
    uv.setXY(i, u, uv.getY(i) / DOOR_HEIGHT)
  }
  uv.needsUpdate = true

  const pivot = new THREE.Group()
  pivot.position.set(spec.hingeX, 0, spec.hingeZ)
  pivot.rotation.y = spec.closedAngle
  pivot.add(new THREE.Mesh(geo, mat))
  return {
    pivot,
    closedAngle: spec.closedAngle,
    openAngle: leafOpenAngle(spec.closedAngle, outX, outZ),
  }
}

/** Tag a door's leaves so the click raycaster knows which door to toggle. */
function tagDoorLeaves(leaves: DoorLeaf[], depth: number, doorId: number) {
  for (const leaf of leaves) {
    leaf.pivot.userData.dungeonDoorKey = { depth, doorId }
  }
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
  const specs = doorLeafSpecs(
    alongZ,
    alongZ ? cr.x : cr.z,
    alongZ ? cr.x + cr.w : cr.z + cr.d,
    alongZ ? entryZ : entryX
  )

  const leaves = specs.map((spec) => makeDoorLeaf(spec, halfW, outX, outZ, mat))
  for (const leaf of leaves) leaf.pivot.name = 'dungeon_entrance_door'
  tagDoorLeaves(leaves, ENTRANCE_DOOR_DEPTH, ENTRANCE_DOOR_ID)
  return leaves
}

export interface InteriorDoor {
  /** Synced-state key (matches the toggle packet + dungeonManager door map). */
  depth: number
  doorId: number
  /** The two swinging leaves (added to a sibling group, animated by the layer). */
  leaves: DoorLeaf[]
  /** Doorway blocking segment in floor-local XZ (add the floor origin for world
   *  space); the player can't cross it while the door is shut. */
  seg: DungeonDoorSeg
  /** Eased open fraction (0 shut .. 1 open); the layer advances it toward the
   *  click-toggled, server-synced open state. */
  open: number
}

/**
 * Stone arch above an interior room door: a flat-topped wall panel spanning the
 * opening from the door's shoulder up to the wall top, its underside the same
 * quarter-ellipse dome as the door cap (so the rounded door fits beneath it).
 * Room-wall texture (matching the surrounding room walls), extruded to the
 * wall-run thickness and centred on the room↔corridor wall line. `northWall`
 * ⇒ the door spans X (panel laid along X, thickness along Z); otherwise it spans
 * Z (rotated so the panel lies along Z, thickness along X).
 */
function addInteriorDoorArch(
  archEntries: GeoEntry[],
  northWall: boolean,
  lat0: number,
  W: number,
  wallLine: number,
  wallTop: number
) {
  // Flat top + sides, then the shared dome underside.
  const shape = new THREE.Shape()
  shape.moveTo(0, DOOR_SHOULDER)
  shape.lineTo(0, wallTop)
  shape.lineTo(W, wallTop)
  shape.lineTo(W, DOOR_SHOULDER)
  traceArchDome(shape, W)
  shape.closePath()
  extrudeAndPlaceArch(
    archEntries,
    shape,
    northWall,
    lat0,
    wallLine,
    WALL_THICKNESS,
    DUNGEON_WALL_TEXTURE_IDX
  )
}

/**
 * One interior room door across a corridor mouth: a pair of swinging leaves
 * (same half-heptagon shape as the surface entrance doors) plus a stone arch
 * filling the wall above the rounded cap. `northWall` ⇒ the mouth is in the
 * room's north wall (door spans X, opens south into the room); otherwise it is
 * the east wall (door spans Z, opens west into the room). `lat0`/`len` are the
 * opening's start cell and width along the wall; `wallLine` is the room↔corridor
 * grid line. Arch quads go to `archEntries` (room-wall texture, merged
 * statically); the returned leaves are added to the floor group and animated.
 */
function buildInteriorDoor(
  depth: number,
  northWall: boolean,
  lat0: number,
  len: number,
  wallLine: number,
  wallTop: number,
  mat: THREE.Material,
  archEntries: GeoEntry[]
): InteriorDoor {
  const halfW = len / 2
  // Swing into the room: south (+Z) for a north wall, west (−X) for an east one.
  const outX = northWall ? 0 : -1
  const outZ = northWall ? 1 : 0
  // The room wall run sits half its thickness outside the room boundary (a north
  // wall at z = line − HALF, an east wall at x = line + HALF), so place the
  // visible leaves and arch on that plane to keep them flush with the wall.
  // The door id and the blocking segment stay on the integer grid line.
  const wallPlane =
    wallLine + (northWall ? -WALL_HALF_THICKNESS : WALL_HALF_THICKNESS)
  // north wall ≙ the entrance's along-Z case (door spans X at z=wallPlane).
  const specs = doorLeafSpecs(northWall, lat0, lat0 + len, wallPlane)
  const leaves = specs.map((s) => makeDoorLeaf(s, halfW, outX, outZ, mat))
  const doorId = encodeInteriorDoorId(northWall, lat0, wallLine)
  tagDoorLeaves(leaves, depth, doorId)
  addInteriorDoorArch(archEntries, northWall, lat0, len, wallPlane, wallTop)
  // Blocking segment along the wall line (floor-local; the layer adds origin).
  const seg: DungeonDoorSeg = northWall
    ? { doorId, ax: lat0, az: wallLine, bx: lat0 + len, bz: wallLine }
    : { doorId, ax: wallLine, az: lat0, bx: wallLine, bz: lat0 + len }
  return { depth, doorId, leaves, seg, open: 0 }
}

/** Percent chance a qualifying corridor mouth gets a door. */
const INTERIOR_DOOR_PCT = 30

/** Stable [0,1) hash of four small ints — picks which corridor mouths get a
 *  door, deterministically per layout so every re-render matches (geometry is
 *  generated, never transmitted, so this needs no server agreement). */
function doorHash(a: number, b: number, c: number, d: number): number {
  let h = 2166136261
  for (const v of [a, b, c, d]) {
    h = Math.imul(h ^ (v >>> 0), 16777619)
  }
  return ((h >>> 0) % 1000) / 1000
}

function shaftRect(shaft: DungeonShaft, ctx: DungeonGeoCtx) {
  return shaft.alongZ
    ? { x: shaft.x, z: shaft.z, w: ctx.shaftW, d: ctx.shaftLen }
    : { x: shaft.x, z: shaft.z, w: ctx.shaftLen, d: ctx.shaftW }
}

/** Half-open cell membership: is cell (x, z) inside rect r ({x, z, w, d})?
 *  Shared by shaft and room tests; mirrors the Rust convention. */
function rectContains(
  r: { x: number; z: number; w: number; d: number },
  x: number,
  z: number
): boolean {
  return x >= r.x && x < r.x + r.w && z >= r.z && z < r.z + r.d
}

function shaftContains(
  shaft: DungeonShaft,
  ctx: DungeonGeoCtx,
  x: number,
  z: number
): boolean {
  return rectContains(shaftRect(shaft, ctx), x, z)
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

  // Steps as a single watertight solid (a stepped prism) rather than one
  // closed box per tread. Stacked boxes share internal faces that are hidden
  // when opaque but show through once the up-shaft fades to a ghost; a single
  // hull keeps only the outer surface (treads, risers, end faces, the two
  // stepped side profiles, and a flat underside). Opaque look is unchanged.
  {
    const w = ctx.shaftW
    const hw = w / 2
    const endU = runStart + stepCount * stepDepth
    const treadY = (i: number) => topY - (i + 0.5) * stepRise
    const tAt = (i: number) => runStart + i * stepDepth
    // Local point for run-coordinate t, height y, lateral offset latOff.
    const pt = (t: number, y: number, latOff: number) => {
      const run = runAt(t)
      return shaft.alongZ
        ? new THREE.Vector3(latCenter + latOff, y, run)
        : new THREE.Vector3(run, y, latCenter + latOff)
    }

    const positions: number[] = []
    const normals: number[] = []
    const uvs: number[] = []
    const indices: number[] = []
    // Quad in CCW order around its rectangle; winding is corrected against the
    // outward normal, and UVs use the same axis-projection as addBox (scaled).
    const addQuad = (
      c0: THREE.Vector3,
      c1: THREE.Vector3,
      c2: THREE.Vector3,
      c3: THREE.Vector3,
      n: THREE.Vector3
    ) => {
      const base = positions.length / 3
      const gn = c1.clone().sub(c0).cross(c2.clone().sub(c0))
      const verts = gn.dot(n) < 0 ? [c0, c3, c2, c1] : [c0, c1, c2, c3]
      const ax = Math.abs(n.x)
      const ay = Math.abs(n.y)
      const az = Math.abs(n.z)
      for (const c of verts) {
        positions.push(c.x, c.y, c.z)
        normals.push(n.x, n.y, n.z)
        let u: number, v: number
        if (ax >= ay && ax >= az) {
          u = c.z
          v = c.y
        } else if (ay >= ax && ay >= az) {
          u = c.x
          v = c.z
        } else {
          u = c.x
          v = c.y
        }
        uvs.push(u * DUNGEON_FLOOR_UV_SCALE, v * DUNGEON_FLOOR_UV_SCALE)
      }
      indices.push(base, base + 1, base + 2, base, base + 2, base + 3)
    }

    // Run-axis world direction (+t) and lateral axis, accounting for reversed.
    const sgn = shaft.reversed ? -1 : 1
    const runDir = shaft.alongZ
      ? new THREE.Vector3(0, 0, sgn)
      : new THREE.Vector3(sgn, 0, 0)
    const minusRun = runDir.clone().negate()
    const latAxis = shaft.alongZ
      ? new THREE.Vector3(1, 0, 0)
      : new THREE.Vector3(0, 0, 1)
    const downN = new THREE.Vector3(0, -1, 0)
    const upN = new THREE.Vector3(0, 1, 0)

    for (let i = 0; i < stepCount; i++) {
      const ty = treadY(i)
      // Stepped side profile on both lateral faces (one rectangle per tread).
      for (const s of [-1, 1] as const) {
        const n = latAxis.clone().multiplyScalar(s)
        addQuad(
          pt(tAt(i), bottomY, s * hw),
          pt(tAt(i + 1), bottomY, s * hw),
          pt(tAt(i + 1), ty, s * hw),
          pt(tAt(i), ty, s * hw),
          n
        )
      }
      // Tread (top).
      addQuad(
        pt(tAt(i), ty, -hw),
        pt(tAt(i + 1), ty, -hw),
        pt(tAt(i + 1), ty, hw),
        pt(tAt(i), ty, hw),
        upN
      )
      // Riser to the next (lower) tread, facing down-run.
      if (i < stepCount - 1) {
        const tb = tAt(i + 1)
        addQuad(
          pt(tb, treadY(i + 1), -hw),
          pt(tb, treadY(i + 1), hw),
          pt(tb, ty, hw),
          pt(tb, ty, -hw),
          runDir
        )
      }
    }
    // Underside (flat at the bottom landing).
    addQuad(
      pt(runStart, bottomY, -hw),
      pt(endU, bottomY, -hw),
      pt(endU, bottomY, hw),
      pt(runStart, bottomY, hw),
      downN
    )
    // Shallow-end face (under the top landing) and deep-end face.
    addQuad(
      pt(runStart, bottomY, -hw),
      pt(runStart, bottomY, hw),
      pt(runStart, treadY(0), hw),
      pt(runStart, treadY(0), -hw),
      minusRun
    )
    addQuad(
      pt(endU, bottomY, -hw),
      pt(endU, bottomY, hw),
      pt(endU, treadY(stepCount - 1), hw),
      pt(endU, treadY(stepCount - 1), -hw),
      runDir
    )

    const geo = new THREE.BufferGeometry()
    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3))
    geo.setAttribute('normal', new THREE.Float32BufferAttribute(normals, 3))
    geo.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2))
    geo.setIndex(indices)
    entries.push({ geo, textureIndex: DUNGEON_FLOOR_TEXTURE_IDX })
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
  // side (faces south). Vertical span runs from this floor's slab (topY)
  // straight down to the floor below (bottomY) — it does NOT rise to the
  // current floor's wall/ceiling height. Skipped for the surface entrance,
  // which supplies its own non-protruding pit walls.
  if (includeWall) {
    const wallTex = DUNGEON_WALL_TEXTURE_IDX
    const wallH = topY - bottomY
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

/** One straight wall run, built as its own mesh so the dungeon layer can fade
 *  just this run to a ghost when it occludes the player. */
export interface WallRun {
  mesh: THREE.Mesh
  /** Group-local AABB; the layer adds the floor group's world position. */
  localAABB: THREE.Box3
}

export interface DungeonFloorGroup {
  group: THREE.Group
  /** Local-space AABB of the up-shaft stairs sub-group, for the layer's
   *  occlusion-fade test (add the group's world position to use it). */
  upShaftAABB: THREE.Box3
  /** Per-side wall runs (all four directions), faded individually on occlusion. */
  wallRuns: WallRun[]
  /** Interior room doors (corridor mouths in north/east room walls), opened by
   *  the layer on player proximity. */
  doors: InteriorDoor[]
}

/**
 * Build the renderable group for one dungeon floor. The caller positions
 * it at (originX, floorY(depth), originZ) in world space.
 */
export function buildDungeonFloorGroup(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx
): DungeonFloorGroup {
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

  // Walls on all four sides are built lower down as per-run fade meshes (the
  // dungeon layer ghosts any run that occludes the player), not merged here.

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

  // --- Down shaft (0 → -floorHeight) merges with the floor geometry, so it
  // shares the slab's y=0 and isn't lifted for shadow contact like the up-shaft;
  // any peter-panning at the hole's top edge is deferred (split it out to fix).
  if (down) {
    collectShaftStairs(entries, down, ctx, 0, -ctx.floorHeight, false, true)
  }

  const group = new THREE.Group()
  addMergedMeshes(group, entries)

  // --- Up shaft (descends from the floor above, +floorHeight → 0): the
  // staircase you arrive by. Built into its own sub-group so the dungeon layer
  // can fade it to a ghost material when it occludes the player from the iso
  // camera (it shares the floor texture, so it can't fade while merged in).
  // Its side wall is omitted: the steps are blocked by an impassable flag, so
  // no wall is needed to contain the player, and a wall would only block the
  // view down the stairs.
  const upEntries: GeoEntry[] = []
  collectShaftStairs(
    upEntries,
    layout.upShaft,
    ctx,
    ctx.floorHeight,
    0,
    true, // top landing: neighbour floor's slab is not rendered
    false, // bottom landing: this floor's slab covers the exit row
    false // no side wall
  )
  const upGroup = new THREE.Group()
  upGroup.name = UP_SHAFT_GROUP_NAME
  // Lift off the floor (SHADOW_CONTACT_LIFT); collision uses the server ramp Y, so
  // the sub-centimetre visual shift doesn't affect it.
  upGroup.position.y = SHADOW_CONTACT_LIFT
  addMergedMeshes(upGroup, upEntries)
  group.add(upGroup)

  // --- Wall runs (all four sides): one mesh per straight run (a run breaks at
  // a corner, a doorway gap or a shaft cell) in their own sub-group, so the
  // dungeon layer can ghost just the runs that occlude the player. The
  // camera-facing south/west runs fade often; the far north/east runs fade only
  // when the layout puts them between the iso camera and the player. They are
  // decorative — collision is server-side — so they cast no shadow (a ghosted
  // run would otherwise drop a wall-less shadow on the floor) and ignore click
  // raycasts (so click-to-move targets the floor behind them).
  const wallRunGroup = new THREE.Group()
  wallRunGroup.name = WALL_RUN_GROUP_NAME
  const wallRuns: WallRun[] = []
  const addWallRun = (
    texIdx: number,
    w: number,
    h: number,
    d: number,
    cx: number,
    cy: number,
    cz: number
  ) => {
    const e: GeoEntry[] = []
    addBox(e, texIdx, w, h, d, cx, cy, cz)
    const geo = e[0].geo
    const mesh = new THREE.Mesh(geo, getHousingMaterial(texIdx))
    mesh.castShadow = false
    mesh.receiveShadow = true
    mesh.raycast = () => {}
    mesh.userData.textureIndex = texIdx
    geo.computeBoundingBox()
    wallRunGroup.add(mesh)
    wallRuns.push({ mesh, localAABB: geo.boundingBox!.clone() })
  }
  // Room cells keep the medieval-stone wall; carved cells in no room are
  // corridors and get the rock-wall corridor texture. Matches the Rust
  // `cell_in_any_room` convention (half-open rectangles). Shaft cells emit no
  // wall (filtered by `inAnyShaft`), so they need no classification here.
  const roomAt = (x: number, z: number) =>
    layout.rooms.some((r) => rectContains(r, x, z))
  const wallTexAt = (x: number, z: number) =>
    roomAt(x, z) ? DUNGEON_WALL_TEXTURE_IDX : DUNGEON_CORRIDOR_WALL_TEXTURE_IDX
  // Pull a corridor run in by the wall thickness at each end where a perpendicular
  // room wall crosses, so the two coplanar faces don't z-fight (the room wall keeps
  // the corner). `diagLo`/`diagHi` are the cells just past each end on the wall
  // side; a room cell there means a room wall crosses. Colinear continuations and
  // corridor↔corridor corners have a solid/corridor cell there, so they stay
  // full-length (no gap). Returns the trimmed run span [lo, hi].
  const trimCorridorRun = (
    tex: number,
    lo: number,
    hi: number,
    diagLo: [number, number],
    diagHi: [number, number]
  ): [number, number] =>
    tex !== DUNGEON_CORRIDOR_WALL_TEXTURE_IDX
      ? [lo, hi]
      : [
          roomAt(diagLo[0], diagLo[1]) ? lo + WALL_THICKNESS : lo,
          roomAt(diagHi[0], diagHi[1]) ? hi - WALL_THICKNESS : hi,
        ]
  // North/south edges merge into x-runs (one wall per row); the wall sits just
  // past the carved cell's north (z − HALF) or south (z + 1 + HALF) face.
  for (let z = 0; z < grid; z++) {
    let northStart = -1
    let northTex = -1
    let southStart = -1
    let southTex = -1
    for (let x = 0; x <= grid; x++) {
      const carved = x < grid && carvedAt(x, z) && !inAnyShaft(x, z)
      const north = carved && !carvedAt(x, z - 1)
      const south = carved && !carvedAt(x, z + 1)
      const tex = carved ? wallTexAt(x, z) : -1
      // Close a run at a gap, corner, or where room↔corridor texture flips.
      if (northStart >= 0 && (!north || tex !== northTex)) {
        const [lo, hi] = trimCorridorRun(
          northTex,
          northStart,
          x,
          [northStart - 1, z - 1],
          [x, z - 1]
        )
        const len = hi - lo
        addWallRun(
          northTex,
          len,
          ctx.wallHeight,
          WALL_THICKNESS,
          lo + len / 2,
          ctx.wallHeight / 2 + SHADOW_CONTACT_LIFT,
          z - WALL_HALF_THICKNESS
        )
        northStart = -1
      }
      if (north && northStart < 0) {
        northStart = x
        northTex = tex
      }
      if (southStart >= 0 && (!south || tex !== southTex)) {
        const [lo, hi] = trimCorridorRun(
          southTex,
          southStart,
          x,
          [southStart - 1, z + 1],
          [x, z + 1]
        )
        const len = hi - lo
        addWallRun(
          southTex,
          len,
          ctx.wallHeight,
          WALL_THICKNESS,
          lo + len / 2,
          ctx.wallHeight / 2 + SHADOW_CONTACT_LIFT,
          z + 1 + WALL_HALF_THICKNESS
        )
        southStart = -1
      }
      if (south && southStart < 0) {
        southStart = x
        southTex = tex
      }
    }
  }
  // East/west edges merge into z-runs; the wall sits just past the carved cell's
  // east (x + 1 + HALF) or west (x − HALF) face.
  for (let x = 0; x < grid; x++) {
    let eastStart = -1
    let eastTex = -1
    let westStart = -1
    let westTex = -1
    for (let z = 0; z <= grid; z++) {
      const carved = z < grid && carvedAt(x, z) && !inAnyShaft(x, z)
      const east = carved && !carvedAt(x + 1, z)
      const west = carved && !carvedAt(x - 1, z)
      const tex = carved ? wallTexAt(x, z) : -1
      if (eastStart >= 0 && (!east || tex !== eastTex)) {
        const [lo, hi] = trimCorridorRun(
          eastTex,
          eastStart,
          z,
          [x + 1, eastStart - 1],
          [x + 1, z]
        )
        const len = hi - lo
        addWallRun(
          eastTex,
          WALL_THICKNESS,
          ctx.wallHeight,
          len,
          x + 1 + WALL_HALF_THICKNESS,
          ctx.wallHeight / 2 + SHADOW_CONTACT_LIFT,
          lo + len / 2
        )
        eastStart = -1
      }
      if (east && eastStart < 0) {
        eastStart = z
        eastTex = tex
      }
      if (westStart >= 0 && (!west || tex !== westTex)) {
        const [lo, hi] = trimCorridorRun(
          westTex,
          westStart,
          z,
          [x - 1, westStart - 1],
          [x - 1, z]
        )
        const len = hi - lo
        addWallRun(
          westTex,
          WALL_THICKNESS,
          ctx.wallHeight,
          len,
          x - WALL_HALF_THICKNESS,
          ctx.wallHeight / 2 + SHADOW_CONTACT_LIFT,
          lo + len / 2
        )
        westStart = -1
      }
      if (west && westStart < 0) {
        westStart = z
        westTex = tex
      }
    }
  }
  group.add(wallRunGroup)

  // --- Interior room doors: a corridor mouth in a room's north or east wall
  // gets a double door (same shape as the surface entrance) with a 30% chance.
  // Placement is hashed from the opening's coordinates so it's stable across
  // re-renders and matches the door-id the toggle packet uses. The arch above
  // (room-wall texture) merges statically; the swinging leaves are returned so
  // the layer can open them on click and the manager can block them when shut.
  const isCorridor = (x: number, z: number) =>
    carvedAt(x, z) && !roomAt(x, z) && !inAnyShaft(x, z)
  const doorMat = getHousingMaterial(DUNGEON_DOOR_TEXTURE_IDX)
  const archEntries: GeoEntry[] = []
  const doors: InteriorDoor[] = []
  const tryDoorway = (
    northWall: boolean,
    lat0: number,
    len: number,
    wallLine: number
  ) => {
    if (
      doorHash(layout.depth, northWall ? 0 : 1, lat0, wallLine) * 100 >=
      INTERIOR_DOOR_PCT
    )
      return
    doors.push(
      buildInteriorDoor(
        layout.depth,
        northWall,
        lat0,
        len,
        wallLine,
        ctx.wallHeight,
        doorMat,
        archEntries
      )
    )
  }
  for (const room of layout.rooms) {
    // North wall (z = room.z): scan its row for maximal runs of cells whose
    // north neighbour is a corridor — the corridor mouth, i.e. a doorway gap.
    {
      const zr = room.z
      let start = -1
      for (let x = room.x; x <= room.x + room.w; x++) {
        const open =
          x < room.x + room.w && carvedAt(x, zr) && isCorridor(x, zr - 1)
        if (open && start < 0) start = x
        if (!open && start >= 0) {
          tryDoorway(true, start, x - start, zr)
          start = -1
        }
      }
    }
    // East wall (cells x = room.x + room.w − 1; wall line one cell further):
    // scan its column for runs whose east neighbour is a corridor.
    {
      const xr = room.x + room.w - 1
      const wallLine = room.x + room.w
      let start = -1
      for (let z = room.z; z <= room.z + room.d; z++) {
        const open =
          z < room.z + room.d && carvedAt(xr, z) && isCorridor(xr + 1, z)
        if (open && start < 0) start = z
        if (!open && start >= 0) {
          tryDoorway(false, start, z - start, wallLine)
          start = -1
        }
      }
    }
  }
  // Arches merge into the floor group but stay non-pickable, so a ground click
  // near a doorway falls through to the floor. The door leaves are NOT added
  // here — the layer parents them to a separate pickable group (so the door
  // click raycast can hit them without them intercepting click-to-move).
  if (archEntries.length > 0) {
    const archGroup = new THREE.Group()
    addMergedMeshes(archGroup, archEntries)
    archGroup.traverse((o) => {
      if (o instanceof THREE.Mesh) o.raycast = () => {}
    })
    group.add(archGroup)
  }

  // Local-space occlusion AABB: the shaft footprint from this floor (y=0) up to
  // the floor above. The layer adds the group's world position before testing.
  const ur = shaftRect(layout.upShaft, ctx)
  const upShaftAABB = new THREE.Box3(
    new THREE.Vector3(ur.x, 0, ur.z),
    new THREE.Vector3(ur.x + ur.w, ctx.floorHeight, ur.z + ur.d)
  )
  return { group, upShaftAABB, wallRuns, doors }
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
