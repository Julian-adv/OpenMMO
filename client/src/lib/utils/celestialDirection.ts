const HOURS_PER_DAY = 24
const DEFAULT_DAYS_PER_YEAR = 360
const DEFAULT_SPRING_EQUINOX_DAY_INDEX = 90

export interface CelestialDirection {
  x: number
  y: number
  z: number
}

export interface DeclinationConfig {
  dayCountPerYear?: number
  springEquinoxDayIndex?: number
}

export function getDeclinationRadFromDayIndex(
  dayIndex: number,
  axialTiltDeg: number,
  config: DeclinationConfig = {}
) {
  const dayCountPerYear = config.dayCountPerYear ?? DEFAULT_DAYS_PER_YEAR
  const springEquinoxDayIndex =
    config.springEquinoxDayIndex ?? DEFAULT_SPRING_EQUINOX_DAY_INDEX
  const axialTiltRad = (axialTiltDeg * Math.PI) / 180
  const phase =
    (2 * Math.PI * (dayIndex - springEquinoxDayIndex)) / dayCountPerYear

  return axialTiltRad * Math.sin(phase)
}

export function getCelestialDirectionFromHourAndDeclination(
  hour: number,
  transitHour: number,
  latitudeDeg: number,
  declinationRad: number
): CelestialDirection {
  const latitudeRad = (latitudeDeg * Math.PI) / 180
  const latitudeCos = Math.cos(latitudeRad)
  const latitudeSin = Math.sin(latitudeRad)
  const cosDeclination = Math.cos(declinationRad)
  const sinDeclination = Math.sin(declinationRad)
  const hourAngle = (2 * Math.PI * (hour - transitHour)) / HOURS_PER_DAY

  const east = -cosDeclination * Math.sin(hourAngle)
  const north =
    latitudeCos * sinDeclination -
    latitudeSin * cosDeclination * Math.cos(hourAngle)
  const up =
    latitudeSin * sinDeclination +
    latitudeCos * cosDeclination * Math.cos(hourAngle)

  return {
    x: east,
    y: up,
    z: -north,
  }
}
