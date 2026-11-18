export interface SnowClearing {
  /** Road centreline segments, each `[ax, az, bx, bz]`. */
  segments: [number, number, number, number][]
  /** Game minute the stretch was shovelled. */
  at_min: number
}

/** Game minutes a shovelled stretch stays clear; mirrors the server. */
export const SNOW_CLEAR_LASTS_MIN = 240
/** Snow creeps back over the last part of that time. */
const RETURN_FROM = 0.7
/** Half the cleared width, and the soft shoulder beyond it, in metres. */
const HALF_WIDTH_M = 2.6
const SHOULDER_M = 1.2

/** 0..1 how clear a stretch shovelled `ageMin` game minutes ago still is. */
export function clearStrength(ageMin: number): number {
  if (ageMin < 0 || ageMin >= SNOW_CLEAR_LASTS_MIN) return 0
  const t = ageMin / SNOW_CLEAR_LASTS_MIN
  return t <= RETURN_FROM ? 1 : 1 - (t - RETURN_FROM) / (1 - RETURN_FROM)
}

/** Paints the cleared roads around `(centreX, centreZ)` into `out`, a square
 *  of `resolution` texels covering `sizeM` metres; 255 is fully clear. The
 *  outer ring of texels stays 0 so sampling past the edge reads as snow. */
export function paintRoadSnowMask(
  out: Uint8Array,
  resolution: number,
  sizeM: number,
  centreX: number,
  centreZ: number,
  stretches: readonly SnowClearing[],
  nowMin: number
): void {
  out.fill(0)
  const texel = sizeM / resolution
  const minX = centreX - sizeM / 2
  const minZ = centreZ - sizeM / 2
  const reach = HALF_WIDTH_M + SHOULDER_M
  for (const stretch of stretches) {
    const strength = clearStrength(nowMin - stretch.at_min)
    if (strength <= 0) continue
    for (const [ax, az, bx, bz] of stretch.segments) {
      const x0 = Math.max(
        1,
        Math.floor((Math.min(ax, bx) - reach - minX) / texel)
      )
      const x1 = Math.min(
        resolution - 2,
        Math.ceil((Math.max(ax, bx) + reach - minX) / texel)
      )
      const z0 = Math.max(
        1,
        Math.floor((Math.min(az, bz) - reach - minZ) / texel)
      )
      const z1 = Math.min(
        resolution - 2,
        Math.ceil((Math.max(az, bz) + reach - minZ) / texel)
      )
      const dx = bx - ax
      const dz = bz - az
      const lengthSq = dx * dx + dz * dz
      for (let iz = z0; iz <= z1; iz++) {
        const pz = minZ + (iz + 0.5) * texel
        for (let ix = x0; ix <= x1; ix++) {
          const px = minX + (ix + 0.5) * texel
          const t =
            lengthSq === 0
              ? 0
              : Math.min(
                  1,
                  Math.max(0, ((px - ax) * dx + (pz - az) * dz) / lengthSq)
                )
          const distance = Math.hypot(px - ax - t * dx, pz - az - t * dz)
          const edge =
            1 - Math.min(1, Math.max(0, (distance - HALF_WIDTH_M) / SHOULDER_M))
          const value = Math.round(255 * strength * edge)
          const index = iz * resolution + ix
          if (value > out[index]) out[index] = value
        }
      }
    }
  }
}
