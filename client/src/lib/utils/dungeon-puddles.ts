import type { DungeonFloorLayout } from '../managers/dungeonManager'
import type { DungeonGeoCtx } from './dungeon-geo-constants'
import { rectContains, shaftRect } from './dungeon-geo-shaft'

export interface DungeonDrip {
  x: number
  z: number
  seed: number
  period: number
  phase: number
}

export interface DungeonPuddleShape {
  lobes: number
  irregularity: number
  phase: number
  skew: number
}

export interface DungeonPuddle {
  x: number
  z: number
  width: number
  depth: number
  seed: number
  shape: DungeonPuddleShape
  drips: DungeonDrip[]
  clip?: DungeonPuddleBounds
}

export interface DungeonPuddleBounds {
  minX: number
  minZ: number
  maxX: number
  maxZ: number
}

export function dungeonPuddleBounds(p: DungeonPuddle): DungeonPuddleBounds {
  return {
    minX: Math.max(p.x - p.width / 2, p.clip?.minX ?? -Infinity),
    minZ: Math.max(p.z - p.depth / 2, p.clip?.minZ ?? -Infinity),
    maxX: Math.min(p.x + p.width / 2, p.clip?.maxX ?? Infinity),
    maxZ: Math.min(p.z + p.depth / 2, p.clip?.maxZ ?? Infinity),
  }
}

const MAX_PUDDLES = 96

export function dungeonPuddleRadius(shape: DungeonPuddleShape, angle: number) {
  return (
    0.68 +
    Math.sin(angle * shape.lobes + shape.phase) * shape.irregularity +
    Math.sin(angle * (shape.lobes + 1) - shape.phase) * 0.04 +
    Math.cos(angle - shape.phase * 0.7) * shape.skew
  )
}

