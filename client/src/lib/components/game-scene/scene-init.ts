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
  waterNormalMap: THREE.Texture
  waterFoamMapPromise: Promise<THREE.Texture>
  waterCausticsMapPromise: Promise<THREE.Texture>
  refractionManager: RefractionRenderManager
  refractionTexture: THREE.Texture
  reflectionManager: ReflectionRenderManager
  reflectionTexture: THREE.Texture
}

export function initScene(
  renderer: WebGPURenderer,
  scene: THREE.Scene,
  viewportWidth: number,
  viewportHeight: number
): SceneInitResult {
  // Generate environment map
  renderer.init().then(() => {
    const pmremGenerator = new PMREMGenerator(renderer)
    const rt = pmremGenerator.fromScene(new RoomEnvironment())
    scene.environment = rt.texture
    scene.environmentIntensity = 0.5
    pmremGenerator.dispose()
  })

  // Create terrain geometry
  const terrainGeometry = createTerrainGeometry(
    TERRAIN_TILE_SIZE,
    TERRAIN_TILE_SEGMENTS
  )

  // Load water textures
  const loader = new THREE.TextureLoader()
  const waterNormalMap = loader.load('/textures/waternormals.jpg')
  waterNormalMap.wrapS = waterNormalMap.wrapT = THREE.RepeatWrapping

  const waterFoamMapPromise = loadFoamTexture()
  const waterCausticsMapPromise = loadCausticsTexture()

  // Initialize render managers
  const refractionManager = new RefractionRenderManager(
    renderer,
    scene,
    viewportWidth,
    viewportHeight
  )
  const reflectionManager = new ReflectionRenderManager(
    renderer,
    scene,
    viewportWidth,
    viewportHeight
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
