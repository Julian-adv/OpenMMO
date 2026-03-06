<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'

  interface Props {
    geometry: THREE.BufferGeometry
    material: MeshStandardNodeMaterial
    mesh?: THREE.Mesh | undefined
    position?: [number, number, number]
    onBeforeRender?: THREE.Mesh['onBeforeRender'] | null
  }

  let {
    geometry,
    material,
    mesh = $bindable(undefined),
    position = [0, 0, 0],
    onBeforeRender = null,
  }: Props = $props()

  function handleCreate(ref: THREE.Mesh) {
    if (onBeforeRender) ref.onBeforeRender = onBeforeRender
  }
</script>

<T.Mesh
  bind:ref={mesh}
  {geometry}
  {material}
  {position}
  castShadow
  receiveShadow
  frustumCulled={false}
  oncreate={handleCreate}
/>
