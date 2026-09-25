import {
  plotAddress,
  plotOrigin,
  type OwnedLandPlot,
  type PlotAddr,
} from '../terrain/landPlots'
import { LAND_PLOT_SIZE } from '../terrain/terrain-constants'

export const LAND_OWNER_COLORS = [
  'rgba(84, 148, 240, 0.5)',
  'rgba(232, 104, 112, 0.5)',
  'rgba(168, 112, 224, 0.5)',
  'rgba(64, 196, 208, 0.5)',
] as const
export const OWN_LAND_COLOR = 'rgba(64, 196, 96, 0.5)'

function plotKey(plot: PlotAddr): string {
  return `${plot.rx},${plot.rz},${plot.index}`
}

function preferredColor(owner: string): number {
  let hash = 0
  for (let i = 0; i < owner.length; i++) {
    hash = (Math.imul(hash, 31) + owner.charCodeAt(i)) | 0
  }
  return (hash >>> 0) % LAND_OWNER_COLORS.length
}

interface OwnerNode {
  name: string
  neighbors: Set<OwnerNode>
  neighborColors: Set<number>
}

export function buildLandOwnerColors(
  plots: readonly OwnedLandPlot[]
): Map<string, string> {
  const owners = new Map<string, OwnerNode>()
  const names = [...new Set(plots.map((plot) => plot.ownerName))].sort()
  for (const name of names) {
    owners.set(name, {
      name,
      neighbors: new Set(),
      neighborColors: new Set(),
    })
  }
  const plotsByAddress = new Map(
    plots.map((plot) => [plotKey(plot), owners.get(plot.ownerName)!])
  )
  for (const plot of plots) {
    const owner = owners.get(plot.ownerName)!
    const { x, z } = plotOrigin(plot.rx, plot.rz, plot.index)
    for (const [dx, dz] of [
      [LAND_PLOT_SIZE, 0],
      [0, LAND_PLOT_SIZE],
    ]) {
      const neighbor = plotsByAddress.get(plotKey(plotAddress(x + dx, z + dz)))
      if (!neighbor || neighbor === owner) continue
      owner.neighbors.add(neighbor)
      neighbor.neighbors.add(owner)
    }
  }

  const remaining = new Set(owners.values())
  const colors = new Map<string, string>()
  while (remaining.size) {
    let next: OwnerNode | undefined
    for (const owner of remaining) {
      if (
        !next ||
        owner.neighborColors.size > next.neighborColors.size ||
        (owner.neighborColors.size === next.neighborColors.size &&
          owner.neighbors.size > next.neighbors.size)
      ) {
        next = owner
      }
    }
    const owner = next!
    const preferred = preferredColor(owner.name)
    let color = -1
    for (let offset = 0; offset < LAND_OWNER_COLORS.length; offset++) {
      const candidate = (preferred + offset) % LAND_OWNER_COLORS.length
      if (!owner.neighborColors.has(candidate)) {
        color = candidate
        break
      }
    }
    // Disconnected holdings of one owner can require more than four colors.
    if (color < 0) {
      color = LAND_OWNER_COLORS.length
      while (owner.neighborColors.has(color)) color++
    }
    colors.set(
      owner.name,
      LAND_OWNER_COLORS[color] ??
        `hsla(${(30 + (color - LAND_OWNER_COLORS.length) * 137.508) % 360}, 75%, 65%, 0.5)`
    )
    remaining.delete(owner)
    for (const neighbor of owner.neighbors) {
      neighbor.neighborColors.add(color)
    }
  }
  return colors
}
