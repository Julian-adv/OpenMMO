import {
  getCelestialDirectionFromHourAndDeclination,
  getDeclinationRadFromDayIndex,
} from './celestialDirection'

export const HOURS_PER_DAY = 24
export const DAYS_PER_MONTH = 30
export const MONTHS_PER_YEAR = 12
export const DAYS_PER_YEAR = DAYS_PER_MONTH * MONTHS_PER_YEAR

export const SUN_LATITUDE_DEG = 40
export const SUN_AXIAL_TILT_DEG = 24
export const SUN_LIGHT_DISTANCE = 120
export const SHADOW_CAMERA_EXTENT = 80
export const SHADOW_CAMERA_FAR = SUN_LIGHT_DISTANCE * 3
export const SUN_DAY_DURATION_SECONDS = 3 * 60 * 60
export const SUN_START_HOUR = 12
export const SUN_MAX_INTENSITY = 2.25
export const SUN_TWILIGHT_ELEVATION_THRESHOLD = 0.11
export const SUN_TWILIGHT_COLOR_BLEND = 0.65
export const MOON_AXIAL_TILT_DEG = 19

export const GAME_START_YEAR = 217
export const GAME_MONTHS_PER_YEAR = 12
export const GAME_DAYS_PER_MONTH = 30

export const SUN_DAY_COLOR_HEX = '#ffffff'
export const SUN_TWILIGHT_COLOR_HEX = '#ff9b86'
export const MOON_LIGHT_COLOR_HEX = '#d6e2ff'
export const MOON_VISIBILITY_THRESHOLD = 0.02
export const ELDER_MOON_MAX_INTENSITY = 1.2
export const SWIFT_MOON_MAX_INTENSITY = 0.9
export const MOON_ILLUMINATION_SOFTENING_EXPONENT = 0.7
export const MOON_LIGHT_FLOOR = 0.3

// Moon surface colors for canvas rendering
// Lit side: base RGB at minimum shade, plus per-channel offset scaled by shade
export const MOON_LIT_BASE = 210
export const MOON_LIT_RANGE = 45
export const MOON_LIT_SHADE_MIN = 0.75
export const MOON_LIT_R_OFFSET = -4
export const MOON_LIT_G_OFFSET = 0
export const MOON_LIT_B_OFFSET = 8
export const MOON_LIT_ALPHA = 255

// Dark side: base RGB at minimum shade, plus per-channel offset
export const MOON_DARK_BASE = 58
export const MOON_DARK_RANGE = 42
export const MOON_DARK_SHADE_MIN = 0.16
export const MOON_DARK_R_OFFSET = 0
export const MOON_DARK_G_OFFSET = 2
export const MOON_DARK_B_OFFSET = 8
export const MOON_DARK_ALPHA = 255

export interface CalendarDate {
  year: number
  month: number
  day: number
}

export interface MoonDefinition {
  id: 'elder' | 'swift'
  displayName: string
  alias: string
  periodDays: number
  phaseOffsetDays: number
}

export interface MoonPhaseState {
  cycleDay: number
  fullMoonDay: number
  illumination: number
  isWaxing: boolean
  orbitalProgress: number
  transitHour: number
  riseHour: number
  normalizedHour: number
  hoursSinceRise: number
  isAboveHorizon: boolean
}

export interface SunTrackConfig {
  hour: number
  sunriseHour: number
  sunsetHour: number
  leftPercent: number
  rightPercent: number
  horizonYPercent: number
  arcHeightPercent: number
}

export interface SunTrackState {
  xPercent: number
  yPercent: number
  isDaylight: boolean
}

export interface MoonTrackConfig {
  phaseState: MoonPhaseState
  isDaylight: boolean
  leftPercent: number
  rightPercent: number
  horizonYPercent: number
  arcHeightPercent: number
  daylightVisibilityScale: number
  visibilityThreshold: number
}

