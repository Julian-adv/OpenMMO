import { afterEach, describe, expect, it, vi } from 'vitest'
import { TerrainSplatManager } from './terrainSplatManager'
import { loadTerrainFile } from '../network/terrainFileSource'

vi.mock('../network/terrainFileSource', () => ({
  loadTerrainFile: vi.fn(),
}))

vi.mock('../utils/networkUtils', () => ({
  getTerrainApiUrl: () => '',
  apiFetch: vi.fn(),
}))

afterEach(() => vi.resetAllMocks())

function splat(palette: number) {
  const data = new Uint8Array(64 * 64 * 4)
  for (let index = 0; index < data.length; index += 4)
    data[index] = palette << 4
  return { bytes: data, cleared: new Uint8Array(512) }
}

function pendingResponse() {
  let resolve!: (response: ReturnType<typeof splat>) => void
  const promise = new Promise<ReturnType<typeof splat>>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

describe('live landscaping updates', () => {
  it('keeps a server update that arrives while the original tile is loading', async () => {
    const pending = pendingResponse()
    vi.mocked(loadTerrainFile).mockReturnValue(pending.promise)
    const manager = new TerrainSplatManager()
    const load = manager.loadSplatmap(0, 0)
    manager.setSplatmap(0, 0, splat(5).bytes)
    pending.resolve(splat(0))
    await load
    expect(manager.getSplatData(0, 0)?.[0]).toBe(0x50)
    await manager.destroy()
  })

  it('refetches stale in-flight data after a distant edit notification', async () => {
    const pending = pendingResponse()
    vi.mocked(loadTerrainFile)
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValue(splat(5))
    const manager = new TerrainSplatManager()
    const load = manager.loadSplatmap(0, 0)
    manager.invalidateLandscaping(0, 0)
    pending.resolve(splat(0))
    await load
    expect(loadTerrainFile).toHaveBeenCalledTimes(2)
    expect(manager.getSplatData(0, 0)?.[0]).toBe(0x50)
    await manager.destroy()
  })

  it('keeps unrelated tile requests when one tile is invalidated', async () => {
    const first = pendingResponse()
    const second = pendingResponse()
    vi.mocked(loadTerrainFile)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    const manager = new TerrainSplatManager()
    const load = manager.loadSplatmap(0, 0)
    await Promise.resolve()
    manager.invalidateLandscaping(1, 0)
    const other = manager.loadSplatmap(1, 0)
    first.resolve(splat(0))
    second.resolve(splat(5))
    await Promise.all([load, other])
    expect(loadTerrainFile).toHaveBeenCalledTimes(2)
    expect(manager.getSplatData(0, 0)?.[0]).toBe(0)
    expect(manager.getSplatData(1, 0)?.[0]).toBe(0x50)
    await manager.destroy()
  })

  it('drops an old cached tile so returning players see the latest terrain', async () => {
    vi.mocked(loadTerrainFile).mockResolvedValue(splat(5))
    const manager = new TerrainSplatManager()
    manager.setSplatmap(0, 0, splat(0).bytes)
    manager.invalidateLandscaping(0, 0)
    await manager.loadSplatmap(0, 0)
    expect(manager.getSplatData(0, 0)?.[0]).toBe(0x50)
    await manager.destroy()
  })
})
