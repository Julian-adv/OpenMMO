<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { NodeMaterial } from 'three/webgpu'
  import { createWaterMaterial } from '../shaders/water-material'
  import { onMount, tick } from 'svelte'

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
  let meshRef = $state<THREE.Mesh | undefined>(undefined)

  onMount(() => {
    const result = createWaterMaterial({
      heightmapTexture,
      normalMap,
      foamMap,
      surfaceMap,
      refractionMap,
    })
    material = result.material

    // After Svelte renders {#if material} → meshRef is set
    tick().then(() => {
      if (!meshRef) return
      meshRef.onBeforeRender = () => {
        const u = result.uniforms
        u.uHeightmapTexture.value = heightmapTexture
        u.uTime.value = time
        if (sunDirection) u.uSunDirection.value.copy(sunDirection)
        if (sunColor) u.uSunColor.value.copy(sunColor)
        if (cameraDirection) u.uCameraDirection.value.copy(cameraDirection)
        if (refractionMap) u.uRefractionMap.value = refractionMap
      }
    })

    return () => {
      result.material.dispose()
    }
  })

  // Position Y slightly above terrain to avoid z-fighting
  const waterPosition: [number, number, number] = $derived([position[0], 0.01, position[2]])
</script>

{#if material}
  <T.Mesh
    bind:ref={meshRef}
    {geometry}
    {material}
    position={waterPosition}
    receiveShadow={false}
    castShadow={false}
  />
{/if}
