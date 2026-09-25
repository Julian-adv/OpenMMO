import { describe, expect, it } from 'vitest'
import {
  decodeGrassData,
  encodeGrassBuffer,
  filterGrassData,
  getInstanceData,
  removeGrassInRect,
} from './grass-data'
import { clearedCellAt } from '../terrain/landscaping'

const size = 4 + 64 * 64 * 3
const heights = new Uint16Array(65 * 65).fill(10200)

function density(cells: [number, number, number, number, number][]) {
  const bytes = new Uint8Array(size)
  new DataView(bytes.buffer).setUint32(0, 0x47523034, true)
  for (const [x, z, short, tall, flowers] of cells)
    bytes.set([short, tall, flowers], 4 + (z * 64 + x) * 3)
  return bytes.buffer
}

describe('grass cell densities', () => {
  it('expands each type inside its cell and round-trips the counts', () => {
    const bytes = density([
      [0, 0, 64, 36, 2],
      [63, 63, 255, 8, 1],
    ])
    const data = decodeGrassData(bytes, -12, 7, heights)
    expect([data.shortCount, data.tallCount, data.flowerCount]).toEqual([
      319, 44, 3,
    ])
    for (const type of ['short', 'tall', 'flower'] as const) {
      const raw = getInstanceData(data, type)
      for (let i = 0; i < raw.length; i += 5) {
        const x = Math.floor(raw[i] - (-12 * 64 - 32))
        const z = Math.floor(raw[i + 2] - (7 * 64 - 32))
        expect(x === 0 || x === 63).toBe(true)
        expect(z).toBe(x)
        expect(raw[i + 1]).toBeCloseTo(10)
      }
    }
    expect(encodeGrassBuffer(data, -12, 7)).toEqual(bytes)
    expect(decodeGrassData(bytes, -12, 7, heights).buffer).toEqual(data.buffer)
  })

  it('keeps unchanged cells stable when a neighboring cell changes', () => {
    const first = decodeGrassData(density([[32, 32, 8, 0, 0]]), 0, 0, heights)
    const next = decodeGrassData(
      density([
        [32, 32, 8, 0, 0],
        [33, 32, 12, 0, 0],
      ]),
      0,
      0,
      heights
    )
    expect(getInstanceData(next, 'short').slice(0, 8 * 5)).toEqual(
      getInstanceData(first, 'short')
    )
  })

  it('skips cleared cells while preserving source bytes and surviving placements', () => {
    const bytes = density([
      [0, 0, 255, 255, 255],
      [32, 32, 8, 4, 1],
      [63, 63, 255, 255, 255],
    ])
    const original = bytes.slice(0)
    const mask = new Uint8Array(512)
    mask[0] = 1
    mask[511] = 128
    const all = decodeGrassData(bytes, -256, 7, heights)
    const filtered = filterGrassData(all, (x, z) =>
      clearedCellAt(mask, -256, 7, x, z)
    )!
    expect(decodeGrassData(bytes, -256, 7, heights, mask)).toEqual(filtered)
    expect(bytes).toEqual(original)
    mask.fill(255)
    expect(
      decodeGrassData(bytes, -256, 7, heights, mask).buffer.byteLength
    ).toBe(12)
  })

  it('uses the same pattern across the world seam', () => {
    const bytes = density([[32, 32, 8, 2, 1]])
    const first = getInstanceData(
      decodeGrassData(bytes, -256, 0, heights),
      'short'
    )
    const wrapped = getInstanceData(
      decodeGrassData(bytes, 256, 0, heights),
      'short'
    )
    for (let i = 0; i < first.length; i += 5) {
      expect(wrapped[i] - first[i]).toBeCloseTo(512 * 64, 2)
      expect(wrapped.slice(i + 1, i + 5)).toEqual(first.slice(i + 1, i + 5))
    }
  })

  it('keeps a partial-cell house cut empty after saving and reloading', () => {
    const data = decodeGrassData(
      density([
        [32, 32, 64, 36, 2],
        [33, 32, 8, 4, 1],
      ]),
      0,
      0,
      heights
    )
    const cut = removeGrassInRect(data, 0.1, 0.2, 0.3, 0.4)!
    const bytes = new Uint8Array(encodeGrassBuffer(cut, 0, 0))
    const offset = 4 + (32 * 64 + 32) * 3
    expect([...bytes.slice(offset, offset + 6)]).toEqual([0, 0, 0, 8, 4, 1])
    const reloaded = decodeGrassData(bytes.buffer, 0, 0, heights)
    expect(removeGrassInRect(reloaded, 0.1, 0.2, 0.3, 0.4)).toBeNull()
  })

  it('reads legacy placements as counts, including saturated cells and edges', () => {
    const bytes = new Uint8Array(16 + 302 * 6)
    const view = new DataView(bytes.buffer)
    view.setUint32(0, 0x47523033, true)
    view.setUint32(4, 300, true)
    view.setUint32(8, 1, true)
    view.setUint32(12, 1, true)
    view.setUint16(bytes.length - 6, 65535, true)
    view.setUint16(bytes.length - 4, 65535, true)
    const data = decodeGrassData(bytes.buffer, 0, 0, heights)
    expect(encodeGrassBuffer(data, 0, 0)).toEqual(
      density([
        [0, 0, 255, 1, 0],
        [63, 63, 0, 0, 1],
      ])
    )
    const mask = new Uint8Array(512)
    mask[0] = 1
    expect(
      encodeGrassBuffer(
        decodeGrassData(bytes.buffer, 0, 0, heights, mask),
        0,
        0
      )
    ).toEqual(density([[63, 63, 0, 0, 1]]))
  })

  it('skips underwater placements and accepts empty tiles', () => {
    const bytes = density([[0, 0, 64, 36, 1]])
    const data = decodeGrassData(
      bytes,
      0,
      0,
      new Uint16Array(65 * 65).fill(10000)
    )
    expect(data.shortCount + data.tallCount + data.flowerCount).toBe(0)
    expect(encodeGrassBuffer(data, 0, 0)).toEqual(density([]))
  })

  it('rejects truncated and inconsistent payloads', () => {
    for (const bytes of [
      new ArrayBuffer(0),
      new ArrayBuffer(16),
      density([]).slice(0, -1),
    ])
      expect(() => decodeGrassData(bytes, 0, 0, heights)).toThrow()
    const legacy = new ArrayBuffer(16)
    const view = new DataView(legacy)
    view.setUint32(0, 0x47523033, true)
    view.setUint32(4, 1, true)
    expect(() => decodeGrassData(legacy, 0, 0, heights)).toThrow()
  })
})
