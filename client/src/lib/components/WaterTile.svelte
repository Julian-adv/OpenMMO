<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { NodeMaterial } from 'three/webgpu'
  import type { WaterMaterialResult } from '../shaders/water-material'

  interface Props {
    geometry: THREE.BufferGeometry
    position?: [number, number, number]
    heightmapTexture: THREE.DataTexture
    material: NodeMaterial
    waterResult: WaterMaterialResult
  }

  let {
    geometry,
    position = [0, 0, 0],
    heightmapTexture,
    material,
    waterResult,
  }: Props = $props()

  let mesh = $state<THREE.Mesh | undefined>(undefined)

  // Swap per-tile heightmap texture on the shared material before each draw
  $effect(() => {
    if (!mesh) return
    const tex = heightmapTexture
    mesh.onBeforeRender = () => {
      waterResult.uniforms.uHeightmapTexture.value = tex
    }
  })

  // Position Y slightly above terrain to avoid z-fighting
  const waterPosition: [number, number, number] = $derived([position[0], 0.01, position[2]])
</script>

<T.Mesh
  bind:ref={mesh}
  {geometry}
  {material}
  position={waterPosition}
  receiveShadow={false}
  castShadow={false}
/>
