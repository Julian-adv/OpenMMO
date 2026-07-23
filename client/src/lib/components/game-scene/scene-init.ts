import * as THREE from 'three'
import { PMREMGenerator, type WebGPURenderer } from 'three/webgpu'
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
import { RefractionRenderManager } from '../../managers/refractionRenderManager'
import { ReflectionRenderManager } from '../../managers/reflectionRenderManager'
import { loadFoamTexture } from '../../shaders/water-foam-gen'
import { loadCausticsTexture } from '../../shaders/caustics-gen'
import {
  TERRAIN_TILE_SEGMENTS,
  TERRAIN_TILE_SIZE,
  createTerrainGeometry,
} from './terrain-utils'

export interface SceneInitResult {
  terrainGeometry: THREE.BufferGeometry
  waterNormalMap: THREE.Texture | null
  waterFoamMapPromise: Promise<THREE.Texture | null>
  waterCausticsMapPromise: Promise<THREE.Texture | null>
  refractionManager: RefractionRenderManager | null
  refractionTexture: THREE.Texture | null
  reflectionManager: ReflectionRenderManager | null
  reflectionTexture: THREE.Texture | null
}

export function initScene(
  renderer: WebGPURenderer,
  scene: THREE.Scene,
  viewportWidth: number,
  viewportHeight: number,
  options: {
    skipWaterLayer?: boolean
    skipWaterEffects?: boolean
    waterRtDivisor?: number
  } = {}
): SceneInitResult {
  // Create terrain geometry
  const terrainGeometry = createTerrainGeometry(
    TERRAIN_TILE_SIZE,
    TERRAIN_TILE_SEGMENTS
  )

  // Shared by both skip paths: no refraction/reflection render targets.
  const noWaterEffects = {
    refractionManager: null,
    refractionTexture: null,
    reflectionManager: null,
    reflectionTexture: null,
  }

  if (options.skipWaterLayer) {
    return {
      terrainGeometry,
      waterNormalMap: null,
      waterFoamMapPromise: Promise.resolve(null),
      waterCausticsMapPromise: Promise.resolve(null),
      ...noWaterEffects,
    }
  }

  // Load water textures
  const loader = new THREE.TextureLoader()
  const waterNormalMap = loader.load('/textures/waternormals.jpg')
  waterNormalMap.wrapS = waterNormalMap.wrapT = THREE.RepeatWrapping

  const waterFoamMapPromise = loadFoamTexture()
  const waterCausticsMapPromise = loadCausticsTexture()

  // On mobile, keep the water surface but skip the environment map and
  // refraction/reflection render targets to avoid a large GPU allocation spike.
  if (options.skipWaterEffects) {
    return {
      terrainGeometry,
      waterNormalMap,
      waterFoamMapPromise,
      waterCausticsMapPromise,
      ...noWaterEffects,
    }
  }

  // Generate environment map
  renderer.init().then(() => {
    const pmremGenerator = new PMREMGenerator(renderer)
    const rt = pmremGenerator.fromScene(new RoomEnvironment())
    scene.environment = rt.texture
    scene.environmentIntensity = 0.5
    pmremGenerator.dispose()
  })

  // Initialize render managers
  const refractionManager = new RefractionRenderManager(
    renderer,
    scene,
    viewportWidth,
    viewportHeight,
    options.waterRtDivisor
  )
  const reflectionManager = new ReflectionRenderManager(
    renderer,
    scene,
    viewportWidth,
    viewportHeight,
    options.waterRtDivisor
  )

  return {
    terrainGeometry,
    waterNormalMap,
    waterFoamMapPromise,
    waterCausticsMapPromise,
    refractionManager,
    refractionTexture: refractionManager.texture,
    reflectionManager,
    reflectionTexture: reflectionManager.texture,
  }
}
