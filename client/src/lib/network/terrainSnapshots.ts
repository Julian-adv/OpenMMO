import {
  rememberTerrainFiles,
  forgetTerrainFiles,
  clearTerrainManifests,
} from './terrainFileSource'
import {
  TerrainFileCache,
  terrainFileCache,
  defaultHeightmap,
  decodeLandscape,
  HEIGHT_BYTES,
  SPLAT_BYTES,
  CLEARED_BYTES,
  type TerrainFiles,
} from './terrainFiles'
export type { TerrainFile, TerrainFiles } from './terrainFiles'
export type TerrainVersion = {
  tile_x: number
  tile_z: number
  files: TerrainFiles
}
export type TerrainSnapshot = {
  tile_x: number
  tile_z: number
  height: Uint8Array
  splat: Uint8Array
  trees: Uint8Array | null
  grass: Uint8Array | null
  cleared: Uint8Array
}

export class TerrainSnapshots {
  private active = new Map<string, TerrainVersion>()
  private timers = new Set<ReturnType<typeof setTimeout>>()

  constructor(
    private baseUrl: () => string,
    private apply: (tile: TerrainSnapshot) => void,
    private resync: () => void,
    private fileCache: TerrainFileCache = terrainFileCache
  ) {}

  set(version: TerrainVersion) {
    const key = `${version.tile_x},${version.tile_z}`
    this.active.set(key, version)
    rememberTerrainFiles(
      this.baseUrl(),
      version.tile_x,
      version.tile_z,
      version.files
    )
    void this.load(key, version)
  }

  remove(key: string) {
    const version = this.active.get(key)
    if (version)
      forgetTerrainFiles(this.baseUrl(), version.tile_x, version.tile_z)
    this.active.delete(key)
  }

  reset() {
    this.active.clear()
    clearTerrainManifests()
    for (const timer of this.timers) clearTimeout(timer)
    this.timers.clear()
  }

  private async load(key: string, version: TerrainVersion) {
    try {
      const files = version.files
      const [heightFile, splatFile, trees, grass, landscape] =
        await Promise.all([
          this.fileCache.file(this.baseUrl(), files.height),
          this.fileCache.file(
            this.baseUrl(),
            files.landscape ? null : files.splat
          ),
          this.fileCache.file(this.baseUrl(), files.trees),
          this.fileCache.file(this.baseUrl(), files.grass),
          this.fileCache.file(this.baseUrl(), files.landscape),
        ])
      const height = heightFile ?? defaultHeightmap()
      const decoded = landscape ? decodeLandscape(landscape) : null
      const splat = decoded?.splat ?? splatFile ?? new Uint8Array(SPLAT_BYTES)
      const cleared = decoded?.cleared ?? new Uint8Array(CLEARED_BYTES)
      if (height.length !== HEIGHT_BYTES || splat.length !== SPLAT_BYTES) {
        throw new Error('Invalid terrain file size')
      }
      if (this.active.get(key) === version) {
        this.apply({
          tile_x: version.tile_x,
          tile_z: version.tile_z,
          height,
          splat,
          trees,
          grass,
          cleared,
        })
      }
    } catch (error) {
      if (this.active.get(key) !== version) return
      if (error instanceof Error && ['404', '409'].includes(error.message)) {
        this.resync()
        return
      }
      const timer = setTimeout(() => {
        this.timers.delete(timer)
        if (this.active.get(key) === version) void this.load(key, version)
      }, 1000)
      this.timers.add(timer)
    }
  }
}
