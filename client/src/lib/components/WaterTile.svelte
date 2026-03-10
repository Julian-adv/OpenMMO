<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { WaterMaterialResult } from '../shaders/water-material'
  import { tick } from 'svelte'

  interface Props {
    geometry: THREE.BufferGeometry
    position?: [number, number, number]
    heightmapTexture: THREE.DataTexture
    waterResult: WaterMaterialResult
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
    cameraDirection?: THREE.Vector3 | null
    moonBrightness?: number
    refractionMap?: THREE.Texture | null
    reflectionMap?: THREE.Texture | null
  }

  let {
    geometry,
    position = [0, 0, 0],
    heightmapTexture,
    waterResult,
    time = 0,
    sunDirection = null,
    sunColor = null,
    cameraDirection = null,
    moonBrightness = 0,
    refractionMap = null,
    reflectionMap = null,
  }: Props = $props()

  let meshRef = $state<THREE.Mesh | undefined>(undefined)

  // Set up onBeforeRender for per-frame uniform updates once mesh is ready
  $effect(() => {
    if (!meshRef) return
    const mesh = meshRef
    tick().then(() => {
      mesh.onBeforeRender = () => {
        const u = waterResult.uniforms
        u.uHeightmapTexture.value = heightmapTexture
        u.uTime.value = time
        waterResult.updateWaveDirections(time)
        if (sunDirection) u.uSunDirection.value.copy(sunDirection)
        if (sunColor) u.uSunColor.value.copy(sunColor)
        if (cameraDirection) u.uCameraDirection.value.copy(cameraDirection)
        u.uMoonBrightness.value = moonBrightness
        if (refractionMap) u.uRefractionMap.value = refractionMap
        if (reflectionMap) u.uReflectionMap.value = reflectionMap
      }
    })
  })

  // Position Y slightly above terrain to avoid z-fighting
  const waterPosition: [number, number, number] = $derived([position[0], 0.01, position[2]])
</script>

<T.Mesh
  bind:ref={meshRef}
  {geometry}
  material={waterResult.material}
  position={waterPosition}
  receiveShadow={false}
  castShadow={false}
/>
