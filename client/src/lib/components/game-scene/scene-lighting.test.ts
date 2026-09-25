import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { CSMShadowNode } from 'three/addons/csm/CSMShadowNode.js'
import {
  computeSunLightSnapshot,
  SUN_LIGHT_DISTANCE,
  SUN_MAX_INTENSITY,
} from '../../utils/celestialSimulation'
import { setupCsmShadow } from './renderer-quality'
import {
  createSceneLightingController,
  type SceneLightingUpdateParams,
} from './scene-lighting'

function createLighting() {
  const light = new THREE.DirectionalLight()
  const date = { year: 217, month: 4, day: 1 }
  const params: SceneLightingUpdateParams = {
    currentPlayerPosition: new THREE.Vector3(),
    localCalendarDate: date,
    ambientLight: new THREE.AmbientLight(),
    directionalLight: light,
    directionalShadowsEnabled: true,
    scene: new THREE.Scene(),
    sunLightSnapshot: computeSunLightSnapshot(12, date),
    eclipseFactor: 0,
    cloudFactor: 0,
    rainIntensity: 0,
  }
  return { controller: createSceneLightingController(), light, params }
}

function initializeCascades(light: THREE.DirectionalLight) {
  setupCsmShadow(light)
  const csm = light.shadow.shadowNode as CSMShadowNode
  csm.setup({
    camera: new THREE.PerspectiveCamera(60, 1, 0.1, 500),
    renderer: {
      coordinateSystem: THREE.WebGPUCoordinateSystem,
      reversedDepthBuffer: false,
    },
  } as unknown as Parameters<CSMShadowNode['setup']>[0])
  expect(csm.lights).toHaveLength(2)
  return csm
}

describe('lightning', () => {
  it.each([0, 6, 12, 18])(
    'flashes at maximum sunlight and restores the weather lighting at hour %s',
    (hour) => {
      const { controller, light, params } = createLighting()
      params.sunLightSnapshot = computeSunLightSnapshot(
        hour,
        params.localCalendarDate
      )
      params.cloudFactor = 1
      params.rainIntensity = 1
      params.currentPlayerPosition = new THREE.Vector3(20, 8, -30)
      params.lightningDirection = new THREE.Vector3(-1, 1, 1).normalize()
      controller.update(params)
      const intensity = light.intensity
      const color = light.color.clone()
      const position = light.position.clone()
      const castsShadow = light.castShadow
      const ambientIntensity = params.ambientLight!.intensity
      const environmentIntensity = params.scene.environmentIntensity

      controller.update({ ...params, lightningStrength: 1 })
      expect(light.intensity).toBe(SUN_MAX_INTENSITY)
      expect(light.intensity).toBeGreaterThan(intensity)
      expect(light.color.getHex()).toBe(0xffffff)
      expect(light.position.y).toBeGreaterThan(light.target.position.y)
      const flashDirection = light.position.clone().sub(light.target.position)
      expect(flashDirection.length()).toBeCloseTo(SUN_LIGHT_DISTANCE)
      expect(
        flashDirection.normalize().distanceTo(params.lightningDirection)
      ).toBeLessThan(0.000001)
      expect(light.castShadow).toBe(castsShadow)
      expect(params.ambientLight!.intensity).toBe(ambientIntensity)
      expect(params.scene.environmentIntensity).toBe(environmentIntensity)

      controller.update({ ...params, lightningStrength: 0.35 })
      expect(light.intensity).toBeGreaterThan(intensity)
      expect(light.intensity).toBeLessThan(SUN_MAX_INTENSITY)
      const fadingDistance = light.position.distanceTo(position)
      controller.update({ ...params, lightningStrength: 0.01 })
      if (intensity > 0) {
        expect(light.position.distanceTo(position)).toBeLessThan(fadingDistance)
      } else {
        expect(light.position.distanceTo(position)).toBeCloseTo(fadingDistance)
      }

      controller.update(params)
      expect(light.intensity).toBe(intensity)
      expect(light.color.equals(color)).toBe(true)
      expect(light.position.equals(position)).toBe(true)
    }
  )

  it('never lights the dungeon during a flash', () => {
    const { controller, light, params } = createLighting()
    controller.update({ ...params, lightningStrength: 1, underground: true })
    expect(light.intensity).toBe(0)
    expect(light.castShadow).toBe(false)
  })
})

describe('rain shadows', () => {
  it('fades every cascade in light rain and restores clear-weather shadows', () => {
    const { controller, light, params } = createLighting()
    const csm = initializeCascades(light)
    controller.update(params)
    const clearLightIntensity = light.intensity
    expect(light.shadow.intensity).toBe(1)

    let previous = light.shadow.intensity
    for (const rain of [0.01, 0.05, 0.1, 0.2]) {
      params.rainIntensity = rain
      controller.update(params)
      expect(light.shadow.intensity).toBeLessThan(previous)
      expect(light.castShadow).toBe(true)
      expect(light.intensity).toBe(clearLightIntensity)
      for (const cascade of csm.lights) {
        expect(cascade.shadow?.intensity).toBe(light.shadow.intensity)
      }
      previous = light.shadow.intensity
    }

    expect(light.shadow.intensity).toBeGreaterThan(0)
    expect(light.shadow.intensity).toBeLessThanOrEqual(0.05)
    params.rainIntensity = 1
    controller.update(params)
    expect(light.shadow.intensity).toBe(previous)

    params.rainIntensity = 0
    controller.update(params)
    expect(light.shadow.intensity).toBe(1)
    for (const cascade of csm.lights) {
      expect(cascade.shadow?.intensity).toBe(1)
    }
  })

  it('starts with faint shadows when cascades initialize during rain', () => {
    const { controller, light, params } = createLighting()
    controller.update({ ...params, rainIntensity: 0.2 })
    const csm = initializeCascades(light)
    for (const cascade of csm.lights) {
      expect(cascade.shadow?.intensity).toBeLessThanOrEqual(0.05)
    }
  })

  it.each(['night', 'underground', 'disabled'] as const)(
    'preserves shadow suppression when %s and restores daytime shadows',
    (mode) => {
      const { controller, light, params } = createLighting()
      controller.update({ ...params, rainIntensity: 0.2 })
      controller.update({
        ...params,
        underground: mode === 'underground',
        directionalShadowsEnabled: mode !== 'disabled',
        sunLightSnapshot: computeSunLightSnapshot(
          mode === 'night' ? 0 : 12,
          params.localCalendarDate
        ),
      })
      expect(light.castShadow).toBe(false)
      controller.update(params)
      expect(light.castShadow).toBe(true)
      expect(light.shadow.intensity).toBe(1)
    }
  )
})
