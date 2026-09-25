import { describe, expect, it } from 'vitest'
import { plotAddress, type OwnedLandPlot } from '../terrain/landPlots'
import { LAND_PLOT_SIZE } from '../terrain/terrain-constants'
import { WORLD_MAX_X, WORLD_MIN_X } from '../terrain/world-wrap'
import { buildLandOwnerColors, LAND_OWNER_COLORS } from './landPlotColors'

function plot(x: number, z: number, ownerName: string): OwnedLandPlot {
  return { ...plotAddress(x, z), ownerName }
}

function layout(rows: string[]): OwnedLandPlot[] {
  return rows.flatMap((row, z) =>
    [...row].flatMap((owner, x) =>
      owner === '.' ? [] : [plot(x * LAND_PLOT_SIZE, z * LAND_PLOT_SIZE, owner)]
    )
  )
}

describe('buildLandOwnerColors', () => {
  it('uses four colors for four mutually neighboring estates', () => {
    const colors = buildLandOwnerColors(
      layout(['AAAA', 'ABCA', 'ADCA', 'AAAA'])
    )
    expect(colors.size).toBe(4)
    expect(new Set(colors.values())).toEqual(new Set(LAND_OWNER_COLORS))
  })

  it('is independent of ownership response order', () => {
    const plots = layout(['AAAA', 'ABCA', 'ADCA', 'AAAA', '..EF'])
    expect(buildLandOwnerColors(plots)).toEqual(
      buildLandOwnerColors([...plots].reverse())
    )
  })

  it.each([
    ['tile', 0, 0],
    ['region', 960, 0],
    ['negative region', -64, -1024],
    ['world seam', WORLD_MAX_X - LAND_PLOT_SIZE, 0],
  ])('separates neighboring owners across a %s boundary', (_, x, z) => {
    const colors = buildLandOwnerColors([
      plot(x, z, 'Owner 0'),
      plot(x + LAND_PLOT_SIZE, z, 'Owner 4'),
    ])
    expect(colors.get('Owner 0')).not.toBe(colors.get('Owner 4'))
  })

  it('keeps one owner color across regions, the world seam and separate holdings', () => {
    const colors = buildLandOwnerColors([
      plot(960, 0, 'Alice'),
      plot(992, 0, 'Alice'),
      plot(WORLD_MAX_X - LAND_PLOT_SIZE, 0, 'Alice'),
      plot(WORLD_MIN_X, 0, 'Alice'),
      plot(5000, 5000, 'Alice'),
      plot(WORLD_MIN_X + LAND_PLOT_SIZE, 0, 'Bob'),
    ])
    expect(colors.size).toBe(2)
    expect(colors.get('Alice')).not.toBe(colors.get('Bob'))
  })

  it('allows color reuse when plots meet only at a corner or have a gap', () => {
    const colors = buildLandOwnerColors([
      plot(0, 0, 'Owner 0'),
      plot(32, 32, 'Owner 4'),
      plot(96, 0, 'Owner 8'),
    ])
    expect(colors.size).toBe(3)
    expect(new Set(colors.values()).size).toBe(1)
  })

  it('keeps all borders distinguishable when disconnected holdings require extra colors', () => {
    const plots: OwnedLandPlot[] = []
    const owners = ['Alice', 'Bob', 'Cora', 'Dan', 'Eve']
    for (let a = 0; a < owners.length; a++) {
      for (let b = a + 1; b < owners.length; b++) {
        const z = plots.length * LAND_PLOT_SIZE
        plots.push(plot(0, z, owners[a]), plot(LAND_PLOT_SIZE, z, owners[b]))
      }
    }
    const colors = buildLandOwnerColors(plots)
    expect(colors.size).toBe(5)
    expect(new Set(colors.values()).size).toBe(5)
  })

  it('handles an empty ownership snapshot', () => {
    expect(buildLandOwnerColors([]).size).toBe(0)
  })
})
