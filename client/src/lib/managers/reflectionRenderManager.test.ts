import * as THREE from 'three'
import type { WebGPURenderer } from 'three/webgpu'
import { describe, expect, it, vi } from 'vitest'
import { ReflectionRenderManager } from './reflectionRenderManager'

function setup() {
  const scene = new THREE.Scene()
  scene.background = new THREE.Color(0x123456)
  const terrain = new THREE.Group()
  const water = new THREE.Group()
  const housing = new THREE.Group()
  scene.add(terrain, water, housing)
  const camera = new THREE.OrthographicCamera(-10, 10, 10, -10, 0.1, 1000)
  camera.position.set(5, 25, 15)
  camera.lookAt(0, 10, 0)
  camera.updateMatrixWorld()
  const renderer = {
    hasInitialized: () => true,
    getClearColor: (color: THREE.Color) => color.set(0x345678),
    getClearAlpha: () => 1,
    setClearColor: vi.fn(),
    getRenderTarget: () => null,
    setRenderTarget: vi.fn(),
    render: vi.fn(),
  }
  const manager = new ReflectionRenderManager(
    renderer as unknown as WebGPURenderer,
    scene,
    64,
    64,
    1
  )
  manager.setCamera(camera)
  manager.setTerrainGroup(terrain)
  manager.setWaterGroup(water)
  manager.setHousingGroup(housing)
  return { manager, renderer, scene, terrain, water, housing }
}

describe('planar reflections', () => {
  it('keeps the existing water reflection at sea level by default', () => {
    const { manager, renderer, terrain, water, housing } = setup()
    renderer.render.mockImplementation((_scene, camera: THREE.Camera) => {
      expect(camera.matrixWorld.elements[13]).toBe(-25)
      expect(terrain.visible).toBe(false)
      expect(water.visible).toBe(false)
      expect(housing.visible).toBe(false)
    })
    manager.render()
    expect(renderer.render).toHaveBeenCalledOnce()
    expect(housing.visible).toBe(true)
    manager.dispose()
  })

  it('restores visibility, background and render target after a failed render', () => {
    const { manager, renderer, terrain, water, housing, scene } = setup()
    const background = scene.background
    housing.visible = false
    renderer.render.mockImplementation(() => {
      throw new Error('render failed')
    })
    expect(() => manager.render()).toThrow('render failed')
    expect(terrain.visible).toBe(true)
    expect(water.visible).toBe(true)
    expect(housing.visible).toBe(false)
    expect(scene.background).toBe(background)
    expect(renderer.setRenderTarget).toHaveBeenLastCalledWith(null)
    manager.dispose()
  })
})
