import { writable } from 'svelte/store'
import { getTerrainApiUrl } from '../utils/networkUtils'
import { plotAddress, REGION_PLOTS } from '../terrain/landPlots'
import { regionKey } from '../terrain/terrain-constants'
import { wrapRegionX } from '../terrain/world-wrap'

/** Mirrors `Climate` in shared/src/worldgen/climate.rs. */
export const Climate = {
  Sea: 0,
  WetCoast: 1,
  Temperate: 2,
  RainShadow: 3,
  Alpine: 4,
} as const

const FAILED_RETRY_MS = 5000

const cache = new Map<string, Uint8Array<ArrayBuffer>>()
const inflight = new Set<string>()
const failedAt = new Map<string, number>()

/** Bumped when a region's climate grid arrives so renders re-run. */
export const climateVersion = writable(0)

function key(rx: number, rz: number) {
  return regionKey(wrapRegionX(rx), rz)
}

export function getCachedClimate(rx: number, rz: number) {
  return cache.get(key(rx, rz)) ?? null
}

/** Zone byte at a world position, or null until its region has loaded. */
export function climateAt(x: number, z: number): number | null {
  const addr = plotAddress(x, z)
  const grid = getCachedClimate(addr.rx, addr.rz)
  if (!grid) {
    requestClimate(addr.rx, addr.rz)
    return null
  }
  return grid[addr.index]
}

/** Baked per bake, so a 404 is remembered like a failure and retried slowly. */
export function requestClimate(rx: number, rz: number): void {
  const k = key(rx, rz)
  if (cache.has(k) || inflight.has(k)) return
  const failed = failedAt.get(k)
  if (failed !== undefined && Date.now() - failed < FAILED_RETRY_MS) return
  inflight.add(k)
  fetch(`${getTerrainApiUrl()}/api/terrain/climate/${wrapRegionX(rx)}/${rz}`)
    .then(async (resp) => {
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
      const bytes = new Uint8Array(await resp.arrayBuffer())
      if (bytes.length !== REGION_PLOTS) throw new Error('bad length')
      cache.set(k, bytes)
      failedAt.delete(k)
      climateVersion.update((v) => v + 1)
    })
    .catch(() => failedAt.set(k, Date.now()))
    .finally(() => inflight.delete(k))
}

export function clearClimateCache() {
  cache.clear()
  inflight.clear()
  failedAt.clear()
}
