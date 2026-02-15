import {
  getCelestialDirectionFromHourAndDeclination,
  getDeclinationRadFromDayIndex,
} from './celestialDirection'

export interface SunSimulationConfig {
  latitudeDeg: number
  sunriseHour: number
  dayDurationSeconds: number
  startHour: number
  startMonth?: number
  startDay?: number
  axialTiltDeg?: number
  lightDistance: number
  maxIntensity: number
}

export interface SunVector {
  x: number
  y: number
  z: number
}

export interface SunLightState {
  gameHour: number
  direction: SunVector
  positionOffset: SunVector
  intensity: number
}

export interface SunLightSimulation {
  advance: (deltaSeconds: number) => void
  getGameHour: () => number
  setGameHour: (hour: number) => void
  setCalendarDate: (month: number, day: number) => void
  getLightState: () => SunLightState
}

export interface SolarDaylightWindowConfig {
  latitudeDeg: number
  month: number
  day: number
  axialTiltDeg?: number
}

export interface SolarDaylightWindow {
  sunriseHour: number
  sunsetHour: number
  dayLengthHours: number
}

const HOURS_PER_DAY = 24
const MONTHS_PER_YEAR = 12
const DAYS_PER_MONTH = 30
const DAYLIGHT_SOFTENING_EXPONENT = 0.7
const DAYLIGHT_FLOOR = 0.4

function normalizeHour(hour: number) {
  return ((hour % HOURS_PER_DAY) + HOURS_PER_DAY) % HOURS_PER_DAY
}

function clampMonth(month: number) {
  return Math.min(MONTHS_PER_YEAR, Math.max(1, Math.floor(month)))
}

function clampDay(day: number) {
  return Math.min(DAYS_PER_MONTH, Math.max(1, Math.floor(day)))
}

function dayOfYearFromCalendar(month: number, day: number) {
  const clampedMonth = clampMonth(month)
  const clampedDay = clampDay(day)
  return (clampedMonth - 1) * DAYS_PER_MONTH + clampedDay
}

export function getSolarDaylightWindow(
  config: SolarDaylightWindowConfig
): SolarDaylightWindow {
  const dayOfYear = dayOfYearFromCalendar(config.month, config.day)
  const latitudeRad = (config.latitudeDeg * Math.PI) / 180
  const axialTiltDeg = config.axialTiltDeg ?? 24
  const declination = getDeclinationRadFromDayIndex(dayOfYear, axialTiltDeg)
  const cosHourAngle = -Math.tan(latitudeRad) * Math.tan(declination)

  if (cosHourAngle <= -1) {
    return {
      sunriseHour: 0,
      sunsetHour: HOURS_PER_DAY,
      dayLengthHours: HOURS_PER_DAY,
    }
  }

  if (cosHourAngle >= 1) {
    return {
      sunriseHour: 12,
      sunsetHour: 12,
      dayLengthHours: 0,
    }
  }

  const hourAngle = Math.acos(cosHourAngle)
  const dayLengthHours = (HOURS_PER_DAY * hourAngle) / Math.PI

  return {
    sunriseHour: 12 - dayLengthHours / 2,
    sunsetHour: 12 + dayLengthHours / 2,
    dayLengthHours,
  }
}

export function createSunLightSimulation(
  config: SunSimulationConfig
): SunLightSimulation {
  const latitudeRad = (config.latitudeDeg * Math.PI) / 180
  const latitudeCos = Math.cos(latitudeRad)
  const axialTiltDeg = config.axialTiltDeg ?? 24
  let elapsedSeconds =
    (normalizeHour(config.startHour) / HOURS_PER_DAY) *
    config.dayDurationSeconds
  let dayOfYear = dayOfYearFromCalendar(
    config.startMonth ?? 1,
    config.startDay ?? 1
  )

  function getGameHour() {
    return (elapsedSeconds / config.dayDurationSeconds) * HOURS_PER_DAY
  }

  function setGameHour(hour: number) {
    elapsedSeconds =
      (normalizeHour(hour) / HOURS_PER_DAY) * config.dayDurationSeconds
  }

  function setCalendarDate(month: number, day: number) {
    dayOfYear = dayOfYearFromCalendar(month, day)
  }

  function getSolarDeclinationRad() {
    return getDeclinationRadFromDayIndex(dayOfYear, axialTiltDeg)
  }

  function getSunDirectionFromHour(hour: number): SunVector {
    const declination = getSolarDeclinationRad()
    return getCelestialDirectionFromHourAndDeclination(
      hour,
      12,
      config.latitudeDeg,
      declination
    )
  }

  function advance(deltaSeconds: number) {
    elapsedSeconds = (elapsedSeconds + deltaSeconds) % config.dayDurationSeconds
    if (elapsedSeconds < 0) {
      elapsedSeconds += config.dayDurationSeconds
    }
  }

  function getLightState(): SunLightState {
    const gameHour = getGameHour()
    const direction = getSunDirectionFromHour(gameHour)
    const baseDaylightFactor = Math.min(
      1,
      Math.max(0, direction.y / Math.max(latitudeCos, 1e-6))
    )
    const softenedDaylightFactor = Math.pow(
      baseDaylightFactor,
      DAYLIGHT_SOFTENING_EXPONENT
    )
    const daylightFactor =
      direction.y > 0
        ? DAYLIGHT_FLOOR + (1 - DAYLIGHT_FLOOR) * softenedDaylightFactor
        : 0

    return {
      gameHour,
      direction,
      positionOffset: {
        x: direction.x * config.lightDistance,
        y: direction.y * config.lightDistance,
        z: direction.z * config.lightDistance,
      },
      intensity: config.maxIntensity * daylightFactor,
    }
  }

  return {
    advance,
    getGameHour,
    setGameHour,
    setCalendarDate,
    getLightState,
  }
}
