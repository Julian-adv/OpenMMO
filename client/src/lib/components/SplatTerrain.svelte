<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'
  import type { ResolvedRegionLayers } from '../managers/terrainMetaManager'

  interface Props {
    geometry: THREE.BufferGeometry
    material: MeshStandardNodeMaterial
    mesh?: THREE.Mesh | undefined
    position?: [number, number, number]
    splatTexture?: THREE.Texture | null
    regionLayers?: ResolvedRegionLayers | null
  }

  let {
    geometry,
    material,
    mesh = $bindable(undefined),
    position = [0, 0, 0],
    splatTexture = null,
    regionLayers = null,
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

  // Placeholder textures for missing normal/ORM maps
  const placeholderNorm = new THREE.DataTexture(
    new Uint8Array([128, 128, 255, 255]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  placeholderNorm.needsUpdate = true

  const placeholderORM = new THREE.DataTexture(
    new Uint8Array([255, 255, 0, 255]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  placeholderORM.needsUpdate = true

  // Swap per-tile textures on the shared material before each draw
  $effect(() => {
    if (!mesh) return
    const tex = splatTexture ?? defaultSplat
    const rl = regionLayers

    mesh.onBeforeRender = () => {
      const u = material.userData?.uniforms
      if (!u) return

      u.splatMap.value = tex

      if (rl) {
        u.diffTex0.value = rl.layers[0].map
        u.diffTex1.value = rl.layers[1].map
        u.diffTex2.value = rl.layers[2].map
        u.diffTex3.value = rl.layers[3].map

        if (u.normTex0) {
          u.normTex0.value = rl.layers[0].normalMap ?? placeholderNorm
          u.normTex1.value = rl.layers[1].normalMap ?? placeholderNorm
          u.normTex2.value = rl.layers[2].normalMap ?? placeholderNorm
          u.normTex3.value = rl.layers[3].normalMap ?? placeholderNorm
        }

        if (u.ormTex0) {
          u.ormTex0.value = rl.layers[0].orm ?? placeholderORM
          u.ormTex1.value = rl.layers[1].orm ?? placeholderORM
          u.ormTex2.value = rl.layers[2].orm ?? placeholderORM
          u.ormTex3.value = rl.layers[3].orm ?? placeholderORM
        }

        u.uTile0.value = rl.layers[0].tile
        u.uTile1.value = rl.layers[1].tile
        u.uTile2.value = rl.layers[2].tile
        u.uTile3.value = rl.layers[3].tile
      }
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
  frustumCulled={false}
/>
