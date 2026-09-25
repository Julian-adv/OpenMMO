import { MathUtils } from 'three'
import { get } from 'svelte/store'
import { estateChests } from '../stores/estateFurnitureStore'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import { unwrapWorldXNear } from '../terrain/world-wrap'
import { apiFetch, getTerrainApiUrl } from '../utils/networkUtils'
import type {
  ObjectDef,
  ObjectPlacement,
  ObjectRegionData,
} from '../stores/editorStore'
import type { Position } from '../network/networkTypes'
import { createEvent } from '../network/networkEvents'
import {
  TERRAIN_TILE_SIZE,
  worldRectToTileBounds,
} from '../components/game-scene/terrain-utils'
import { regionKey, tileToRegion } from '../terrain/terrain-constants'
import { loadGLB } from '../utils/gltfCache'
import { getObjectModelPath } from '../utils/modelPaths'
import { detectFootprint, type FootprintData } from '../utils/objectFootprint'
import type { HouseData } from '../types/housing'
import { roomContainsXZ } from './housing-queries'

export interface ChangedObjectRegion {
  rx: number
  rz: number
  data: ObjectRegionData
}

const HOUSE_SIGN_ATTACH_DISTANCE = 1

export function isObjectInsideHouse(
  placement: ObjectPlacement,
  house: HouseData
): boolean {
  return house.rooms.some((room) => {
    if (
      room.roomType !== 'normal' ||
      room.floorLevel !== (placement.floorLevel ?? 0)
    ) {
      return false
    }
    return roomContainsXZ(house, room, placement.x, placement.z)
  })
}

export function shouldMoveObjectWithHouse(
  placement: ObjectPlacement,
  house: HouseData
): boolean {
  if (isObjectInsideHouse(placement, house)) return true
  if (!placement.type.startsWith('shop_sign')) return false

  return house.rooms.some((room) => {
    if (
      room.roomType !== 'normal' ||
      room.floorLevel !== (placement.floorLevel ?? 0)
    ) {
      return false
    }
    const minX = house.origin.x + room.localX
    const minZ = house.origin.z + room.localZ
    const maxX = minX + room.sizeX
    const maxZ = minZ + room.sizeZ
    const dx = Math.max(minX - placement.x, 0, placement.x - maxX)
    const dz = Math.max(minZ - placement.z, 0, placement.z - maxZ)
    return Math.hypot(dx, dz) <= HOUSE_SIGN_ATTACH_DISTANCE
  })
}

export class ObjectManager {
  private cache = new Map<string, ObjectRegionData>()
  private terrainApiUrl: string
  private catalogCache: ObjectDef[] | null = null
  private footprintCache = new Map<string, FootprintData>()
  private regionChanged = createEvent<(region: ChangedObjectRegion) => void>()
  private worldReset = createEvent<() => void>()
  private generation = 0

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  async fetchCatalog(): Promise<ObjectDef[]> {
    if (this.catalogCache) return this.catalogCache
    const resp = await fetch('/models/objects/catalog.json')
    const data: ObjectDef[] = await resp.json()
    this.catalogCache = data
    return data
  }

  async fetchFootprint(objectType: string): Promise<FootprintData | null> {
    const cached = this.footprintCache.get(objectType)
    if (cached) return cached
    await this.fetchCatalog()
    const def = this.getCatalogEntry(objectType)
    if (!def || !def.model) return null
    const gltf = await loadGLB(getObjectModelPath(def.model))
    const data = detectFootprint(gltf.scene)
    this.footprintCache.set(objectType, data)
    return data
  }

  async fetchObject(rx: number, rz: number): Promise<ObjectRegionData> {
    const key = regionKey(rx, rz)
    const cached = this.cache.get(key)
    if (cached) return cached

    const generation = this.generation
    const data = await this.loadObjectRegion(rx, rz)
    if (generation !== this.generation) return this.fetchObject(rx, rz)
    this.cache.set(key, data)
    return data
  }

