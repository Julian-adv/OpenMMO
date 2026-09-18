import { weather_cells_at, weather_rain_at } from '../wasm/onlinerpg_shared'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

/** One live rain cell, as `weather_cells_at` serialises it. */
export interface RadarCell {
  sector: number
  zone: number
  x: number
  z: number
  radiusM: number
  env: number
  progress: number
  remainMin: number
  stage: 'forming' | 'raining' | 'clearing'
}

export const ZONE_NAMES = [
  'Sea',
  'Wet coast',
  'Temperate',
  'Rain shadow',
  'Alpine',
] as const

export function zoneName(zone: number): string {
  return ZONE_NAMES[zone] ?? `Zone ${zone}`
}

export function cellsAt(seed: number, bias: number, tMin: number): RadarCell[] {
  return weather_cells_at(seed, bias, tMin) as RadarCell[]
}

/** World rectangle the radar draws, in metres. Covers the main continent. */
export interface RadarView {
  x0: number
  x1: number
  z0: number
  z1: number
}

export const CONTINENT_VIEW: RadarView = {
  x0: -9500,
  x1: 12500,
  z0: -4000,
  z1: 11500,
}

/**
 * The copy of `x` that lies nearest the view on the cylindrical world. A cell
 * just west of the seam rains into the east of the view, and `rain_at` sees it
 * that way; drawing its canonical x would put the disc off the far side.
 */
export function viewWrappedX(x: number, view: RadarView): number {
  const centre = (view.x0 + view.x1) / 2
  return centre + shortestWrappedDeltaX(centre, x)
}

/** True when a cell's disc reaches into the drawn window. */
export function cellInView(
  cell: Pick<RadarCell, 'x' | 'z' | 'radiusM'>,
  view: RadarView
): boolean {
  const x = viewWrappedX(cell.x, view)
  const nearestX = Math.min(Math.max(x, view.x0), view.x1)
  const nearestZ = Math.min(Math.max(cell.z, view.z0), view.z1)
  const dx = x - nearestX
  const dz = cell.z - nearestZ
  return dx * dx + dz * dz <= cell.radiusM * cell.radiusM
}

/** World metres → canvas pixels. The canvas keeps the view's aspect, so one
 *  scale serves both axes and circles stay round. */
export function worldToCanvas(
  x: number,
  z: number,
  view: RadarView,
  width: number
): { x: number; y: number } {
  const scale = width / (view.x1 - view.x0)
  return { x: (x - view.x0) * scale, y: (z - view.z0) * scale }
}

export function canvasHeightFor(view: RadarView, width: number): number {
  return Math.round((width * (view.z1 - view.z0)) / (view.x1 - view.x0))
}

/** Canvas pixels → world metres, for hover readouts. */
export function canvasToWorld(
  px: number,
  py: number,
  view: RadarView,
  width: number
): { x: number; z: number } {
  const scale = width / (view.x1 - view.x0)
  return { x: view.x0 + px / scale, z: view.z0 + py / scale }
}

/** Game minutes the next-rain search looks ahead: half a game day. */
export const RAIN_SEARCH_HORIZON_MIN = 720

/**
 * Minutes until rain next reaches `threshold` at a position, or null if it
 * stays dry through the horizon.
 *
 * A plain scan, deliberately: a cell's rim can pass over a point in a couple
 * of minutes, so a coarse stride with a refine step would skip whole showers
 * rather than merely round them. One game minute is 7.5 real seconds, which
 * is below anything worth announcing.
 */
export function minutesUntilRain(
  seed: number,
  bias: number,
  tMin: number,
  x: number,
  z: number,
  horizonMin = RAIN_SEARCH_HORIZON_MIN,
  stepMin = 1,
  threshold = 0.05
): number | null {
  for (let ahead = 0; ahead <= horizonMin; ahead += stepMin) {
    if (weather_rain_at(seed, bias, tMin + ahead, x, z) >= threshold) {
      return ahead
    }
  }
  return null
}

/** Game minutes as a compact duration: "45m", "2h 05m", "1d 03h". */
export function formatGameMinutes(minutes: number): string {
  const total = Math.max(0, Math.round(minutes))
  if (total < 60) return `${total}m`
  const hours = Math.floor(total / 60)
  if (hours < 24) return `${hours}h ${String(total % 60).padStart(2, '0')}m`
  return `${Math.floor(hours / 24)}d ${String(hours % 24).padStart(2, '0')}h`
}

/** A game day is 3 real hours, so a game minute is 7.5 real seconds. */
export const REAL_SECONDS_PER_GAME_MINUTE = 7.5

export function formatRealMinutes(gameMinutes: number): string {
  const real = (gameMinutes * REAL_SECONDS_PER_GAME_MINUTE) / 60
  return real < 1 ? '<1 real min' : `${Math.round(real)} real min`
}
