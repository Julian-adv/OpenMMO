import type { TerrainHeightManager } from '../managers/terrainHeightManager'
import { wrapWorldX } from '../terrain/world-wrap'

interface FloatingSurfaceInput {
  x: number
  z: number
  fallbackY: number
  heightManager: Pick<TerrainHeightManager, 'groundYOrNull'> | null
  waterSurfaceAt?: ((x: number, z: number) => number) | null
  hasWaterSurfaceData?: ((x: number, z: number) => boolean) | null
}

export function floatingSurfaceY({
  x,
  z,
  fallbackY,
  heightManager,
  waterSurfaceAt,
  hasWaterSurfaceData,
}: FloatingSurfaceInput): number | null {
  x = wrapWorldX(x)
  const bed = heightManager?.groundYOrNull(x, z)
  if (bed == null || !waterSurfaceAt || hasWaterSurfaceData?.(x, z) === false)
    return fallbackY

  const surface = waterSurfaceAt(x, z)
  return surface > bed ? surface : null
}
