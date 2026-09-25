import * as THREE from 'three'
import { addMergedMeshes, type GeoEntry } from './house-geo-utils'
import { getGhostHousingMaterial, getHousingMaterial } from './housing-textures'
import type {
  DungeonFloorLayout,
  InteriorDoorSpec,
} from '../managers/dungeonManager'
import { addBox, quadMeshBuilder } from './dungeon-geo-primitives'
import { buildCaveWall } from './dungeon-geo-cave'
import { buildDungeonWallWeathering } from './dungeon-wall-weathering'
import {
  buildMasonryWall,
  buildMasonryWallGhost,
  masonryFloorDamageBuilder,
} from './dungeon-geo-masonry'
import { dungeonFloorClearance } from './dungeon-floor-clearance'
import { buildDungeonFloorRubble } from './dungeon-geo-rubble'
import { dungeonCaveSeed, dungeonCaveTheme } from './dungeon-cave-themes'
import {
  shaftRect,
  rectContains,
  shaftContains,
  shaftStepCell,
  collectShaftStairs,
} from './dungeon-geo-shaft'
import { buildInteriorDoor, type InteriorDoor } from './dungeon-geo-doors'
import {
  DUNGEON_FLOOR_TEXTURE_IDX,
  DUNGEON_WALL_TEXTURE_IDX,
  DUNGEON_MASONRY_BACK_TEXTURE_IDX,
  DUNGEON_WALL_WEATHERING_TEXTURE_IDX,
  DUNGEON_WALL_DETAILS_TEXTURE_IDX,
  SLAB_THICKNESS,
  DUNGEON_FLOOR_UV_SCALE,
  SHADOW_CONTACT_LIFT,
  WALL_THICKNESS,
  WALL_HALF_THICKNESS,
  UP_SHAFT_GROUP_NAME,
  type DungeonGeoCtx,
} from './dungeon-geo-constants'

const WALL_RUN_GROUP_NAME = 'wallRuns'

/** A wall run that fades when it occludes the player. */
export interface WallRun {
  mesh: THREE.Mesh
  ghostMesh?: THREE.Mesh
  weathering?: THREE.Mesh
  /** Group-local AABB; the layer adds the floor group's world position. */
  localAABB: THREE.Box3
  /** South/west walls in a room or connected corridor corner fade together. */
  fadeGroup: number
}

export interface DungeonFloorGroup {
  group: THREE.Group
  /** Group-local bounds for the up-shaft occlusion test. */
  upShaftAABB: THREE.Box3
  /** Per-side wall runs (all four directions), faded individually on occlusion. */
  wallRuns: WallRun[]
  /** Interior room doors at corridor mouths, animated by the layer. */
  doors: InteriorDoor[]
}

