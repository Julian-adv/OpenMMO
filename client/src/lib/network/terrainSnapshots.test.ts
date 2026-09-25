import { TerrainFileCache } from './terrainFiles'
import { createHash } from 'node:crypto'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  TerrainSnapshots,
  type TerrainFile,
  type TerrainSnapshot,
  type TerrainVersion,
} from './terrainSnapshots'

const heightData = (value: number) => new Uint8Array(65 * 65 * 2).fill(value)
const file = (path: string, data: Uint8Array): TerrainFile => ({
  path,
  hash: createHash('sha256').update(data).digest('hex'),
})
const version = (value: number): TerrainVersion => ({
  tile_x: 1,
  tile_z: 2,
  files: {
    height: file('height/r+00_+00/h_+0001_+0002.bin', heightData(value)),
    splat: null,
    trees: null,
    grass: null,
    landscape: null,
  },
})

describe('terrain file downloads', () => {
  let replies: Array<(response: Response) => void>
  let fetchMock: ReturnType<typeof vi.fn>
  let apply: ReturnType<typeof vi.fn<(tile: TerrainSnapshot) => void>>
  let resync: ReturnType<typeof vi.fn<() => void>>
  let tiles: TerrainSnapshots
  const reply = (value: number) => new Response(heightData(value))
  const settle = () => vi.advanceTimersByTimeAsync(0)

  beforeEach(() => {
    vi.useFakeTimers()
    replies = []
    fetchMock = vi.fn(
      () => new Promise<Response>((resolve) => replies.push(resolve))
    )
    vi.stubGlobal('fetch', fetchMock)
    vi.stubGlobal('crypto', {
      subtle: {
        digest: async (_algorithm: string, bytes: ArrayBuffer) => {
          const digest = createHash('sha256')
            .update(new Uint8Array(bytes))
            .digest()
          return digest.buffer.slice(
            digest.byteOffset,
            digest.byteOffset + digest.byteLength
          )
        },
      },
    })
    apply = vi.fn()
    resync = vi.fn()
    tiles = new TerrainSnapshots(
      () => '',
      apply,
      resync,
      new TerrainFileCache()
    )
  })

  afterEach(() => {
    tiles.reset()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it('applies only the latest requested version when responses arrive out of order', async () => {
    tiles.set(version(1))
    tiles.set(version(2))
    replies[1](reply(2))
    await settle()
    replies[0](reply(1))
    await settle()
    expect(apply.mock.calls.map(([tile]) => tile.height[0])).toEqual([2])
  })

  it('ignores a response after leaving, then reuses it on reentry', async () => {
    tiles.set(version(1))
    tiles.remove('1,2')
    replies[0](reply(1))
    await settle()
    expect(apply).not.toHaveBeenCalled()
    tiles.set(version(1))
    await settle()
    expect(apply).toHaveBeenCalledOnce()
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('deduplicates downloads across resets without applying the old subscription', async () => {
    tiles.set(version(1))
    tiles.reset()
    tiles.set(version(1))
    replies[0](reply(1))
    await settle()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(apply).toHaveBeenCalledOnce()
  })

  it('reuses verified content after a reset and across paths with the same hash', async () => {
    tiles.set(version(1))
    tiles.reset()
    replies[0](reply(1))
    await settle()
    expect(apply).not.toHaveBeenCalled()
    const other = version(1)
    other.tile_x = 2
    other.files.height!.path = 'height/r+00_+00/h_+0002_+0002.bin'
    tiles.set(other)
    await settle()
    expect(apply.mock.calls[0][0].tile_x).toBe(2)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it.each([404, 409])('resyncs when the origin returns %i', async (status) => {
    tiles.set(version(1))
    replies[0](new Response(null, { status }))
    await settle()
    expect(resync).toHaveBeenCalledOnce()
    expect(apply).not.toHaveBeenCalled()
  })

  it('rejects changed bytes without caching them and bypasses the HTTP cache', async () => {
    tiles.set(version(1))
    replies[0](reply(2))
    await settle()
    expect(resync).toHaveBeenCalledOnce()
    expect(apply).not.toHaveBeenCalled()
    expect(fetchMock.mock.calls[0][1].cache).toBe('no-store')
    tiles.set(version(1))
    replies[1](reply(1))
    await settle()
    expect(apply).toHaveBeenCalledOnce()
  })

  it('retries temporary failures and cancels retries on reset', async () => {
    tiles.set(version(1))
    replies[0](new Response(null, { status: 503 }))
    await settle()
    await vi.advanceTimersByTimeAsync(1000)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    replies[1](new Response(null, { status: 503 }))
    await settle()
    tiles.reset()
    await vi.advanceTimersByTimeAsync(1000)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(apply).not.toHaveBeenCalled()
  })

  it('creates defaults for absent files without network requests', async () => {
    const empty = version(1)
    empty.files.height = null
    tiles.set(empty)
    await settle()
    const tile = apply.mock.calls[0][0]
    expect(new DataView(tile.height.buffer).getUint16(0, true)).toBe(10000)
    expect(tile.splat).toEqual(new Uint8Array(16384))
    expect(tile.cleared).toEqual(new Uint8Array(512))
    expect(tile.grass).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('uses the raw landscaping splat and mask, fetching only changed files', async () => {
    const landscape = new Uint8Array(4 + 16384 + 512)
    landscape.set([76, 78, 68, 49])
    landscape[4] = 5
    landscape[4 + 16384] = 128
    const first = version(1)
    first.files.landscape = file(
      'landscaping/r+00_+00/l_+0001_+0002.bin',
      landscape
    )
    first.files.splat = file(
      'splat/r+00_+00/s_+0001_+0002.bin',
      new Uint8Array(16384)
    )
    tiles.set(first)
    expect(fetchMock).toHaveBeenCalledTimes(2)
    replies[0](reply(1))
    await settle()
    expect(apply).not.toHaveBeenCalled()
    replies[1](new Response(landscape))
    await settle()
    expect(apply.mock.calls[0][0].splat[0]).toBe(5)
    expect(apply.mock.calls[0][0].cleared[0]).toBe(128)
    landscape[4] = 6
    tiles.set({
      ...first,
      files: {
        ...first.files,
        landscape: file(first.files.landscape.path, landscape),
      },
    })
    expect(fetchMock).toHaveBeenCalledTimes(3)
    replies[2](new Response(landscape))
    await settle()
    expect(apply.mock.calls[1][0].splat[0]).toBe(6)
  })
})
