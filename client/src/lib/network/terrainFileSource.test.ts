import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  clearTerrainManifests,
  loadTerrainFile,
  rememberTerrainFiles,
} from './terrainFileSource'
import { TerrainFileCache, type TerrainFiles } from './terrainFiles'
import {
  landscapeBytes,
  stubSha256Digest,
  terrainFile as descriptor,
} from './terrain-files.fixture'

const empty = (): TerrainFiles => ({
  height: null,
  splat: null,
  trees: null,
  grass: null,
  landscape: null,
})

beforeEach(() => {
  stubSha256Digest()
})
afterEach(() => {
  clearTerrainManifests()
  vi.unstubAllGlobals()
})

describe('terrain renderer file source', () => {
  it('shares initial manifest requests across layers and wraps tile coordinates', async () => {
    const fetchMock = vi.fn(async (_url: string) => Response.json(empty()))
    vi.stubGlobal('fetch', fetchMock)
    const [height, splat] = await Promise.all([
      loadTerrainFile('https://initial.test', 256, -1, 'height'),
      loadTerrainFile('https://initial.test', 256, -1, 'splat'),
    ])
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][0]).toBe(
      'https://initial.test/api/terrain/manifest/-256/-1'
    )
    expect(height.bytes?.length).toBe(8450)
    expect(splat.bytes?.length).toBe(16384)
  })

  it('uses websocket manifests and shares landscaping bytes between visual layers', async () => {
    const landscape = landscapeBytes()
    const files = empty()
    files.landscape = descriptor(
      'landscaping/r+00_+00/l_+0000_+0000.bin',
      landscape
    )
    rememberTerrainFiles('https://landscape.test', 0, 0, files)
    const fetchMock = vi.fn(async () => new Response(landscape))
    vi.stubGlobal('fetch', fetchMock)
    const [splat, trees] = await Promise.all([
      loadTerrainFile('https://landscape.test', 0, 0, 'splat'),
      loadTerrainFile('https://landscape.test', 0, 0, 'trees'),
    ])
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(splat.bytes?.[0]).toBe(5)
    expect(trees.bytes).toBeNull()
    expect(trees.cleared[0]).toBe(128)
  })

  it('refreshes a stale manifest when a raw file changes before its download', async () => {
    const original = new Uint8Array(8450).fill(1)
    const current = new Uint8Array(8450).fill(2)
    const files = empty()
    files.height = descriptor('height/r+00_+00/h_+0000_+0000.bin', original)
    rememberTerrainFiles('https://changed.test', 0, 0, files)
    const next = { ...files, height: descriptor(files.height.path, current) }
    const fetchMock = vi.fn(async (url: string) =>
      url.includes('/manifest/') ? Response.json(next) : new Response(current)
    )
    vi.stubGlobal('fetch', fetchMock)
    expect(
      (await loadTerrainFile('https://changed.test', 0, 0, 'height')).bytes?.[0]
    ).toBe(2)
    expect(fetchMock).toHaveBeenCalledTimes(3)
  })

  it('keeps verified memory entries unchanged when consumers edit their byte arrays', async () => {
    const bytes = new Uint8Array([128, 255])
    const file = descriptor('grass/r+00_+00/g_+0000_+0000.bin', bytes)
    const cache = new TerrainFileCache()
    const fetchMock = vi.fn(async () => new Response(bytes))
    vi.stubGlobal('fetch', fetchMock)
    const first = (await cache.file('', file))!
    first[0] = 0
    expect(await cache.file('', file)).toEqual(bytes)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })
})