export function buildDungeonFloorGroup(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx,
  doorSpecs: InteriorDoorSpec[],
  dungeonId = ''
): DungeonFloorGroup {
  const grid = ctx.grid
  const cave = dungeonCaveTheme(dungeonId, layout.depth)
  const caveSeed = dungeonCaveSeed(dungeonId, layout.depth)
  const roomIndexAt = (x: number, z: number) =>
    layout.rooms.findIndex((r) => rectContains(r, x, z))
  const roomAt = (x: number, z: number) => roomIndexAt(x, z) >= 0
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

  // Slab skirts only cover exposed edges, avoiding internal shadow seams.
  const solidAt = (x: number, z: number) => carvedAt(x, z) && !inDownHole(x, z)
  const floorTexAt = (x: number, z: number) =>
    roomAt(x, z) || inAnyShaft(x, z)
      ? DUNGEON_FLOOR_TEXTURE_IDX
      : cave.floorTexture
  const slabs = new Map<number, ReturnType<typeof quadMeshBuilder>>()
  const masonryFloor =
    cave.id === 'masonry'
      ? masonryFloorDamageBuilder(
          cave.floorTexture,
          caveSeed,
          dungeonFloorClearance(layout, ctx)
        )
      : null
  const v = (x: number, y: number, z: number) => new THREE.Vector3(x, y, z)
  const yB = -SLAB_THICKNESS
  for (let z = 0; z < grid; z++) {
    let runStart = -1
    let runN = false
    let runS = false
    let runTex = -1
    for (let x = 0; x <= grid; x++) {
      const solid = x < grid && solidAt(x, z)
      const nOpen = solid && !solidAt(x, z - 1)
      const sOpen = solid && !solidAt(x, z + 1)
      const tex = solid ? floorTexAt(x, z) : -1
      if (
        runStart >= 0 &&
        (!solid || nOpen !== runN || sOpen !== runS || tex !== runTex)
      ) {
        let slab = slabs.get(runTex)
        if (!slab) {
          slab = quadMeshBuilder(DUNGEON_FLOOR_UV_SCALE)
          slabs.set(runTex, slab)
        }
        const x0 = runStart
        const cap = (y: number, n: THREE.Vector3) =>
          slab.addQuad(
            v(x0, y, z),
            v(x, y, z),
            v(x, y, z + 1),
            v(x0, y, z + 1),
            n
          )
        const skirtZ = (za: number, n: THREE.Vector3) =>
          slab.addQuad(
            v(x0, yB, za),
            v(x, yB, za),
            v(x, 0, za),
            v(x0, 0, za),
            n
          )
        const skirtX = (xa: number, n: THREE.Vector3) =>
          slab.addQuad(
            v(xa, yB, z),
            v(xa, yB, z + 1),
            v(xa, 0, z + 1),
            v(xa, 0, z),
            n
          )
        cap(0, v(0, 1, 0))
        if (masonryFloor && runTex === cave.floorTexture) {
          for (let column = x0; column < x; column++)
            masonryFloor.addCell(column, z)
        }
        cap(yB, v(0, -1, 0))
        if (runN) skirtZ(z, v(0, 0, -1))
        if (runS) skirtZ(z + 1, v(0, 0, 1))
        if (!solidAt(x0 - 1, z)) skirtX(x0, v(-1, 0, 0))
        if (!solidAt(x, z)) skirtX(x, v(1, 0, 0))
        runStart = -1
      }
      if (solid && runStart < 0) {
        runStart = x
        runN = nOpen
        runS = sOpen
        runTex = tex
      }
    }
  }
  for (const [tex, slab] of slabs) slab.finish(entries, tex)

  if (down) {
    collectShaftStairs(entries, down, ctx, 0, -ctx.floorHeight, false, true)
  }

  const group = new THREE.Group()
  addMergedMeshes(group, entries)
  if (masonryFloor) {
    const damageEntries: GeoEntry[] = []
    masonryFloor.finish(damageEntries)
    const damageGroup = new THREE.Group()
    damageGroup.name = 'masonryFloorDamage'
    addMergedMeshes(damageGroup, damageEntries)
    for (const child of damageGroup.children) {
      const mesh = child as THREE.Mesh
      mesh.castShadow = false
      mesh.raycast = () => {}
    }
    group.add(damageGroup)
  }
  group.add(buildDungeonFloorRubble(layout, ctx, dungeonId))

  // Keep the up-shaft separate for occlusion fading.
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
  // Visual lift; collision follows the server ramp.
  upGroup.position.y = SHADOW_CONTACT_LIFT
  addMergedMeshes(upGroup, upEntries)
  group.add(upGroup)

  // Wall runs fade independently or by group, without shadows or click blocking.
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
    cz: number,
    fadeGroup = -1,
    inward = 1
  ) => {
    let geo: THREE.BufferGeometry
    let ghostGeometry: THREE.BufferGeometry | undefined
    const alongX = w > d
    const length = alongX ? w : d
    const center = alongX ? cx : cz
    const lo = center - length / 2
    const hi = center + length / 2
    const boundary = (alongX ? cz : cx) + inward * WALL_HALF_THICKNESS
    if (texIdx === cave.wallTexture) {
      const buildWall = cave.id === 'masonry' ? buildMasonryWall : buildCaveWall
      geo = buildWall(alongX, lo, hi, boundary, inward, h, caveSeed)
      if (cave.id === 'masonry') {
        ghostGeometry = buildMasonryWallGhost(alongX, lo, hi, boundary, h)
      }
    } else {
      const e: GeoEntry[] = []
      addBox(e, texIdx, w, h, d, cx, cy, cz)
      geo = e[0].geo
    }
    const weatheringGeometry = buildDungeonWallWeathering(geo, {
      alongX,
      lo,
      hi,
      boundary,
      inward,
      height: h,
      seed: caveSeed,
    })
    const material = getHousingMaterial(texIdx)
    const mesh = new THREE.Mesh(
      geo,
      ghostGeometry
        ? [material, getHousingMaterial(DUNGEON_MASONRY_BACK_TEXTURE_IDX)]
        : material
    )
    mesh.castShadow = false
    mesh.receiveShadow = true
    mesh.raycast = () => {}
    mesh.userData.textureIndex = texIdx
    geo.computeBoundingBox()
    wallRunGroup.add(mesh)
    let weathering: THREE.Mesh | undefined
    if (weatheringGeometry) {
      weathering = new THREE.Mesh(weatheringGeometry, [
        getHousingMaterial(DUNGEON_WALL_WEATHERING_TEXTURE_IDX),
        getHousingMaterial(DUNGEON_WALL_DETAILS_TEXTURE_IDX),
      ])
      weathering.name = 'wallWeathering'
      weathering.receiveShadow = true
      weathering.raycast = () => {}
      mesh.add(weathering)
    }
    let ghostMesh: THREE.Mesh | undefined
    if (ghostGeometry) {
      ghostMesh = new THREE.Mesh(ghostGeometry, getGhostHousingMaterial(texIdx))
      ghostMesh.visible = false
      ghostMesh.raycast = () => {}
      ghostMesh.userData.textureIndex = texIdx
      wallRunGroup.add(ghostMesh)
    }
    wallRuns.push({
      mesh,
      ghostMesh,
      weathering,
      localAABB: geo.boundingBox!.clone(),
      fadeGroup,
    })
  }
  // Corridor groups join at grid corners, independent of wall thickness.
  const corridorParents = new Map<number, number>()
  const resolveFadeGroup = (id: number): number => {
    const parent = corridorParents.get(id)
    if (parent === undefined) return id
    const group = resolveFadeGroup(parent)
    corridorParents.set(id, group)
    return group
  }
  const wallFadeGroup = (
    room: number,
    x0: number,
    z0: number,
    x1: number,
    z1: number
  ): number => {
    if (room >= 0) return room
    const start = resolveFadeGroup(layout.rooms.length + x0 + z0 * (grid + 1))
    const end = resolveFadeGroup(layout.rooms.length + x1 + z1 * (grid + 1))
    if (start !== end) corridorParents.set(end, start)
    return start
  }

  // Carved cells outside rooms use the corridor texture; shafts emit no walls.
  const wallTexAt = (x: number, z: number) =>
    roomAt(x, z) ? DUNGEON_WALL_TEXTURE_IDX : cave.wallTexture
  // Trim corridor ends where perpendicular room walls cross.
  const trimCorridorRun = (
    tex: number,
    lo: number,
    hi: number,
    diagLo: [number, number],
    diagHi: [number, number]
  ): [number, number] =>
    tex !== cave.wallTexture
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
          z + 1 + WALL_HALF_THICKNESS,
          wallFadeGroup(
            roomIndexAt(southStart, z),
            southStart,
            z + 1,
            x,
            z + 1
          ),
          -1
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
          lo + len / 2,
          -1,
          -1
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
          lo + len / 2,
          wallFadeGroup(roomIndexAt(x, westStart), x, westStart, x, z)
        )
        westStart = -1
      }
      if (west && westStart < 0) {
        westStart = z
        westTex = tex
      }
    }
  }
  for (const run of wallRuns) run.fadeGroup = resolveFadeGroup(run.fadeGroup)
  group.add(wallRunGroup)

  // Door arches are static; the layer animates the leaves.
  const archEntries: GeoEntry[] = []
  const doors: InteriorDoor[] = doorSpecs.map((spec) =>
    buildInteriorDoor(layout.depth, spec, ctx.wallHeight, archEntries)
  )
  // Ground clicks pass through arches; door leaves have their own pickable group.
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