export function generateDungeonPuddles(
  layout: DungeonFloorLayout,
  ctx: DungeonGeoCtx,
  dungeonId: string
): DungeonPuddle[] {
  let seed = 2166136261
  for (const char of `${dungeonId}:${layout.depth}:puddles`)
    seed = Math.imul(seed ^ char.charCodeAt(0), 16777619)
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
    return seed / 4294967296
  }
  const { grid } = ctx
  const carvedAt = (x: number, z: number) =>
    x >= 0 && z >= 0 && x < grid && z < grid && layout.carved[x + z * grid]
  const roomAt = new Int16Array(grid * grid).fill(-1)
  layout.rooms.forEach((room, index) => {
    for (let z = room.z; z < room.z + room.d; z++)
      for (let x = room.x; x < room.x + room.w; x++)
        roomAt[x + z * grid] = index
  })
  const excluded = [shaftRect(layout.upShaft, ctx)]
  if (layout.downShaft) excluded.push(shaftRect(layout.downShaft, ctx))
  for (const prop of layout.props)
    if (prop.kind !== 'torch_wall')
      excluded.push({ x: prop.x, z: prop.z, w: 1, d: 1 })
  if (layout.chest)
    excluded.push({ x: layout.chest[0], z: layout.chest[1], w: 1, d: 1 })

  const eligible = (x: number, z: number, room: number) =>
    carvedAt(x, z) &&
    roomAt[x + z * grid] === room &&
    !excluded.some((rect) => rectContains(rect, x, z))

  const puddles: DungeonPuddle[] = []
  const place = (cellX: number, cellZ: number, room: number) => {
    if (puddles.length >= MAX_PUDDLES || !eligible(cellX, cellZ, room))
      return false
    const alongX =
      room >= 0
        ? random() < 0.5
        : Number(eligible(cellX - 2, cellZ, room)) +
            Number(eligible(cellX + 2, cellZ, room)) >
          Number(eligible(cellX, cellZ - 2, room)) +
            Number(eligible(cellX, cellZ + 2, room))
    const alongCell = alongX ? cellX : cellZ
    const acrossCell = alongX ? cellZ : cellX
    const at = (along: number, across: number) =>
      alongX ? eligible(along, across, room) : eligible(across, along, room)
    let acrossMin = acrossCell
    let acrossMax = acrossCell + 1
    while (at(alongCell, acrossMin - 1)) acrossMin--
    while (at(alongCell, acrossMax)) acrossMax++
    const wallAt = (along: number, across: number) =>
      !carvedAt(alongX ? along : across, alongX ? across : along)
    const lowWall = wallAt(alongCell, acrossMin - 1)
    const highWall = wallAt(alongCell, acrossMax)
    const clipLow = acrossMin + (lowWall ? 0 : 0.08)
    const clipHigh = acrossMax - (highWall ? 0 : 0.08)

    const sizeRoll = random()
    const scale =
      sizeRoll < 0.12 ? 3 : sizeRoll < 0.35 ? 2 : 0.65 + random() * 0.7
    const acrossSize = Math.min(
      room < 0
        ? 0.65 + random() * 1.05
        : (1.2 + random() * 1.2) * Math.sqrt(scale),
      acrossMax - acrossMin - 0.16
    )
    const acrossLow = acrossMin + acrossSize / 2 + 0.08
    const acrossHigh = acrossMax - acrossSize / 2 - 0.08
    const edgeBias = random() < 0.65 && (lowWall || highWall)
    const lowEdge = lowWall && (!highWall || random() < 0.5)
    const inset = 0.06 + random() * 0.12
    const across = edgeBias
      ? lowEdge
        ? acrossMin + inset
        : acrossMax - inset
      : acrossLow + (acrossHigh - acrossLow) * random()
    const stripAt = (along: number) => {
      if (across - acrossSize / 2 < clipLow && !wallAt(along, acrossMin - 1))
        return false
      if (across + acrossSize / 2 > clipHigh && !wallAt(along, acrossMax))
        return false
      for (
        let cell = Math.floor(Math.max(clipLow, across - acrossSize / 2));
        cell < Math.ceil(Math.min(clipHigh, across + acrossSize / 2));
        cell++
      )
        if (!at(along, cell)) return false
      return true
    }
    if (!stripAt(alongCell)) return false
    let alongMin = alongCell
    let alongMax = alongCell + 1
    while (stripAt(alongMin - 1)) alongMin--
    while (stripAt(alongMax)) alongMax++
    const alongSize = Math.min(
      room < 0
        ? (1.8 + random() * 1.4) * scale
        : (1.2 + random() * 1.2) * Math.sqrt(scale),
      alongMax - alongMin - 0.16
    )
    const along = Math.max(
      alongMin + alongSize / 2 + 0.08,
      Math.min(
        alongCell + 0.1 + random() * 0.8,
        alongMax - alongSize / 2 - 0.08
      )
    )
    const x = alongX ? along : across
    const z = alongX ? across : along
    const width = alongX ? alongSize : acrossSize
    const depth = alongX ? acrossSize : alongSize
    const clip = {
      minX: alongX ? alongMin + 0.08 : clipLow,
      minZ: alongX ? clipLow : alongMin + 0.08,
      maxX: alongX ? alongMax - 0.08 : clipHigh,
      maxZ: alongX ? clipHigh : alongMax - 0.08,
    }
    if (
      puddles.some(
        (p) =>
          Math.abs(p.x - x) < (p.width + width) / 2 + 0.8 &&
          Math.abs(p.z - z) < (p.depth + depth) / 2 + 0.8
      )
    )
      return false
    const shape: DungeonPuddleShape = {
      lobes: 2 + Math.floor(random() * 4),
      irregularity: 0.06 + random() * 0.12,
      phase: random() * Math.PI * 2,
      skew: 0.02 + random() * 0.05,
    }
    const dripRoll = random()
    const dripCount = dripRoll < 0.3 ? 0 : dripRoll < 0.8 ? 1 : 2
    const drips: DungeonDrip[] = []
    for (let attempt = 0; attempt < 24 && drips.length < dripCount; attempt++) {
      const angle = random() * Math.PI * 2
      const radius =
        (dungeonPuddleRadius(shape, angle) - 0.12) * (0.35 + random() * 0.4)
      const dx = x + (Math.cos(angle) * radius * width) / 2
      const dz = z + (Math.sin(angle) * radius * depth) / 2
      if (
        dx < clip.minX + 0.12 ||
        dx > clip.maxX - 0.12 ||
        dz < clip.minZ + 0.12 ||
        dz > clip.maxZ - 0.12 ||
        drips.some((drip) => Math.hypot(drip.x - dx, drip.z - dz) <= 0.12)
      )
        continue
      const period = 1.8 + random() * 2.8
      drips.push({
        x: dx,
        z: dz,
        seed: random() * 100,
        period,
        phase: random() * period,
      })
    }
    puddles.push({
      x,
      z,
      width,
      depth,
      seed: random() * 100,
      shape,
      drips,
      clip,
    })
    return true
  }

  const candidates: [number, number][] = []
  for (let z = 0; z < grid; z++)
    for (let x = 0; x < grid; x++)
      if (eligible(x, z, -1) && random() < 0.065) candidates.push([x, z])
  for (let i = candidates.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1))
    ;[candidates[i], candidates[j]] = [candidates[j], candidates[i]]
  }
  for (const [x, z] of candidates) place(x, z, -1)

  layout.rooms.forEach((room, index) => {
    if (random() >= 0.15) return
    for (let attempt = 0; attempt < 12; attempt++) {
      const x = room.x + 1 + Math.floor(random() * Math.max(1, room.w - 2))
      const z = room.z + 1 + Math.floor(random() * Math.max(1, room.d - 2))
      if (place(x, z, index)) break
    }
  })
  return puddles
}
