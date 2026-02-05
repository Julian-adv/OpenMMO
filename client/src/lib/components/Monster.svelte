<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'

  interface Props {
    position: { x: number; y: number; z: number }
  }

  let { position }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/scp939.glb')
</script>

{#if $gltf}
  <T.Group
    position={[position.x, position.y, position.z]}
    rotation={[0, 0, 0]}
    scale={[1, 1, 1]}
  >
    <T is={$gltf.scene} castShadow receiveShadow />
  </T.Group>
{/if}
