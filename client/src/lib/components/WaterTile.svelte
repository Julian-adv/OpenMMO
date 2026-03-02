<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { createWaterMaterial } from '../shaders/water-material'

  interface Props {
    geometry: THREE.BufferGeometry
    position?: [number, number, number]
    heightmapTexture: THREE.DataTexture
    normalMap: THREE.Texture
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
  }

  let {
    geometry,
    position = [0, 0, 0],
    heightmapTexture,
    normalMap,
    time = 0,
    sunDirection = null,
    sunColor = null,
  }: Props = $props()

  let material = $state<THREE.ShaderMaterial | null>(null)

  // Create/recreate material when heightmapTexture or normalMap change
  $effect(() => {
    const hm = heightmapTexture
    const nm = normalMap
    if (!hm || !nm) return

    const mat = createWaterMaterial({ heightmapTexture: hm, normalMap: nm })
    material = mat

    return () => {
      mat.dispose()
    }
  })

  // Update time uniform every frame
  $effect(() => {
    if (material) material.uniforms.uTime.value = time
  })

  // Update sun uniforms
  $effect(() => {
    if (!material) return
    if (sunDirection) material.uniforms.uSunDirection.value.copy(sunDirection)
    if (sunColor) material.uniforms.uSunColor.value.copy(sunColor)
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
