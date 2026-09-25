import type { DungeonFloorLayout } from '../managers/dungeonManager'
import type { DungeonGeoCtx } from './dungeon-geo-constants'
import { dungeonCaveSeed } from './dungeon-cave-themes'
import { dungeonFloorClearance } from './dungeon-floor-clearance'

export interface DungeonFloorStone {
  x: number
  z: number
  width: number
  depth: number
  height: number
  rotation: number
}

export function generateDungeonFloorStones(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx,
  dungeonId: string
): DungeonFloorStone[] {
  let seed = dungeonCaveSeed(`${dungeonId}:floor-stones`, layout.depth)
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
    return seed / 4294967296
  }
  const { grid } = ctx
  const carved = (x: number, z: number) =>
    x >= 0 && z >= 0 && x < grid && z < grid && layout.carved[x + z * grid]
  const clear = dungeonFloorClearance(layout, ctx)
  const stones: DungeonFloorStone[] = []
  for (let z = 0; z < grid; z++) {
    for (let x = 0; x < grid; x++) {
      if (!clear(x + 0.5, z + 0.5)) continue
      const west = !carved(x - 1, z)
      const east = !carved(x + 1, z)
      const north = !carved(x, z - 1)
      const south = !carved(x, z + 1)
      const edge = west || east || north || south
      if (random() > (edge ? 0.3 : 0.045)) continue
      const count = edge ? 2 + Math.floor(random() * 2) : 1
      for (let i = 0; i < count; i++) {
        const plate = i === 0
        const width = plate ? 0.32 + random() * 0.34 : 0.09 + random() * 0.17
        const depth = width * (0.65 + random() * 0.3)
        const radius = width / 2
        const inset = radius + 0.1 + random() * 0.06
        const px =
          x + (west ? inset : east ? 1 - inset : 0.18 + random() * 0.64)
        const pz =
          z + (north ? inset : south ? 1 - inset : 0.18 + random() * 0.64)
        if (
          ![-radius, radius].every((dx) =>
            [-radius, radius].every((dz) => clear(px + dx, pz + dz))
          )
        )
          continue
        stones.push({
          x: px,
          z: pz,
          width,
          depth,
          height: plate ? 0.018 + random() * 0.023 : 0.035 + random() * 0.065,
          rotation: random() * Math.PI * 2,
        })
        if (stones.length >= 240) return stones
      }
    }
  }
  return stones
}
