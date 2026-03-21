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
export const AMBIENT_NIGHT_INTENSITY = 0.3

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
  scene: THREE.Scene
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

  // Quantize shadow light direction to prevent shadow map flickering.
  // Tiny per-frame direction changes rotate the shadow texel grid, causing
  // boundary pixels to oscillate. Snapping to discrete angular steps keeps
  // the grid stable between updates. Comparison uses squared values to
  // avoid per-frame sqrt calls.
  const SHADOW_DIR_SNAP_SQ = 0.0005 * 0.0005
  let snappedOffset: { x: number; y: number; z: number } | null = null

  function snapShadowDirection(offset: Vector3Like): Vector3Like {
    if (snappedOffset !== null) {
      const dx = offset.x - snappedOffset.x
      const dy = offset.y - snappedOffset.y
      const dz = offset.z - snappedOffset.z
      const lenSq =
        offset.x * offset.x + offset.y * offset.y + offset.z * offset.z
      if (
        lenSq === 0 ||
        dx * dx + dy * dy + dz * dz < SHADOW_DIR_SNAP_SQ * lenSq
      ) {
        return snappedOffset
      }
    }
    snappedOffset = snappedOffset ?? { x: 0, y: 0, z: 0 }
    snappedOffset.x = offset.x
    snappedOffset.y = offset.y
    snappedOffset.z = offset.z
    return snappedOffset
  }

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

    // Scale IBL environment intensity with day/night cycle
    const envDayIntensity = 0.5
    const envNightIntensity = 0.03
    params.scene.environmentIntensity =
      envDayIntensity +
      (envNightIntensity - envDayIntensity) *
        celestialLightState.ambientNightFactor

    if (!params.directionalLight) return

    const directionalLightState = celestialLightState.directional
    const playerPos = params.currentPlayerPosition

    const shadowOffset = snapShadowDirection(
      directionalLightState.positionOffset
    )
    params.directionalLight.position.set(
      playerPos.x + shadowOffset.x,
      playerPos.y + shadowOffset.y,
      playerPos.z + shadowOffset.z
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
