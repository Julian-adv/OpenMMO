export interface SunSimulationConfig {
  latitudeDeg: number
  sunriseHour: number
  dayDurationSeconds: number
  startHour: number
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
  getLightState: () => SunLightState
}

const HOURS_PER_DAY = 24

function normalizeHour(hour: number) {
  return ((hour % HOURS_PER_DAY) + HOURS_PER_DAY) % HOURS_PER_DAY
}

export function createSunLightSimulation(
  config: SunSimulationConfig
): SunLightSimulation {
  const latitudeRad = (config.latitudeDeg * Math.PI) / 180
  const latitudeCos = Math.cos(latitudeRad)
  const latitudeSin = Math.sin(latitudeRad)
  let elapsedSeconds =
    (normalizeHour(config.startHour) / HOURS_PER_DAY) *
    config.dayDurationSeconds

  function getGameHour() {
    return (elapsedSeconds / config.dayDurationSeconds) * HOURS_PER_DAY
  }

  function setGameHour(hour: number) {
    elapsedSeconds =
      (normalizeHour(hour) / HOURS_PER_DAY) * config.dayDurationSeconds
  }

  function getSunDirectionFromHour(hour: number): SunVector {
    const theta = 2 * Math.PI * ((hour - config.sunriseHour) / HOURS_PER_DAY)
    const sinTheta = Math.sin(theta)
    return {
      x: Math.cos(theta),
      y: latitudeCos * sinTheta,
      z: latitudeSin * sinTheta,
    }
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
    const daylightFactor = Math.max(
      0,
      direction.y / Math.max(latitudeCos, 1e-6)
    )

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
    getLightState,
  }
}
