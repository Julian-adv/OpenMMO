import { TILE_DIM, VERTS_PER_SIDE } from '../terrain/terrain-constants'

export type TerrainFile = { path: string; hash: string }
export type TerrainFiles = {
  height: TerrainFile | null
  splat: TerrainFile | null
  trees: TerrainFile | null
  grass: TerrainFile | null
  landscape: TerrainFile | null
}

export const HEIGHT_BYTES = VERTS_PER_SIDE * VERTS_PER_SIDE * 2
export const SPLAT_BYTES = TILE_DIM * TILE_DIM * 4
export const CLEARED_BYTES = (TILE_DIM * TILE_DIM) / 8

const MAX_MEMORY_BYTES = 8 * 1024 * 1024
const FILE_PATH =
  /^(height|splat|trees|grass|landscaping)\/r[+-]\d+_[+-]\d+\/[hstgl]_[+-]\d+_[+-]\d+\.bin$/

export class TerrainFileCache {
  private cache = new Map<string, Uint8Array>()
  private inflight = new Map<string, Promise<Uint8Array>>()
  private cacheSize = 0
  async file(
    base: string,
    file: TerrainFile | null
  ): Promise<Uint8Array<ArrayBuffer> | null> {
    if (!file) return null
    if (!/^[0-9a-f]{64}$/.test(file.hash) || !FILE_PATH.test(file.path)) {
      throw new Error('Invalid terrain file manifest')
    }
    const key = `${base}/${file.hash}`
    const cached = this.cache.get(key)
    if (cached) {
      this.cache.delete(key)
      this.cache.set(key, cached)
      return cached.slice()
    }
    let request = this.inflight.get(key)
    if (!request) {
      request = (async () => {
        const response = await fetch(
          `${base}/api/terrain/files/${file.path}?hash=${file.hash}`,
          {
            cache: 'no-store',
            signal: AbortSignal.timeout(15000),
          }
        )
        if (!response.ok) throw new Error(String(response.status))
        const buffer = await response.arrayBuffer()
        const digest = await crypto.subtle.digest('SHA-256', buffer)
        const hash = Array.from(new Uint8Array(digest), (byte) =>
          byte.toString(16).padStart(2, '0')
        ).join('')
        if (hash !== file.hash) throw new Error('409')
        const bytes = new Uint8Array(buffer)
        if (bytes.byteLength <= MAX_MEMORY_BYTES) {
          this.cache.set(key, bytes)
          this.cacheSize += bytes.byteLength
          while (this.cacheSize > MAX_MEMORY_BYTES) {
            const oldest = this.cache.entries().next().value!
            this.cache.delete(oldest[0])
            this.cacheSize -= oldest[1].byteLength
          }
        }
        return bytes
      })()
      this.inflight.set(key, request)
    }
    try {
      return (await request).slice()
    } finally {
      if (this.inflight.get(key) === request) this.inflight.delete(key)
    }
  }
}

export const terrainFileCache = new TerrainFileCache()

export function defaultHeightmap(): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(HEIGHT_BYTES)
  for (let i = 0; i < bytes.length; i += 2) {
    bytes[i] = 0x10
    bytes[i + 1] = 0x27
  }
  return bytes
}

export function decodeLandscape(bytes: Uint8Array) {
  if (
    bytes.length !== 4 + SPLAT_BYTES + CLEARED_BYTES ||
    new DataView(bytes.buffer, bytes.byteOffset).getUint32(0, true) !==
      0x31444e4c
  ) {
    throw new Error('Invalid landscaping file')
  }
  return {
    splat: bytes.slice(4, 4 + SPLAT_BYTES),
    cleared: bytes.slice(4 + SPLAT_BYTES),
  }
}
