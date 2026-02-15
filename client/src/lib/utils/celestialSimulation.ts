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
export const SUN_TWILIGHT_ELEVATION_THRESHOLD = 0.22
export const SUN_TWILIGHT_COLOR_BLEND = 0.65

export const GAME_START_YEAR = 217
export const GAME_MONTHS_PER_YEAR = 12
export const GAME_DAYS_PER_MONTH = 30

export const SUN_DAY_COLOR_HEX = '#ffffff'
export const SUN_TWILIGHT_COLOR_HEX = '#ff9b86'
export const MOON_LIGHT_COLOR_HEX = '#d6e2ff'
export const MOON_VISIBILITY_THRESHOLD = 0.02
export const ELDER_MOON_MAX_INTENSITY = 0.6
export const SWIFT_MOON_MAX_INTENSITY = 0.45
export const MOON_ILLUMINATION_SOFTENING_EXPONENT = 0.7
export const MOON_LIGHT_FLOOR = 0.3

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
  sunsetWindowHours: number
}

export interface SunTrackState {
  xPercent: number
  yPercent: number
  isDaylight: boolean
  isSunsetWindow: boolean
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
}

export interface SunLightSnapshot {
  gameHour: number
  direction: { x: number; y: number; z: number }
  positionOffset: { x: number; y: number; z: number }
  intensity: number
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
  const cycleDay =
    positiveModulo(absoluteDayIndex + moon.phaseOffsetDays, moon.periodDays) + 1
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
  const normalizedHour = normalizeHour(gameHour)
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
  const opacity = Math.min(
    1,
    Math.max(0, config.phaseState.illumination * visibilityScale)
  )
  const isVisible =
    config.phaseState.isAboveHorizon && opacity > config.visibilityThreshold

  return {
    xPercent,
    yPercent,
    opacity,
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
    isSunsetWindow:
      hasDaylight &&
      (Math.abs(normalizedHour - config.sunriseHour) <=
        config.sunsetWindowHours ||
        Math.abs(normalizedHour - config.sunsetHour) <=
          config.sunsetWindowHours),
  }
}

export function getMoonDirection(
  phaseState: MoonPhaseState,
  latitudeDeg: number
): { x: number; y: number; z: number } {
  const latitudeRad = (latitudeDeg * Math.PI) / 180
  const latitudeCos = Math.cos(latitudeRad)
  const latitudeSin = Math.sin(latitudeRad)
  const hourAngle =
    (2 * Math.PI * (phaseState.normalizedHour - phaseState.transitHour)) /
    HOURS_PER_DAY
  const east = -Math.sin(hourAngle)
  const north = -latitudeSin * Math.cos(hourAngle)
  const up = latitudeCos * Math.cos(hourAngle)

  return {
    x: east,
    y: up,
    z: -north,
  }
}

interface SelectedMoonLight {
  phaseState: MoonPhaseState
  maxIntensity: number
}

function getSelectedMoonLight(
  dayIndex: number,
  gameHour: number
): SelectedMoonLight | null {
  const elderMoonPhaseState = getMoonPhaseState(
    ELDER_MOON_DEFINITION,
    dayIndex,
    gameHour
  )
  const swiftMoonPhaseState = getMoonPhaseState(
    SWIFT_MOON_DEFINITION,
    dayIndex,
    gameHour
  )
  const elderMoonVisible =
    elderMoonPhaseState.isAboveHorizon &&
    elderMoonPhaseState.illumination > MOON_VISIBILITY_THRESHOLD
  if (elderMoonVisible) {
    return {
      phaseState: elderMoonPhaseState,
      maxIntensity: ELDER_MOON_MAX_INTENSITY,
    }
  }

  const swiftMoonVisible =
    swiftMoonPhaseState.isAboveHorizon &&
    swiftMoonPhaseState.illumination > MOON_VISIBILITY_THRESHOLD
  if (swiftMoonVisible) {
    return {
      phaseState: swiftMoonPhaseState,
      maxIntensity: SWIFT_MOON_MAX_INTENSITY,
    }
  }

  return null
}

export function computeCelestialDirectionalLightState(
  sunLightState: SunLightSnapshot,
  dayIndex: number
): CelestialDirectionalLightState {
  const sunAboveHorizon =
    sunLightState.direction.y > 0 && sunLightState.intensity > 0
  const ambientNightFactor = sunLightState.direction.y <= 0 ? 1 : 0

  if (!sunAboveHorizon) {
    const selectedMoonLight = getSelectedMoonLight(
      dayIndex,
      sunLightState.gameHour
    )
    if (selectedMoonLight) {
      const moonDirection = getMoonDirection(
        selectedMoonLight.phaseState,
        SUN_LATITUDE_DEG
      )
      const softenedMoonFactor =
        MOON_LIGHT_FLOOR +
        (1 - MOON_LIGHT_FLOOR) *
          Math.pow(
            selectedMoonLight.phaseState.illumination,
            MOON_ILLUMINATION_SOFTENING_EXPONENT
          )
      return {
        useMoonLight: true,
        positionOffset: {
          x: moonDirection.x * SUN_LIGHT_DISTANCE,
          y: moonDirection.y * SUN_LIGHT_DISTANCE,
          z: moonDirection.z * SUN_LIGHT_DISTANCE,
        },
        intensity: softenedMoonFactor * selectedMoonLight.maxIntensity,
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
        const shade = 0.75 + 0.25 * lightDot
        const base = Math.round(188 + shade * 62)
        red = base - 8
        green = base - 3
        blue = base + 6
        alpha = Math.round(255 * edgeAlpha)
      } else {
        const shade = 0.16 + 0.12 * nz
        const base = Math.round(12 + shade * 42)
        red = base
        green = base + 2
        blue = base + 8
        alpha = Math.round(228 * edgeAlpha)
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
    const signature = `${next.sizePx}:${next.isWaxing ? 1 : 0}:${next.illumination.toFixed(4)}`
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