  resetWorld(): void {
    this.generation++
    this.cache.clear()
    this.worldReset.emit()
  }

  onWorldReset(cb: () => void): () => void {
    return this.worldReset.on(cb)
  }

  private async loadObjectRegion(
    rx: number,
    rz: number
  ): Promise<ObjectRegionData> {
    const response = await fetch(
      `${this.terrainApiUrl}/api/terrain/objects/${rx}/${rz}`
    )
    if (!response.ok) {
      throw new Error(`object region ${rx},${rz} load failed`)
    }
    const json = await response.json()
    return { placements: json.placements ?? [] }
  }

  async saveObject(
    rx: number,
    rz: number,
    data: ObjectRegionData
  ): Promise<boolean> {
    const response = await apiFetch(
      `${this.terrainApiUrl}/api/terrain/objects/${rx}/${rz}`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(data),
      }
    )
    if (!response.ok) return false
    this.cache.set(regionKey(rx, rz), data)
    this.regionChanged.emit({ rx, rz, data })
    return true
  }

  onRegionChanged(cb: (region: ChangedObjectRegion) => void): () => void {
    return this.regionChanged.on(cb)
  }

  async moveHouseContents(
    house: HouseData,
    deltaX: number,
    deltaZ: number
  ): Promise<boolean> {
    const sourceRegions = new Map<string, { rx: number; rz: number }>()
    for (const room of house.rooms) {
      if (room.roomType !== 'normal') continue
      const bounds = worldRectToTileBounds(
        house.origin.x + room.localX - HOUSE_SIGN_ATTACH_DISTANCE,
        house.origin.z + room.localZ - HOUSE_SIGN_ATTACH_DISTANCE,
        house.origin.x + room.localX + room.sizeX + HOUSE_SIGN_ATTACH_DISTANCE,
        house.origin.z + room.localZ + room.sizeZ + HOUSE_SIGN_ATTACH_DISTANCE
      )
      for (let tz = bounds.tileMinZ; tz <= bounds.tileMaxZ; tz++) {
        for (let tx = bounds.tileMinX; tx <= bounds.tileMaxX; tx++) {
          const rx = tileToRegion(tx)
          const rz = tileToRegion(tz)
          sourceRegions.set(regionKey(rx, rz), { rx, rz })
        }
      }
    }

    const originals = new Map<string, ChangedObjectRegion>()
    const working = new Map<string, ChangedObjectRegion>()
    const ensureWorking = async (rx: number, rz: number) => {
      const key = regionKey(rx, rz)
      const cached = working.get(key)
      if (cached) return cached
      const original = await this.loadObjectRegion(rx, rz)
      const entry = { rx, rz, data: structuredClone(original) }
      originals.set(key, { rx, rz, data: original })
      working.set(key, entry)
      return entry
    }

    await Promise.all(
      [...sourceRegions.values()].map(({ rx, rz }) => ensureWorking(rx, rz))
    )

    const moved: ObjectPlacement[] = []
    const changedKeys = new Set<string>()
    for (const [key, region] of working) {
      const kept: ObjectPlacement[] = []
      for (const placement of region.data.placements) {
        if (shouldMoveObjectWithHouse(placement, house)) {
          moved.push({
            ...placement,
            x: placement.x + deltaX,
            z: placement.z + deltaZ,
          })
          changedKeys.add(key)
        } else {
          kept.push(placement)
        }
      }
      region.data.placements = kept
    }
    if (moved.length === 0) return true

    for (const placement of moved) {
      const bounds = worldRectToTileBounds(
        placement.x,
        placement.z,
        placement.x,
        placement.z
      )
      const rx = tileToRegion(bounds.tileMinX)
      const rz = tileToRegion(bounds.tileMinZ)
      const destination = await ensureWorking(rx, rz)
      destination.data.placements.push(placement)
      changedKeys.add(regionKey(rx, rz))
    }

    const changed = [...changedKeys].map((key) => working.get(key)!)
    const saved: ChangedObjectRegion[] = []
    try {
      for (const region of changed) {
        if (!(await this.saveObject(region.rx, region.rz, region.data))) {
          throw new Error(
            `object region ${region.rx},${region.rz} update failed`
          )
        }
        saved.push(region)
      }
      return true
    } catch (error) {
      await Promise.allSettled(
        saved.map((region) => {
          const original = originals.get(regionKey(region.rx, region.rz))!
          return this.saveObject(original.rx, original.rz, original.data)
        })
      )
      console.error('Failed to move house contents:', error)
      return false
    }
  }

  getCached(rx: number, rz: number): ObjectRegionData | null {
    return this.cache.get(regionKey(rx, rz)) ?? null
  }

  invalidate(rx: number, rz: number): void {
    this.cache.delete(regionKey(rx, rz))
  }

  /** Look up a object definition by type id (e.g. "bed"). Returns null if catalog not loaded or not found. */
  getCatalogEntry(objectType: string): ObjectDef | null {
    if (!this.catalogCache) return null
    return this.catalogCache.find((d) => d.id === objectType) ?? null
  }

  /** Prefer the server's placement ID, then the nearest object of that type. */
  findNearestPlacement(
    objectType: string,
    wx: number,
    wz: number,
    objectId?: number | null
  ): ObjectPlacement | null {
    const estateDefinition = getEstateStorageDef(objectType)
    if (estateDefinition) {
      const chest =
        objectId == null ? undefined : get(estateChests).get(objectId)
      if (!chest || chest.item_def_id !== objectType) return null
      return {
        id: chest.id,
        type: estateDefinition.modelId,
        x: unwrapWorldXNear(wx, chest.position.x),
        y: chest.position.y,
        z: chest.position.z,
        rotation: chest.rotation_deg,
        floorLevel: chest.floor_level,
      }
    }
    let best: ObjectPlacement | null = null
    let bestDist = Infinity
    for (const region of this.cache.values()) {
      for (const p of region.placements) {
        if (objectId != null && p.id === objectId) return p
        if (p.type !== objectType) continue
        const dx = p.x - wx
        const dz = p.z - wz
        const dist = dx * dx + dz * dz
        if (dist < bestDist) {
          bestDist = dist
          best = p
        }
      }
    }
    return best
  }

  /** Resolve the placement's animation, offset and rotation in radians. */
  async resolvePose(
    objectType: string,
    wx: number,
    wz: number,
    objectId?: number | null
  ): Promise<{
    anim: string
    interactOffset?: Position
    placement: ObjectPlacement | null
    rotation?: number
  }> {
    const [, placement] = await Promise.all([
      this.fetchCatalog(),
      this.findNearestPlacementAsync(objectType, wx, wz, objectId),
    ])
    const def = this.getCatalogEntry(
      placement?.type ?? getEstateStorageDef(objectType)?.modelId ?? objectType
    )
    return {
      anim: def?.interaction ?? objectType,
      interactOffset: def?.interactOffset,
      placement,
      rotation: placement ? MathUtils.degToRad(placement.rotation) : undefined,
    }
  }

  async findNearestPlacementAsync(
    objectType: string,
    wx: number,
    wz: number,
    objectId?: number | null
  ): Promise<ObjectPlacement | null> {
    if (getEstateStorageDef(objectType))
      return this.findNearestPlacement(objectType, wx, wz, objectId)
    // Ensure the region containing this position is loaded
    const tileX = Math.floor(wx / TERRAIN_TILE_SIZE)
    const tileZ = Math.floor(wz / TERRAIN_TILE_SIZE)
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    await this.fetchObject(rx, rz)
    return this.findNearestPlacement(objectType, wx, wz, objectId)
  }
}

/** Shared singleton instance */
export const objectManager = new ObjectManager()
