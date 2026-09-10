import type { MapCanvasTransform } from './map-structures'
import { unwrapWorldXNear } from '../terrain/world-wrap'

/** One rain cell as returned by `weather_cells_at`. */
export interface RainCellView {
  x: number
  z: number
  radius_m: number
  env: number
  stage: string
}

/** Longest look-ahead the forecast slider offers, in game minutes. */
export const FORECAST_MAX_MINUTES = 12 * 60
export const FORECAST_STEP_MINUTES = 30

const CELL_CORE_ALPHA = 0.72
/** Mirrors `weather::CELL_CORE_SHARE`: the disc fades exactly where the rain does. */
const CELL_CORE_SHARE = 0.7

/** Rain cells as soft discs so the map reads like a satellite cloud layer.
 *  Drawn onto the unrotated atlas: x is unwrapped toward the view centre so a
 *  cell just across the world seam shows on the near side. */
export function drawRainCells(
  ctx: CanvasRenderingContext2D,
  cells: RainCellView[],
  transform: MapCanvasTransform
) {
  ctx.save()
  for (const cell of cells) {
    if (cell.env <= 0) continue
    const cx =
      (unwrapWorldXNear(transform.centerX, cell.x) - transform.viewLeft) *
      transform.scale
    const cy = (cell.z - transform.viewTop) * transform.scale
    const r = cell.radius_m * transform.scale
    if (r < 1) continue
    const alpha = CELL_CORE_ALPHA * cell.env
    const gradient = ctx.createRadialGradient(cx, cy, 0, cx, cy, r)
    gradient.addColorStop(0, `rgba(88, 96, 112, ${alpha})`)
    gradient.addColorStop(CELL_CORE_SHARE, `rgba(88, 96, 112, ${alpha})`)
    gradient.addColorStop(1, 'rgba(88, 96, 112, 0)')
    ctx.fillStyle = gradient
    ctx.beginPath()
    ctx.arc(cx, cy, r, 0, Math.PI * 2)
    ctx.fill()
  }
  ctx.restore()
}

/** Slider caption: "Now", "+30m", "+2h", "+2h 30m". */
export function forecastLabel(offsetMinutes: number): string {
  if (offsetMinutes <= 0) return 'Now'
  const hours = Math.floor(offsetMinutes / 60)
  const minutes = offsetMinutes % 60
  if (hours === 0) return `+${minutes}m`
  if (minutes === 0) return `+${hours}h`
  return `+${hours}h ${minutes}m`
}
