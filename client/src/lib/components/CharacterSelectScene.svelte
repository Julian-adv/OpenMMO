<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { AccountCharacter } from '../network/socket'
  import CharacterPreview from './CharacterPreview.svelte'

  interface Props {
    characters: AccountCharacter[]
    selectedCharacterId: number | null
  }

  let { characters, selectedCharacterId }: Props = $props()

  const SLOT_POSITIONS = [-3, 0, 3]

  let cameraRef = $state<THREE.PerspectiveCamera | undefined>(undefined)

  $effect(() => {
    if (cameraRef) {
      cameraRef.lookAt(0, 0.8, 0)
    }
  })
</script>

<T.PerspectiveCamera
  makeDefault
  position={[0, 1.5, 6]}
  fov={45}
  bind:ref={cameraRef}
/>

<T.AmbientLight intensity={0.5} />
<T.DirectionalLight position={[5, 8, 5]} intensity={1.2} />
<T.DirectionalLight position={[-3, 6, -2]} intensity={0.4} color="#8899cc" />

<T.Mesh rotation.x={-Math.PI / 2} position.y={-0.01} receiveShadow>
  <T.PlaneGeometry args={[12, 6]} />
  <T.MeshStandardMaterial color="#1a2535" opacity={0.6} transparent />
</T.Mesh>

{#each [0, 1, 2] as slotIndex (slotIndex)}
  {@const character = characters[slotIndex]}
  {#if character}
    <CharacterPreview
      positionX={SLOT_POSITIONS[slotIndex]}
      selected={character.id === selectedCharacterId}
    />
  {/if}
{/each}
