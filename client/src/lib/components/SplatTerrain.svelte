<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'

  interface Props {
    geometry: THREE.BufferGeometry
    material: MeshStandardNodeMaterial
    mesh?: THREE.Mesh | undefined
    position?: [number, number, number]
    splatTexture?: THREE.Texture | null
  }

  let {
    geometry,
    material,
    mesh = $bindable(undefined),
    position = [0, 0, 0],
    splatTexture = null,
  }: Props = $props()

  // Default 1x1 all-grass splatmap used until the real one loads
  const defaultSplat = new THREE.DataTexture(
    new Uint8Array([255, 0, 0, 0]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  defaultSplat.wrapS = defaultSplat.wrapT = THREE.ClampToEdgeWrapping
  defaultSplat.minFilter = THREE.LinearFilter
  defaultSplat.magFilter = THREE.LinearFilter
  defaultSplat.needsUpdate = true

  // Swap per-tile splatTexture on the shared material before each draw
  $effect(() => {
    if (!mesh) return
    const tex = splatTexture ?? defaultSplat
    mesh.onBeforeRender = () => {
      const u = material.userData?.uniforms
      if (u) u.splatMap.value = tex
    }
  })
</script>

<T.Mesh
  bind:ref={mesh}
  {geometry}
  {material}
  {position}
  castShadow
  receiveShadow
/>
