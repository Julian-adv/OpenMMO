import type { DungeonRoom } from '../managers/dungeonManager'
import type { DungeonGeoCtx } from './dungeon-geo-constants'

export function dungeonCtx(grid: number): DungeonGeoCtx {
  return { grid, wallHeight: 3, floorHeight: 4, shaftW: 2, shaftLen: 8 }
}

export function carveDungeon(ctx: DungeonGeoCtx, rects: DungeonRoom[]) {
  const carved = Array<boolean>(ctx.grid ** 2).fill(false)
  for (const r of rects)
    for (let z = r.z; z < r.z + r.d; z++)
      for (let x = r.x; x < r.x + r.w; x++) carved[x + z * ctx.grid] = true
  return carved
}
