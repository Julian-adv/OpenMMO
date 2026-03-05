<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'

  interface Props {
    geometry: THREE.BufferGeometry
    material: MeshStandardNodeMaterial
    mesh?: THREE.Mesh | undefined
    position?: [number, number, number]
  }

  let {
    geometry,
    material,
    mesh = $bindable(undefined),
    position = [0, 0, 0],
  }: Props = $props()
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
