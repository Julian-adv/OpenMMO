import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  loadSplatLayers,
  DEFAULT_LAYER_CONFIGS,
  type LayerConfig,
} from '../utils/splatLayerLoader'
import type { SplatLayer } from '../components/makeSplatStandardMaterial'

const REGION_SIZE = 16

/** Matches Rust's i32.div_euclid(16) */
export function tileToRegion(tile: number): number {
  return Math.floor(tile / REGION_SIZE)
}

function regionKey(rx: number, rz: number): string {
  return `${rx},${rz}`
}

export interface RegionMeta {
  layers: [LayerConfig, LayerConfig, LayerConfig, LayerConfig]
}

export interface ResolvedRegionLayers {
  layers: [SplatLayer, SplatLayer, SplatLayer, SplatLayer]
}

export class TerrainMetaManager {
  private metaCache = new Map<string, RegionMeta>()
  private layerCache = new Map<string, ResolvedRegionLayers>()
  private inflightMeta = new Map<string, Promise<RegionMeta>>()
  private inflightLayers = new Map<string, Promise<ResolvedRegionLayers>>()
  private terrainApiUrl: string

  constructor() {
    this.terrainApiUrl = getTerrainApiUrl()
  }

  async fetchMeta(rx: number, rz: number): Promise<RegionMeta> {
    const key = regionKey(rx, rz)
    const cached = this.metaCache.get(key)
    if (cached) return cached

    const inflight = this.inflightMeta.get(key)
    if (inflight) return inflight

    const promise = (async () => {
      try {
        const resp = await fetch(
          `${this.terrainApiUrl}/api/terrain/meta/${rx}/${rz}`
        )
        const json = await resp.json()
        const meta: RegionMeta = {
          layers: json.layers as [
            LayerConfig,
            LayerConfig,
            LayerConfig,
            LayerConfig,
          ],
        }
        this.metaCache.set(key, meta)
        return meta
      } catch {
        // Fallback to defaults on error
        const meta: RegionMeta = { layers: [...DEFAULT_LAYER_CONFIGS] }
        this.metaCache.set(key, meta)
        return meta
      } finally {
        this.inflightMeta.delete(key)
      }
    })()
    this.inflightMeta.set(key, promise)
    return promise
  }

  async getLayersForTile(
    tileX: number,
    tileZ: number
  ): Promise<ResolvedRegionLayers> {
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    const key = regionKey(rx, rz)

    const cached = this.layerCache.get(key)
    if (cached) return cached

    const inflight = this.inflightLayers.get(key)
    if (inflight) return inflight

    const promise = (async () => {
      const meta = await this.fetchMeta(rx, rz)
      const layers = await loadSplatLayers(meta.layers)
      const resolved: ResolvedRegionLayers = { layers }
      this.layerCache.set(key, resolved)
      this.inflightLayers.delete(key)
      return resolved
    })()
    this.inflightLayers.set(key, promise)
    return promise
  }

  getMetaForTile(tileX: number, tileZ: number): RegionMeta | null {
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    return this.metaCache.get(regionKey(rx, rz)) ?? null
  }

  async saveMeta(rx: number, rz: number, meta: RegionMeta): Promise<void> {
    await fetch(`${this.terrainApiUrl}/api/terrain/meta/${rx}/${rz}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ layers: meta.layers }),
    })
    const key = regionKey(rx, rz)
    this.metaCache.set(key, meta)
    this.layerCache.delete(key)
  }

  invalidateRegion(rx: number, rz: number): void {
    const key = regionKey(rx, rz)
    this.metaCache.delete(key)
    this.layerCache.delete(key)
  }

  destroy(): void {
    this.metaCache.clear()
    this.layerCache.clear()
  }
}
