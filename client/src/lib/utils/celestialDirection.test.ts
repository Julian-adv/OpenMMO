import { describe, it, expect } from 'vitest'
import {
  getDeclinationRadFromDayIndex,
  getCelestialDirectionFromHourAndDeclination,
} from './celestialDirection'

// Defaults (from the module):
//   DAYS_PER_YEAR = 360
//   SPRING_EQUINOX_DAY_INDEX = 90

describe('getDeclinationRadFromDayIndex', () => {
  const tiltDeg = 23.5
  const tiltRad = (tiltDeg * Math.PI) / 180

  it('is 0 at spring equinox', () => {
    expect(getDeclinationRadFromDayIndex(90, tiltDeg)).toBeCloseTo(0)
  })

  it('is +tilt at summer solstice (quarter-year after spring)', () => {
    expect(getDeclinationRadFromDayIndex(90 + 90, tiltDeg)).toBeCloseTo(tiltRad)
  })

  it('is 0 at autumn equinox (half-year after spring)', () => {
    expect(getDeclinationRadFromDayIndex(90 + 180, tiltDeg)).toBeCloseTo(0)
  })

  it('is -tilt at winter solstice (three-quarters after spring)', () => {
    expect(getDeclinationRadFromDayIndex(90 + 270, tiltDeg)).toBeCloseTo(
      -tiltRad
    )
  })

  it('returns 0 for any day when axial tilt is 0', () => {
    for (const day of [0, 30, 90, 180, 270, 359]) {
      expect(getDeclinationRadFromDayIndex(day, 0)).toBeCloseTo(0)
    }
  })

  it('is periodic over the configured year length', () => {
    const a = getDeclinationRadFromDayIndex(42, tiltDeg)
    const b = getDeclinationRadFromDayIndex(42 + 360, tiltDeg)
    expect(b).toBeCloseTo(a)
  })

  it('respects custom dayCountPerYear and springEquinoxDayIndex', () => {
    // 100-day year, spring at day 0; summer solstice at day 25.
    const r = getDeclinationRadFromDayIndex(25, tiltDeg, {
      dayCountPerYear: 100,
      springEquinoxDayIndex: 0,
    })
    expect(r).toBeCloseTo(tiltRad)
  })
})

describe('getCelestialDirectionFromHourAndDeclination', () => {
  const TRANSIT = 12

  function mag(v: { x: number; y: number; z: number }) {
    return Math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z)
  }

  it('always returns a unit vector', () => {
    const cases = [
      { hour: 0, lat: 0, dec: 0 },
      { hour: 6, lat: 45, dec: 0 },
      { hour: 12, lat: -30, dec: 0.4 },
      { hour: 18, lat: 60, dec: -0.3 },
      { hour: 23.5, lat: 90, dec: 0.1 },
    ]
    for (const { hour, lat, dec } of cases) {
      const v = getCelestialDirectionFromHourAndDeclination(
        hour,
        TRANSIT,
        lat,
        dec
      )
      expect(mag(v)).toBeCloseTo(1)
    }
  })

  it('places the sun overhead at equator during equinox at transit', () => {
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT,
      TRANSIT,
      0,
      0
    )
    expect(v.x).toBeCloseTo(0)
    expect(v.y).toBeCloseTo(1)
    expect(v.z).toBeCloseTo(0)
  })

  it('sets on the western horizon 6 hours past transit at equator', () => {
    // x = east; west → x negative, y (up) = 0, z (south) = 0.
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT + 6,
      TRANSIT,
      0,
      0
    )
    expect(v.x).toBeCloseTo(-1)
    expect(v.y).toBeCloseTo(0)
    expect(v.z).toBeCloseTo(0)
  })

  it('rises on the eastern horizon 6 hours before transit at equator', () => {
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT - 6,
      TRANSIT,
      0,
      0
    )
    expect(v.x).toBeCloseTo(1)
    expect(v.y).toBeCloseTo(0)
    expect(v.z).toBeCloseTo(0)
  })

  it('points below the horizon at midnight (equinox, equator)', () => {
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT + 12,
      TRANSIT,
      0,
      0
    )
    expect(v.x).toBeCloseTo(0)
    expect(v.y).toBeCloseTo(-1)
    expect(v.z).toBeCloseTo(0)
  })

  it('is periodic with a 24-hour period', () => {
    const a = getCelestialDirectionFromHourAndDeclination(5, TRANSIT, 37, 0.2)
    const b = getCelestialDirectionFromHourAndDeclination(29, TRANSIT, 37, 0.2)
    expect(b.x).toBeCloseTo(a.x)
    expect(b.y).toBeCloseTo(a.y)
    expect(b.z).toBeCloseTo(a.z)
  })

  it('mirrors east/west around transit (equinox, equator)', () => {
    const before = getCelestialDirectionFromHourAndDeclination(
      TRANSIT - 3,
      TRANSIT,
      0,
      0
    )
    const after = getCelestialDirectionFromHourAndDeclination(
      TRANSIT + 3,
      TRANSIT,
      0,
      0
    )
    expect(before.x).toBeCloseTo(-after.x)
    expect(before.y).toBeCloseTo(after.y)
    expect(before.z).toBeCloseTo(after.z)
  })

  it('tilts sun toward +z (south) at transit for a northern observer', () => {
    // lat=45°N, equinox, transit: altitude = 45°, azimuth = due south (+z).
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT,
      TRANSIT,
      45,
      0
    )
    expect(v.x).toBeCloseTo(0)
    expect(v.y).toBeCloseTo(Math.cos((45 * Math.PI) / 180)) // sin(alt)=sin(45)=cos(45)
    expect(v.z).toBeCloseTo(Math.sin((45 * Math.PI) / 180)) // southward
  })

  it('tilts sun toward -z (north) at transit for a southern observer', () => {
    const v = getCelestialDirectionFromHourAndDeclination(
      TRANSIT,
      TRANSIT,
      -45,
      0
    )
    expect(v.x).toBeCloseTo(0)
    expect(v.y).toBeCloseTo(Math.cos((45 * Math.PI) / 180))
    expect(v.z).toBeCloseTo(-Math.sin((45 * Math.PI) / 180))
  })
})
