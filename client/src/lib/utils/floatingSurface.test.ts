import { describe, expect, it, vi } from 'vitest'
import { floatingSurfaceY } from './floatingSurface'
import { WORLD_MAX_X, WORLD_MIN_X } from '../terrain/world-wrap'

function surface() {
  return {
    x: 0,
    z: 0,
    fallbackY: 8,
    heightManager: {
      groundYOrNull: vi.fn((): number | null => 5),
    },
    waterSurfaceAt: vi.fn(() => 9),
    hasWaterSurfaceData: vi.fn(() => true),
  }
}

describe('floating surface sampling', () => {
  it('preserves the last height until the water tile arrives', () => {
    const input = surface()
    input.hasWaterSurfaceData.mockReturnValue(false)
    expect(floatingSurfaceY(input)).toBe(8)
    expect(input.waterSurfaceAt).not.toHaveBeenCalled()
    input.hasWaterSurfaceData.mockReturnValue(true)
    expect(floatingSurfaceY(input)).toBe(9)
  })

  it('does not treat unloaded terrain as water', () => {
    const input = surface()
    input.heightManager.groundYOrNull.mockReturnValue(null)
    expect(floatingSurfaceY(input)).toBe(8)
    expect(input.waterSurfaceAt).not.toHaveBeenCalled()
  })

  it('lets known dry land use normal grounding', () => {
    const input = surface()
    input.heightManager.groundYOrNull.mockReturnValue(10)
    expect(floatingSurfaceY(input)).toBeNull()
  })

  it('samples the same tile on either side of the world seam', () => {
    const input = surface()
    input.x = WORLD_MAX_X + 2
    expect(floatingSurfaceY(input)).toBe(9)
    expect(input.waterSurfaceAt).toHaveBeenCalledWith(WORLD_MIN_X + 2, 0)
    expect(input.hasWaterSurfaceData).toHaveBeenCalledWith(WORLD_MIN_X + 2, 0)
  })
})