export interface MoonTrackState {
  xPercent: number
  yPercent: number
  opacity: number
  isVisible: boolean
}

export interface MoonCanvasParams {
  illumination: number
  isWaxing: boolean
  sizePx: number
  isDaylight?: boolean
}

export interface SunLightSnapshot {
  gameHour: number
  direction: { x: number; y: number; z: number }
  positionOffset: { x: number; y: number; z: number }
  intensity: number
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

export interface CelestialDirectionalLightState {
  useMoonLight: boolean
  positionOffset: { x: number; y: number; z: number }
  intensity: number
  ambientNightFactor: number
  sunColorBlendFactor: number
}

export interface CelestialLightState {
  directional: CelestialDirectionalLightState
  ambientNightFactor: number
  ambientIntensity: number
}

export const ELDER_MOON_DEFINITION: MoonDefinition = {
  id: 'elder',
  displayName: 'Eldor',
  alias: 'Elder',
  periodDays: 30,
  phaseOffsetDays: 0,
}

export const SWIFT_MOON_DEFINITION: MoonDefinition = {
  id: 'swift',
  displayName: 'Serin',
  alias: 'Swift',
  periodDays: 20,
  phaseOffsetDays: 5,
}

export function normalizeHour(hour: number) {
  return ((hour % HOURS_PER_DAY) + HOURS_PER_DAY) % HOURS_PER_DAY
}

export function positiveModulo(value: number, mod: number) {
  return ((value % mod) + mod) % mod
}

export function getAbsoluteDayIndex(date: CalendarDate) {
  const normalizedYear = Math.max(1, Math.floor(date.year))
  const normalizedMonth = Math.min(
    MONTHS_PER_YEAR,
    Math.max(1, Math.floor(date.month))
  )
  const normalizedDay = Math.min(
    DAYS_PER_MONTH,
    Math.max(1, Math.floor(date.day))
  )
  return (
    (normalizedYear - 1) * DAYS_PER_YEAR +
    (normalizedMonth - 1) * DAYS_PER_MONTH +
    (normalizedDay - 1)
  )
}

export function getGameCalendarDayIndex(date: CalendarDate) {
  const normalizedYear = Math.max(GAME_START_YEAR, Math.floor(date.year))
  const normalizedMonth = Math.min(
    GAME_MONTHS_PER_YEAR,
    Math.max(1, Math.floor(date.month))
  )
  const normalizedDay = Math.min(
    GAME_DAYS_PER_MONTH,
    Math.max(1, Math.floor(date.day))
  )
  const yearsSinceStart = normalizedYear - GAME_START_YEAR
  return (
    yearsSinceStart * GAME_MONTHS_PER_YEAR * GAME_DAYS_PER_MONTH +
    (normalizedMonth - 1) * GAME_DAYS_PER_MONTH +
    (normalizedDay - 1)
  )
}

export function getCalendarDateFromGameDayIndex(
  dayIndex: number
): CalendarDate {
  const normalizedDayIndex = Math.max(0, Math.floor(dayIndex))
  const daysPerYear = GAME_MONTHS_PER_YEAR * GAME_DAYS_PER_MONTH
  const year = GAME_START_YEAR + Math.floor(normalizedDayIndex / daysPerYear)
  const dayOfYear = normalizedDayIndex % daysPerYear
  const month = Math.floor(dayOfYear / GAME_DAYS_PER_MONTH) + 1
  const day = (dayOfYear % GAME_DAYS_PER_MONTH) + 1

  return { year, month, day }
}

export function getMoonIllumination(
  cycleDay: number,
  fullMoonDay: number,
  periodDays: number
) {
  if (cycleDay <= fullMoonDay) {
    return (cycleDay - 1) / Math.max(1, fullMoonDay - 1)
  }

  return 1 - (cycleDay - fullMoonDay) / Math.max(1, periodDays - fullMoonDay)
}

export function getMoonPhaseLabel(illumination: number, isWaxing: boolean) {
  if (illumination <= 0.05) return 'New'
  if (illumination >= 0.95) return 'Full'
  if (illumination >= 0.45 && illumination <= 0.55) {
    return isWaxing ? 'First Quarter' : 'Last Quarter'
  }
  if (isWaxing) return illumination < 0.5 ? 'Waxing Crescent' : 'Waxing Gibbous'
  return illumination < 0.5 ? 'Waning Crescent' : 'Waning Gibbous'
}

export function getMoonPhaseState(
  moon: Pick<MoonDefinition, 'periodDays' | 'phaseOffsetDays'>,
  absoluteDayIndex: number,
  gameHour: number
): MoonPhaseState {
  // Use fractional day progress so moon phase/intensity stays continuous at midnight.
  const normalizedHour = normalizeHour(gameHour)
  const dayProgress = normalizedHour / HOURS_PER_DAY
  const cycleDay =
    positiveModulo(
      absoluteDayIndex + dayProgress + moon.phaseOffsetDays,
      moon.periodDays
    ) + 1
  const fullMoonDay = moon.periodDays / 2
  const illumination = Math.max(
    0,
    Math.min(1, getMoonIllumination(cycleDay, fullMoonDay, moon.periodDays))
  )
  const isWaxing = cycleDay <= fullMoonDay
  const orbitalProgress = isWaxing
    ? ((cycleDay - 1) / Math.max(1, fullMoonDay - 1)) * 0.5
    : 0.5 +
      ((cycleDay - fullMoonDay) / Math.max(1, moon.periodDays - fullMoonDay)) *
        0.5

  // New moon aligns with the sun (transit around noon), full moon transits at midnight.
  const transitHour = normalizeHour(12 + orbitalProgress * HOURS_PER_DAY)
  const riseHour = normalizeHour(transitHour - 6)
  const hoursSinceRise = normalizeHour(normalizedHour - riseHour)
  const isAboveHorizon = hoursSinceRise <= 12

  return {
    cycleDay,
    fullMoonDay,
    illumination,
    isWaxing,
    orbitalProgress,
    transitHour,
    riseHour,
    normalizedHour,
    hoursSinceRise,
    isAboveHorizon,
  }
}

export function getMoonTrackState(config: MoonTrackConfig): MoonTrackState {
  const nightArcProgress = Math.min(
    1,
    Math.max(0, config.phaseState.hoursSinceRise / 12)
  )
  const arc = 1 - Math.pow(nightArcProgress * 2 - 1, 2)
  const xPercent =
    config.leftPercent +
    nightArcProgress * (config.rightPercent - config.leftPercent)
  const yPercent = config.horizonYPercent - arc * config.arcHeightPercent
  const visibilityScale = config.isDaylight ? config.daylightVisibilityScale : 1
  const illuminationFactor = config.phaseState.illumination * visibilityScale
  const isVisible =
    config.phaseState.isAboveHorizon &&
    illuminationFactor > config.visibilityThreshold

  return {
    xPercent,
    yPercent,
    opacity: isVisible ? visibilityScale : 0,
    isVisible,
  }
}

export function getSunTrackState(config: SunTrackConfig): SunTrackState {
  const normalizedHour = normalizeHour(config.hour)
  const hasDaylight = config.sunsetHour > config.sunriseHour
  const daylightHours = Math.max(1e-6, config.sunsetHour - config.sunriseHour)
  const clampedHour = hasDaylight
    ? Math.min(config.sunsetHour, Math.max(config.sunriseHour, normalizedHour))
    : config.sunriseHour
  const progress = hasDaylight
    ? (clampedHour - config.sunriseHour) / daylightHours
    : 0.5
  const arc = 1 - Math.pow(progress * 2 - 1, 2)

  return {
    xPercent:
      config.leftPercent +
      progress * (config.rightPercent - config.leftPercent),
    yPercent: config.horizonYPercent - arc * config.arcHeightPercent,
    isDaylight:
      hasDaylight &&
      normalizedHour >= config.sunriseHour &&
      normalizedHour <= config.sunsetHour,
  }
}

const SUN_DAYLIGHT_SOFTENING_EXPONENT = 0.7
const SUN_DAYLIGHT_FLOOR = 0.4

function dayOfYearFromCalendar(month: number, day: number) {
  const clampedMonth = Math.min(MONTHS_PER_YEAR, Math.max(1, Math.floor(month)))
  const clampedDay = Math.min(DAYS_PER_MONTH, Math.max(1, Math.floor(day)))
  return (clampedMonth - 1) * DAYS_PER_MONTH + clampedDay
}

export function getSolarDaylightWindow(
  config: SolarDaylightWindowConfig
): SolarDaylightWindow {
  const dayOfYear = dayOfYearFromCalendar(config.month, config.day)
  const latitudeRad = (config.latitudeDeg * Math.PI) / 180
  const axialTiltDeg = config.axialTiltDeg ?? SUN_AXIAL_TILT_DEG
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

export function computeSunLightSnapshot(
  gameHour: number,
  calendarDate: CalendarDate
): SunLightSnapshot {
  const normalizedGameHour = normalizeHour(gameHour)
  const dayOfYear = dayOfYearFromCalendar(calendarDate.month, calendarDate.day)
  const latitudeRad = (SUN_LATITUDE_DEG * Math.PI) / 180
  const latitudeCos = Math.cos(latitudeRad)
  const declination = getDeclinationRadFromDayIndex(
    dayOfYear,
    SUN_AXIAL_TILT_DEG
  )
  const direction = getCelestialDirectionFromHourAndDeclination(
    normalizedGameHour,
    12,
    SUN_LATITUDE_DEG,
    declination
  )

  const baseDaylightFactor = Math.min(
    1,
    Math.max(0, direction.y / Math.max(latitudeCos, 1e-6))
  )
  const softenedDaylightFactor = Math.pow(
    baseDaylightFactor,
    SUN_DAYLIGHT_SOFTENING_EXPONENT
  )
  const daylightFactor =
    direction.y > 0
      ? SUN_DAYLIGHT_FLOOR + (1 - SUN_DAYLIGHT_FLOOR) * softenedDaylightFactor
      : 0

  return {
    gameHour: normalizedGameHour,
    direction,
    positionOffset: {
      x: direction.x * SUN_LIGHT_DISTANCE,
      y: direction.y * SUN_LIGHT_DISTANCE,
      z: direction.z * SUN_LIGHT_DISTANCE,
    },
    intensity: SUN_MAX_INTENSITY * daylightFactor,
  }
}

export function getMoonDirection(
  phaseState: MoonPhaseState,
  latitudeDeg: number,
  dayIndex: number,
  axialTiltDeg = MOON_AXIAL_TILT_DEG
): { x: number; y: number; z: number } {
  const declination = getDeclinationRadFromDayIndex(dayIndex, axialTiltDeg)
  return getCelestialDirectionFromHourAndDeclination(
    phaseState.normalizedHour,
    phaseState.transitHour,
    latitudeDeg,
    declination
  )
}

interface MoonLightSample {
  direction: { x: number; y: number; z: number }
  intensity: number
}

interface MoonLightCandidate {
  phaseState: MoonPhaseState
  maxIntensity: number
}

function getMoonDirectionalIntensity(
  phaseState: MoonPhaseState,
  maxIntensity: number
) {
  if (
    !phaseState.isAboveHorizon ||
    phaseState.illumination <= MOON_VISIBILITY_THRESHOLD
  ) {
    return 0
  }

  const softenedMoonFactor =
    MOON_LIGHT_FLOOR +
    (1 - MOON_LIGHT_FLOOR) *
      Math.pow(phaseState.illumination, MOON_ILLUMINATION_SOFTENING_EXPONENT)

  return softenedMoonFactor * maxIntensity
}

function getMoonLightSamples(
  dayIndex: number,
  gameHour: number
): MoonLightSample[] {
  const seasonalDayIndex = dayIndex + normalizeHour(gameHour) / HOURS_PER_DAY
  const candidates: MoonLightCandidate[] = [
    {
      phaseState: getMoonPhaseState(ELDER_MOON_DEFINITION, dayIndex, gameHour),
      maxIntensity: ELDER_MOON_MAX_INTENSITY,
    },
    {
      phaseState: getMoonPhaseState(SWIFT_MOON_DEFINITION, dayIndex, gameHour),
      maxIntensity: SWIFT_MOON_MAX_INTENSITY,
    },
  ]

  const samples: MoonLightSample[] = []
  for (const candidate of candidates) {
    const intensity = getMoonDirectionalIntensity(
      candidate.phaseState,
      candidate.maxIntensity
    )
    if (intensity <= 0) continue

    samples.push({
      direction: getMoonDirection(
        candidate.phaseState,
        SUN_LATITUDE_DEG,
        seasonalDayIndex
      ),
      intensity,
    })
  }

  return samples
}

export function computeCelestialDirectionalLightState(
  sunLightState: SunLightSnapshot,
  dayIndex: number
): CelestialDirectionalLightState {
  const sunAboveHorizon =
    sunLightState.direction.y > 0 && sunLightState.intensity > 0
  const ambientNightFactor = sunLightState.direction.y <= 0 ? 1 : 0

  if (!sunAboveHorizon) {
    const moonLightSamples = getMoonLightSamples(
      dayIndex,
      sunLightState.gameHour
    )
    if (moonLightSamples.length > 0) {
      let brightestSample = moonLightSamples[0]

      for (const sample of moonLightSamples) {
        if (sample.intensity > brightestSample.intensity) {
          brightestSample = sample
        }
      }

      return {
        useMoonLight: true,
        positionOffset: {
          x: brightestSample.direction.x * SUN_LIGHT_DISTANCE,
          y: brightestSample.direction.y * SUN_LIGHT_DISTANCE,
          z: brightestSample.direction.z * SUN_LIGHT_DISTANCE,
        },
        intensity: brightestSample.intensity,
        ambientNightFactor,
        sunColorBlendFactor: 0,
      }
    }
  }

  const twilightFactor = sunAboveHorizon
    ? Math.min(
        1,
        Math.max(
          0,
          (SUN_TWILIGHT_ELEVATION_THRESHOLD - sunLightState.direction.y) /
            SUN_TWILIGHT_ELEVATION_THRESHOLD
        )
      )
    : 0

  return {
    useMoonLight: false,
    positionOffset: sunLightState.positionOffset,
    intensity: sunLightState.intensity,
    ambientNightFactor,
    sunColorBlendFactor: twilightFactor * SUN_TWILIGHT_COLOR_BLEND,
  }
}

export function computeCelestialLightState(
  sunLightState: SunLightSnapshot,
  calendarDate: CalendarDate,
  ambientDayIntensity: number,
  ambientNightIntensity: number
): CelestialLightState {
  const dayIndex = getGameCalendarDayIndex(calendarDate)
  const directional = computeCelestialDirectionalLightState(
    sunLightState,
    dayIndex
  )
  const ambientNightFactor = directional.ambientNightFactor
  const ambientIntensity =
    ambientDayIntensity +
    (ambientNightIntensity - ambientDayIntensity) * ambientNightFactor

  return {
    directional,
    ambientNightFactor,
    ambientIntensity,
  }
}

function toMoonPhaseAngleRad(illumination: number, isWaxing: boolean) {
  const clamped = Math.min(1, Math.max(0, illumination))
  const baseAngle = Math.acos(1 - 2 * clamped)
  return isWaxing ? baseAngle : 2 * Math.PI - baseAngle
}

export function drawMoonToCanvas(
  node: HTMLCanvasElement,
  params: MoonCanvasParams
) {
  const pixelRatio = globalThis.devicePixelRatio ?? 1
  const renderSize = Math.max(24, Math.round(params.sizePx * pixelRatio))
  if (node.width !== renderSize || node.height !== renderSize) {
    node.width = renderSize
    node.height = renderSize
  }

  const context = node.getContext('2d')
  if (!context) return

  const imageData = context.createImageData(renderSize, renderSize)
  const pixels = imageData.data
  const radius = renderSize * 0.5 - 0.5
  const center = renderSize * 0.5
  const phaseAngle = toMoonPhaseAngleRad(params.illumination, params.isWaxing)
  const sunX = Math.sin(phaseAngle)
  const sunZ = -Math.cos(phaseAngle)

  for (let py = 0; py < renderSize; py += 1) {
    for (let px = 0; px < renderSize; px += 1) {
      const nx = (px + 0.5 - center) / radius
      const ny = (py + 0.5 - center) / radius
      const radiusSquared = nx * nx + ny * ny
      const pixelIndex = (py * renderSize + px) * 4

      if (radiusSquared > 1) {
        pixels[pixelIndex + 3] = 0
        continue
      }

      const nz = Math.sqrt(1 - radiusSquared)
      const lightDot = nx * sunX + nz * sunZ
      const distanceFromEdge = Math.sqrt(radiusSquared)
      const edgeAlpha = Math.min(1, Math.max(0, (1 - distanceFromEdge) / 0.05))

      let red = 0
      let green = 0
      let blue = 0
      let alpha = 0

      if (lightDot > 0) {
        const litBase = params.isDaylight ? 220 : MOON_LIT_BASE
        const litRange = params.isDaylight ? 35 : MOON_LIT_RANGE
        const shade = MOON_LIT_SHADE_MIN + (1 - MOON_LIT_SHADE_MIN) * lightDot
        const base = Math.round(litBase + shade * litRange)
        red = Math.min(255, base + MOON_LIT_R_OFFSET)
        green = Math.min(255, base + MOON_LIT_G_OFFSET)
        blue = Math.min(255, base + MOON_LIT_B_OFFSET)
        alpha = Math.round(MOON_LIT_ALPHA * edgeAlpha)
      } else {
        const darkBase = params.isDaylight ? 150 : MOON_DARK_BASE
        const darkRange = params.isDaylight ? 40 : MOON_DARK_RANGE
        const shade = MOON_DARK_SHADE_MIN + (1 - MOON_DARK_SHADE_MIN) * nz
        const base = Math.round(darkBase + shade * darkRange)
        red = base + MOON_DARK_R_OFFSET
        green = base + MOON_DARK_G_OFFSET
        blue = base + MOON_DARK_B_OFFSET
        alpha = MOON_DARK_ALPHA
      }

      pixels[pixelIndex] = red
      pixels[pixelIndex + 1] = green
      pixels[pixelIndex + 2] = blue
      pixels[pixelIndex + 3] = alpha
    }
  }

  context.clearRect(0, 0, renderSize, renderSize)
  context.putImageData(imageData, 0, 0)

  context.beginPath()
  context.arc(center, center, radius - 0.5, 0, 2 * Math.PI)
  context.strokeStyle = 'rgba(220, 230, 255, 0.24)'
  context.lineWidth = Math.max(1, renderSize * 0.04)
  context.stroke()
}

export function moonPhaseCanvasAction(
  node: HTMLCanvasElement,
  params: MoonCanvasParams
) {
  let lastSignature = ''

  const render = (next: MoonCanvasParams) => {
    const signature = `${next.sizePx}:${next.isWaxing ? 1 : 0}:${next.illumination.toFixed(4)}:${next.isDaylight ? 1 : 0}`
    if (signature === lastSignature) return
    lastSignature = signature
    drawMoonToCanvas(node, next)
  }

  render(params)

  return {
    update(next: MoonCanvasParams) {
      render(next)
    },
  }
}
