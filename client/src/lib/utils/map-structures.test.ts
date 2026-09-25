import { describe, expect, it } from 'vitest'
import { drawLandPlotCells, drawLandPlotGrid } from './map-structures'
import { buildLandOwnerColors, OWN_LAND_COLOR } from './landPlotColors'
import {
  LandGrade,
  plotAddress,
  plotOrigin,
  REGION_PLOTS,
} from '../terrain/landPlots'

function stubCtx() {
  const lines: {
    x0: number
    y0: number
    x1: number
    y1: number
    width: number
  }[] = []
  let start = { x: 0, y: 0 }
  const ctx = {
    lineWidth: 1,
    save() {},
    restore() {},
    beginPath() {},
    stroke() {},
    moveTo(x: number, y: number) {
      start = { x, y }
    },
    lineTo(x: number, y: number) {
      lines.push({
        x0: start.x,
        y0: start.y,
        x1: x,
        y1: y,
        width: ctx.lineWidth,
      })
    },
  }
  return { ctx: ctx as unknown as CanvasRenderingContext2D, lines }
}

const transform = { centerX: 0, viewLeft: 0, viewTop: 0, scale: 1 }

describe('drawLandPlotGrid', () => {
  it('draws 32 m plot lines', () => {
    const { ctx, lines } = stubCtx()
    drawLandPlotGrid(ctx, 128, transform)
    const vertical = lines
      .filter((l) => l.x0 === l.x1)
      .map((l) => [l.x0, l.width])
    expect(vertical).toEqual([
      [0, 0.75],
      [32, 0.75],
      [64, 0.75],
      [96, 0.75],
      [128, 0.75],
    ])
  })

  it('skips drawing when plots would be under 4 px', () => {
    const { ctx, lines } = stubCtx()
    drawLandPlotGrid(ctx, 1024, { ...transform, scale: 0.1 })
    expect(lines).toHaveLength(0)
  })

  it('fills graded plots at their world origin', () => {
    const rects: number[][] = []
    const ctx = {
      save() {},
      restore() {},
      fillStyle: '',
      fillRect(x: number, y: number, w: number, h: number) {
        rects.push([x, y, w, h])
      },
    } as unknown as CanvasRenderingContext2D
    const grades = new Uint8Array(REGION_PLOTS).fill(LandGrade.Homestead)
    const addr = plotAddress(1000, 500)
    grades[addr.index] = LandGrade.Crown
    drawLandPlotCells(
      ctx,
      [{ rx: addr.rx, rz: addr.rz, grades }],
      transform,
      new Map()
    )
    const o = plotOrigin(addr.rx, addr.rz, addr.index)
    expect(rects).toEqual([[o.x, o.z, 32, 32]])
  })

  it('prioritizes owner colors over grades, including before grades load', () => {
    const fills: string[] = []
    const ctx = {
      save() {},
      restore() {},
      fillStyle: '',
      fillRect() {
        fills.push(this.fillStyle)
      },
    }
    const grades = new Uint8Array(REGION_PLOTS).fill(LandGrade.Homestead)
    grades[0] = LandGrade.Crown
    grades[1] = LandGrade.Reserved
    grades[2] = LandGrade.Crown
    const ownerColors = buildLandOwnerColors([
      { rx: 0, rz: 0, index: 0, ownerName: 'Alice' },
      { rx: 0, rz: 0, index: 1, ownerName: 'Bob' },
      { rx: 1, rz: 0, index: 0, ownerName: 'Alice' },
    ])
    drawLandPlotCells(
      ctx as unknown as CanvasRenderingContext2D,
      [
        {
          rx: 0,
          rz: 0,
          grades,
          owners: new Map([
            [0, 'Alice'],
            [1, 'Bob'],
          ]),
        },
        { rx: 1, rz: 0, grades: null, owners: new Map([[0, 'Alice']]) },
      ],
      transform,
      ownerColors,
      'Alice'
    )
    expect(fills).toEqual([
      OWN_LAND_COLOR,
      ownerColors.get('Bob'),
      'rgba(255, 196, 64, 0.5)',
      OWN_LAND_COLOR,
    ])
  })

  it('preserves owner colors when the viewport contains only part of their land', () => {
    const ownerColors = buildLandOwnerColors([
      { rx: 0, rz: 0, index: 0, ownerName: 'Alice' },
      { rx: 0, rz: 0, index: 1, ownerName: 'Bob' },
      { rx: 1, rz: 0, index: 0, ownerName: 'Alice' },
    ])
    const fills: string[] = []
    const ctx = {
      save() {},
      restore() {},
      fillStyle: '',
      fillRect() {
        fills.push(this.fillStyle)
      },
    }
    for (const rx of [0, 1]) {
      drawLandPlotCells(
        ctx as unknown as CanvasRenderingContext2D,
        [{ rx, rz: 0, grades: null, owners: new Map([[0, 'Alice']]) }],
        { ...transform, centerX: rx * 1024 },
        ownerColors
      )
    }
    expect(fills).toEqual([ownerColors.get('Alice'), ownerColors.get('Alice')])
    expect(fills[0]).not.toBe(OWN_LAND_COLOR)
  })
})
