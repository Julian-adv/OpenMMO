import { wrapTileX } from '../terrain/world-wrap'
import {
  decodeLandscape,
  defaultHeightmap,
  terrainFileCache,
  HEIGHT_BYTES,
  SPLAT_BYTES,
  CLEARED_BYTES,
  type TerrainFiles,
} from './terrainFiles'

const manifests = new Map<string, TerrainFiles>()
const pending = new Map<string, Promise<TerrainFiles>>()

function manifestUrl(base: string, x: number, z: number) {
  return `${base}/api/terrain/manifest/${wrapTileX(x)}/${z}`
}

export function rememberTerrainFiles(
  base: string,
  x: number,
  z: number,
  files: TerrainFiles
) {
  manifests.set(manifestUrl(base, x, z), files)
}

export function forgetTerrainFiles(base: string, x: number, z: number) {
  manifests.delete(manifestUrl(base, x, z))
}

export function clearTerrainManifests() {
  manifests.clear()
}

async function getManifest(
  url: string,
  refresh: boolean
): Promise<TerrainFiles> {
  if (!refresh) {
    const known = manifests.get(url)
    if (known) return known
  }
  let request = pending.get(url)
  if (!request) {
    request = (async () => {
      const response = await fetch(url, {
        cache: 'no-store',
        signal: AbortSignal.timeout(15000),
      })
      if (!response.ok) throw new Error(String(response.status))
      return response.json() as Promise<TerrainFiles>
    })()
    pending.set(url, request)
  }
  try {
    return await request
  } finally {
    if (pending.get(url) === request) pending.delete(url)
  }
}

export async function loadTerrainFile(
  base: string,
  x: number,
  z: number,
  kind: 'height' | 'splat' | 'trees' | 'grass'
): Promise<{ bytes: Uint8Array<ArrayBuffer> | null; cleared: Uint8Array }> {
  for (let attempt = 0; ; attempt++) {
    try {
      const files = await getManifest(manifestUrl(base, x, z), attempt > 0)
      const [raw, landscape] = await Promise.all([
        terrainFileCache.file(
          base,
          kind === 'splat' && files.landscape ? null : files[kind]
        ),
        terrainFileCache.file(base, kind === 'height' ? null : files.landscape),
      ])
      const decoded = landscape ? decodeLandscape(landscape) : null
      const bytes =
        kind === 'height'
          ? (raw ?? defaultHeightmap())
          : kind === 'splat'
            ? (decoded?.splat ?? raw ?? new Uint8Array(SPLAT_BYTES))
            : raw
      if (
        (kind === 'height' && bytes?.length !== HEIGHT_BYTES) ||
        (kind === 'splat' && bytes?.length !== SPLAT_BYTES)
      ) {
        throw new Error('Invalid terrain file size')
      }
      return {
        bytes,
        cleared: decoded?.cleared ?? new Uint8Array(CLEARED_BYTES),
      }
    } catch (error) {
      if (
        attempt === 0 &&
        error instanceof Error &&
        ['404', '409'].includes(error.message)
      )
        continue
      throw error
    }
  }
}
