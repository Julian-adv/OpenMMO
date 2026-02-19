import * as THREE from 'three'
import {
  MOON_LIGHT_COLOR_HEX,
  SUN_DAY_COLOR_HEX,
  SUN_TWILIGHT_COLOR_HEX,
  type CalendarDate,
  type SunLightSnapshot,
  computeCelestialLightState,
} from '../../utils/celestialSimulation'

export const AMBIENT_DAY_INTENSITY = 0.95
export const AMBIENT_NIGHT_INTENSITY = 2.24

export interface Vector3Like {
  x: number
  y: number
  z: number
}

export interface SceneLightingUpdateParams {
  currentPlayerPosition: Vector3Like | null
  localCalendarDate: CalendarDate
  ambientLight: THREE.AmbientLight | undefined
  directionalLight: THREE.DirectionalLight | undefined
  sunLightSnapshot: SunLightSnapshot
  eclipseFactor: number
}

export interface SceneLightingController {
  ambientDayIntensity: number
  update: (params: SceneLightingUpdateParams) => void
}

export function createSceneLightingController(): SceneLightingController {
  const sunDayColor = new THREE.Color(SUN_DAY_COLOR_HEX)
  const sunTwilightColor = new THREE.Color(SUN_TWILIGHT_COLOR_HEX)
  const sunDirectionalColor = new THREE.Color()
  const moonLightColor = new THREE.Color(MOON_LIGHT_COLOR_HEX)
  const ambientDayColor = new THREE.Color('#ffffff')
  const ambientNightColor = new THREE.Color('#8ea8ff')
  const ambientColor = new THREE.Color()

  function update(params: SceneLightingUpdateParams) {
    if (!params.currentPlayerPosition) return

    const sunLightState = params.sunLightSnapshot
    const celestialLightState = computeCelestialLightState(
      sunLightState,
      params.localCalendarDate,
      AMBIENT_DAY_INTENSITY,
      AMBIENT_NIGHT_INTENSITY
    )

    const eclipse = params.eclipseFactor

    if (params.ambientLight) {
      ambientColor
        .copy(ambientDayColor)
        .lerp(ambientNightColor, celestialLightState.ambientNightFactor)

      params.ambientLight.color.copy(ambientColor)
      params.ambientLight.intensity =
        celestialLightState.ambientIntensity * (1 - eclipse * 0.5)
    }

    if (!params.directionalLight) return

    const directionalLightState = celestialLightState.directional
    const playerPos = params.currentPlayerPosition

    params.directionalLight.position.set(
      playerPos.x + directionalLightState.positionOffset.x,
      playerPos.y + directionalLightState.positionOffset.y,
      playerPos.z + directionalLightState.positionOffset.z
    )
    params.directionalLight.intensity =
      directionalLightState.intensity * (1 - eclipse * 0.95)

    if (directionalLightState.useMoonLight) {
      params.directionalLight.color.copy(moonLightColor)
    } else {
      sunDirectionalColor
        .copy(sunDayColor)
        .lerp(sunTwilightColor, directionalLightState.sunColorBlendFactor)
      params.directionalLight.color.copy(sunDirectionalColor)
    }

    if (params.directionalLight.target) {
      params.directionalLight.target.position.set(
        playerPos.x,
        playerPos.y,
        playerPos.z
      )
      params.directionalLight.target.updateMatrixWorld()
    }
  }

  return {
    ambientDayIntensity: AMBIENT_DAY_INTENSITY,
    update,
  }
}
