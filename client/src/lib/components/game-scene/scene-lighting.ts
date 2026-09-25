import * as THREE from 'three'
import {
  MOON_LIGHT_COLOR_HEX,
  SUN_DAY_COLOR_HEX,
  SUN_TWILIGHT_COLOR_HEX,
  SUN_MAX_INTENSITY,
  SUN_LIGHT_DISTANCE,
  type CalendarDate,
  type SunLightSnapshot,
  computeCelestialLightState,
} from '../../utils/celestialSimulation'
import { setDirectionalShadowIntensity } from './renderer-quality'

export const AMBIENT_DAY_INTENSITY = 0.35
export const AMBIENT_NIGHT_INTENSITY = 0.3
const RAIN_SHADOW_FADE_END = 0.2
const RAIN_SHADOW_INTENSITY = 0.03

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
  directionalShadowsEnabled: boolean
  scene: THREE.Scene
  sunLightSnapshot: SunLightSnapshot
  eclipseFactor: number
  /** Overcast 0..1 at the player. */
  cloudFactor?: number
  rainIntensity?: number
  lightningStrength?: number
  lightningDirection?: Vector3Like
  /** Dungeon render mode: no sun/moon, dim cold ambient, dark background. */
  underground?: boolean
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
  const lightningColor = new THREE.Color('#ffffff')
  const lightningPosition = new THREE.Vector3()
  const ambientDayColor = new THREE.Color('#fff8f0')
  const ambientTwilightColor = new THREE.Color('#ffb080')
  const ambientNightColor = new THREE.Color('#8ea8ff')
  const ambientColor = new THREE.Color()

  // Snap light direction to stabilize shadow edges.
  const SHADOW_DIR_SNAP_SQ = 0.0005 * 0.0005
  const SUN_SHADOW_ELEVATION_MIN = 0.08
  let snappedOffset: { x: number; y: number; z: number } | null = null
  let lastDirectionalCastShadow: boolean | null = null

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

  const undergroundAmbientColor = new THREE.Color('#8090b0')
  const undergroundBackground = new THREE.Color('#060606')
  let savedBackground: THREE.Scene['background'] = null
  let wasUnderground = false

  /** Use dim ambient lighting underground. */
  function updateUnderground(params: SceneLightingUpdateParams) {
    if (!wasUnderground) {
      savedBackground = params.scene.background
      params.scene.background = undergroundBackground
      wasUnderground = true
    }
    if (params.ambientLight) {
      params.ambientLight.color.copy(undergroundAmbientColor)
      params.ambientLight.intensity = 0.25
    }
    params.scene.environmentIntensity = 0.03
    if (params.directionalLight) {
      params.directionalLight.intensity = 0
      if (lastDirectionalCastShadow !== false) {
        params.directionalLight.castShadow = false
        lastDirectionalCastShadow = false
      }
    }
  }

  function update(params: SceneLightingUpdateParams) {
    if (!params.currentPlayerPosition) return

    if (params.underground) {
      updateUnderground(params)
      return
    }
    if (wasUnderground) {
      params.scene.background = savedBackground
      savedBackground = null
      wasUnderground = false
    }

    const sunLightState = params.sunLightSnapshot
    const celestialLightState = computeCelestialLightState(
      sunLightState,
      params.localCalendarDate,
      AMBIENT_DAY_INTENSITY,
      AMBIENT_NIGHT_INTENSITY
    )

    const eclipse = params.eclipseFactor
    const cloud = params.cloudFactor ?? 0

    if (params.ambientLight) {
      const twilightBlend = celestialLightState.directional.sunColorBlendFactor
      ambientColor
        .copy(ambientDayColor)
        .lerp(ambientTwilightColor, twilightBlend)
        .lerp(ambientNightColor, celestialLightState.ambientNightFactor)

      params.ambientLight.color.copy(ambientColor)
      params.ambientLight.intensity =
        celestialLightState.ambientIntensity *
        (1 - eclipse * 0.5) *
        (1 - cloud * 0.25)
    }

    // Scale IBL environment intensity with day/night cycle
    const envDayIntensity = 0.2
    const envNightIntensity = 0.03
    params.scene.environmentIntensity =
      (envDayIntensity +
        (envNightIntensity - envDayIntensity) *
          celestialLightState.ambientNightFactor) *
      (1 - cloud * 0.25)

    if (!params.directionalLight) return

    const directionalLightState = celestialLightState.directional
    const playerPos = params.currentPlayerPosition
    const lightning = THREE.MathUtils.clamp(params.lightningStrength ?? 0, 0, 1)

    const shadowOffset = snapShadowDirection(
      directionalLightState.positionOffset
    )
    params.directionalLight.position.set(
      playerPos.x + shadowOffset.x,
      playerPos.y + shadowOffset.y,
      playerPos.z + shadowOffset.z
    )
    const baseIntensity =
      directionalLightState.intensity * (1 - eclipse * 0.95) * (1 - cloud * 0.5)
    params.directionalLight.intensity = THREE.MathUtils.lerp(
      baseIntensity,
      SUN_MAX_INTENSITY,
      lightning
    )

    const rainShadowFade = THREE.MathUtils.smoothstep(
      params.rainIntensity ?? 0,
      0,
      RAIN_SHADOW_FADE_END
    )
    setDirectionalShadowIntensity(
      params.directionalLight,
      THREE.MathUtils.lerp(1, RAIN_SHADOW_INTENSITY, rainShadowFade)
    )

    const shouldCastSunShadow =
      params.directionalShadowsEnabled &&
      !directionalLightState.useMoonLight &&
      sunLightState.direction.y >= SUN_SHADOW_ELEVATION_MIN &&
      baseIntensity > 0.1
    if (lastDirectionalCastShadow !== shouldCastSunShadow) {
      params.directionalLight.castShadow = shouldCastSunShadow
      if (shouldCastSunShadow) params.directionalLight.shadow.needsUpdate = true
      lastDirectionalCastShadow = shouldCastSunShadow
    }

    if (directionalLightState.useMoonLight) {
      params.directionalLight.color.copy(moonLightColor)
    } else {
      sunDirectionalColor
        .copy(sunDayColor)
        .lerp(sunTwilightColor, directionalLightState.sunColorBlendFactor)
      params.directionalLight.color.copy(sunDirectionalColor)
    }

    if (lightning > 0) {
      const blend =
        (SUN_MAX_INTENSITY * lightning) / params.directionalLight.intensity
      const direction = params.lightningDirection
      lightningPosition.set(
        playerPos.x + (direction?.x ?? 0) * SUN_LIGHT_DISTANCE,
        playerPos.y + (direction?.y ?? 1) * SUN_LIGHT_DISTANCE,
        playerPos.z + (direction?.z ?? 0) * SUN_LIGHT_DISTANCE
      )
      params.directionalLight.position.lerp(lightningPosition, blend)
      params.directionalLight.color.lerp(lightningColor, blend)
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
