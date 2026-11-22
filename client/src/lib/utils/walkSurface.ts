import { bridgeManager } from '../managers/bridgeManager'
import { housingManager } from '../managers/housingManager'
import { HOUSING_TEXTURES, type WalkSurface } from './housing-textures'

/** The sounding surface under feet at a world point, null on silent ground. */
export function walkSurfaceAt(
  x: number,
  y: number,
  z: number
): WalkSurface | null {
  const deck = bridgeManager.deckSurfaceAt(x, z, y)
  if (deck) return deck
  const room = housingManager.findRoomUnderFeet(x, y, z)
  if (!room) return null
  return (
    HOUSING_TEXTURES[room.floorTexture % HOUSING_TEXTURES.length].surface ??
    null
  )
}
