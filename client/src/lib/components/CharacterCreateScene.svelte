<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { CharacterClass } from '../network/networkTypes'
  import CharacterPreview from './CharacterPreview.svelte'

  interface Props {
    characterClass: CharacterClass
  }

  let { characterClass }: Props = $props()

  const CAMERA_FOV = 42
  const CAMERA_POSITION_Y = 1.4
  const CAMERA_POSITION_Z = 7.6
  const CAMERA_LOOK_AT_Y = 0.85

  const PLATFORM_RADIUS = 0.92
  const PLATFORM_THICKNESS = 0.1
  const PLATFORM_Y = PLATFORM_THICKNESS / 2 + 0.002
  const CHARACTER_Y_OFFSET = PLATFORM_THICKNESS
  const CHARACTER_Z = 2.3

  const AMBIENT_INTENSITY = 0.12
  const KEY_LIGHT_INTENSITY = 0.05
  const FILL_LIGHT_INTENSITY = 0.48

  let cameraRef = $state<THREE.PerspectiveCamera | undefined>(undefined)

  $effect(() => {
    if (!cameraRef) return
    cameraRef.lookAt(0, CAMERA_LOOK_AT_Y, CHARACTER_Z)
  })
</script>

<T.PerspectiveCamera
  makeDefault
  position={[0, CAMERA_POSITION_Y, CAMERA_POSITION_Z]}
  fov={CAMERA_FOV}
  bind:ref={cameraRef}
/>

<T.AmbientLight intensity={AMBIENT_INTENSITY} />
<T.DirectionalLight
  position={[5, 8, 5]}
  intensity={KEY_LIGHT_INTENSITY}
  castShadow
  shadow.camera.left={-8}
  shadow.camera.right={8}
  shadow.camera.top={8}
  shadow.camera.bottom={-8}
  shadow.camera.near={0.5}
  shadow.camera.far={24}
  shadow.mapSize.width={1024}
  shadow.mapSize.height={1024}
  shadow.bias={-0.00025}
  shadow.normalBias={0.02}
/>
<T.DirectionalLight
  position={[-3, 6, -2]}
  intensity={FILL_LIGHT_INTENSITY}
  color="#8899cc"
/>

<T.Mesh
  rotation.x={-Math.PI / 2}
  position={[0, -0.01, CHARACTER_Z]}
  receiveShadow
>
  <T.PlaneGeometry args={[10, 10]} />
  <T.MeshStandardMaterial color="#1a2535" opacity={0.6} transparent />
</T.Mesh>

<T.Mesh
  position={[0, PLATFORM_Y, CHARACTER_Z]}
  receiveShadow
>
  <T.CylinderGeometry args={[PLATFORM_RADIUS, PLATFORM_RADIUS, PLATFORM_THICKNESS, 40]} />
  <T.MeshStandardMaterial color="#2f3f52" opacity={1.0} transparent />
</T.Mesh>

{#key characterClass}
  <CharacterPreview
    positionX={0}
    positionY={CHARACTER_Y_OFFSET}
    positionZ={CHARACTER_Z}
    selected={true}
    {characterClass}
  />
{/key}
