import type { DungeonFloorLayout } from '../managers/dungeonManager'
import type { DungeonGeoCtx } from './dungeon-geo-constants'
import { rectContains, shaftRect } from './dungeon-geo-shaft'

export function dungeonFloorClearance(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx
) {
  const excluded = [shaftRect(layout.upShaft, ctx), ...layout.rooms]
  if (layout.downShaft) excluded.push(shaftRect(layout.downShaft, ctx))
  for (const prop of layout.props) {
    if (prop.kind !== 'torch_wall')
      excluded.push({ x: prop.x, z: prop.z, w: 1, d: 1 })
  }
  if (layout.chest)
    excluded.push({ x: layout.chest[0], z: layout.chest[1], w: 1, d: 1 })
  const padded = excluded.map((r) => ({
    x: r.x - 0.5,
    z: r.z - 0.5,
    w: r.w + 1,
    d: r.d + 1,
  }))
  return (x: number, z: number) =>
    x >= 0 &&
    z >= 0 &&
    x < ctx.grid &&
    z < ctx.grid &&
    layout.carved[Math.floor(x) + Math.floor(z) * ctx.grid] &&
    !padded.some((r) => rectContains(r, x, z))
}
