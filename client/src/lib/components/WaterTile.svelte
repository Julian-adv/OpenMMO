<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { NodeMaterial } from 'three/webgpu'
  import { createWaterMaterial, type WaterMaterialResult } from '../shaders/water-material'

  interface Props {
    geometry: THREE.BufferGeometry
    position?: [number, number, number]
    heightmapTexture: THREE.DataTexture
    normalMap: THREE.Texture
    foamMap: THREE.Texture
    surfaceMap: THREE.Texture
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
    cameraDirection?: THREE.Vector3 | null
    refractionMap?: THREE.Texture | null
  }

  let {
    geometry,
    position = [0, 0, 0],
    heightmapTexture,
    normalMap,
    foamMap,
    surfaceMap,
    time = 0,
    sunDirection = null,
    sunColor = null,
    cameraDirection = null,
    refractionMap = null,
  }: Props = $props()

  let material = $state<NodeMaterial | null>(null)
  let waterResult = $state<WaterMaterialResult | null>(null)

  // Create/recreate material when heightmapTexture or normalMap change
  $effect(() => {
    const hm = heightmapTexture
    const nm = normalMap
    if (!hm || !nm) return

    const fm = foamMap
    const sm = surfaceMap
    if (!fm || !sm) return
    const result = createWaterMaterial({
      heightmapTexture: hm,
      normalMap: nm,
      foamMap: fm,
      surfaceMap: sm,
      refractionMap,
    })
    waterResult = result
    material = result.material

    return () => {
      result.material.dispose()
    }
  })

  // Update time uniform every frame
  $effect(() => {
    if (waterResult) waterResult.uniforms.uTime.value = time
  })

  // Update sun uniforms
  $effect(() => {
    if (!waterResult) return
    if (sunDirection) waterResult.uniforms.uSunDirection.value.copy(sunDirection)
    if (sunColor) waterResult.uniforms.uSunColor.value.copy(sunColor)
    if (cameraDirection) waterResult.uniforms.uCameraDirection.value.copy(cameraDirection)
  })

  // Update refraction map when it changes
  $effect(() => {
    if (waterResult && refractionMap) {
      waterResult.uniforms.uRefractionMap.value = refractionMap
    }
  })

  // Position Y slightly above terrain to avoid z-fighting
  const waterPosition: [number, number, number] = $derived([position[0], 0.01, position[2]])
</script>

{#if material}
  <T.Mesh
    {geometry}
    {material}
    position={waterPosition}
    receiveShadow={false}
    castShadow={false}
  />
{/if}
