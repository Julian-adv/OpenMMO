<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { PMREMGenerator, type WebGPURenderer } from 'three/webgpu'
  import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
  import { onMount } from 'svelte'
  import { interactivity } from '@threlte/extras'
  import type { AccountCharacter } from '../network/socket'
  import CharacterPreview from './CharacterPreview.svelte'
  import CharacterSlotLabel from './CharacterSlotLabel.svelte'

  interactivity()

  interface Props {
    characters: AccountCharacter[]
    selectedCharacterId: number | null
    onSlotClick: (slotIndex: number) => void
  onSlotDoubleClick: (slotIndex: number) => void
  }

  let { characters, selectedCharacterId, onSlotClick, onSlotDoubleClick }: Props = $props()

  const SLOT_SPACING = 1.8
  const SLOT_POSITIONS = [-SLOT_SPACING, 0, SLOT_SPACING]
  const SLOT_DEPTH = 2.5
  const SLOT_DISC_RADIUS = 0.76
  const SLOT_DISC_THICKNESS = 0.1
  const SLOT_DISC_Y = SLOT_DISC_THICKNESS / 2 + 0.002
  const CHARACTER_Y_OFFSET = SLOT_DISC_THICKNESS
  const PLATFORM_MARGIN_X = 2.8
  const PLATFORM_MARGIN_Z_FRONT = 3.2
  const PLATFORM_MARGIN_Z_BACK = 4.2
  const PLATFORM_SCALE = 2
  const BASE_PLATFORM_WIDTH = SLOT_SPACING * 2 + PLATFORM_MARGIN_X * 2
  const BASE_PLATFORM_DEPTH =
    PLATFORM_MARGIN_Z_FRONT + PLATFORM_MARGIN_Z_BACK + SLOT_DEPTH
  const PLATFORM_WIDTH = BASE_PLATFORM_WIDTH * PLATFORM_SCALE
  const PLATFORM_DEPTH = BASE_PLATFORM_DEPTH * PLATFORM_SCALE
  const PLATFORM_CENTER_Z = (SLOT_DEPTH + PLATFORM_MARGIN_Z_FRONT - PLATFORM_MARGIN_Z_BACK) / 2
  const CAMERA_FOV = 45
  const CAMERA_POSITION_Y = 1.5
  const CAMERA_LOOK_AT_Y = 0.8
  const CHARACTER_HALF_WIDTH = 1.0
  const CHARACTER_HALF_HEIGHT = 1.8
  const CAMERA_FIT_PADDING = 1.1
  const MAX_CAMERA_WIDTH = 1280
  const AMBIENT_INTENSITY = 0.12
  const KEY_LIGHT_INTENSITY = 0.05
  const FILL_LIGHT_INTENSITY = 0.48

  const { size, renderer: _renderer, scene } = useThrelte()
  // Cast renderer — Threlte types it as WebGLRenderer but we use WebGPURenderer via createRenderer
  const renderer = _renderer as unknown as WebGPURenderer
  let viewportSize = $state({ width: 1, height: 1 })
  let cameraPositionZ = $state(8)

  let cameraRef = $state<THREE.PerspectiveCamera | undefined>(undefined)

  onMount(() => {
    const unsubscribe = size.subscribe((nextSize) => {
      viewportSize = nextSize
    })

    const pmremGenerator = new PMREMGenerator(renderer)
    pmremGenerator.fromSceneAsync(new RoomEnvironment()).then((rt) => {
      scene.environment = rt.texture
      scene.environmentIntensity = 0.1
      pmremGenerator.dispose()
    })

    return () => {
      scene.environment?.dispose()
      scene.environment = null
      unsubscribe()
    }
  })

  function calculateCameraPositionZ(width: number, height: number) {
    const safeWidth = Math.min(Math.max(1, width), MAX_CAMERA_WIDTH)
    const safeHeight = Math.max(1, height)
    const aspect = safeWidth / safeHeight

    const halfVerticalFov = THREE.MathUtils.degToRad(CAMERA_FOV / 2)
    const halfHorizontalFov = Math.atan(Math.tan(halfVerticalFov) * aspect)

    const halfSpanX = SLOT_SPACING + CHARACTER_HALF_WIDTH
    const fitDistanceByWidth = halfSpanX / Math.tan(halfHorizontalFov)
    const fitDistanceByHeight = CHARACTER_HALF_HEIGHT / Math.tan(halfVerticalFov)
    const offsetZ = Math.max(fitDistanceByWidth, fitDistanceByHeight) * CAMERA_FIT_PADDING

    return SLOT_DEPTH + offsetZ
  }

  $effect(() => {
    cameraPositionZ = calculateCameraPositionZ(viewportSize.width, viewportSize.height)

    if (cameraRef) {
      cameraRef.lookAt(0, CAMERA_LOOK_AT_Y, SLOT_DEPTH)
    }
  })
</script>

<T.PerspectiveCamera
  makeDefault
  position={[0, CAMERA_POSITION_Y, cameraPositionZ]}
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
  position={[0, -0.01, PLATFORM_CENTER_Z]}
  receiveShadow
>
  <T.PlaneGeometry args={[PLATFORM_WIDTH, PLATFORM_DEPTH]} />
  <T.MeshStandardMaterial color="#1a2535" opacity={0.6} transparent depthWrite={false} envMapIntensity={0} />
</T.Mesh>

{#each [0, 1, 2] as slotIndex (slotIndex)}
  {@const character = characters[slotIndex]}
  <T.Mesh
    position={[SLOT_POSITIONS[slotIndex], SLOT_DISC_Y, SLOT_DEPTH]}
    receiveShadow
  >
    <T.CylinderGeometry
      args={[SLOT_DISC_RADIUS, SLOT_DISC_RADIUS, SLOT_DISC_THICKNESS, 40]}
    />
    <T.MeshStandardMaterial
      color="#2f3f52"
      opacity={1.0}
      transparent
      envMapIntensity={0}
    />
  </T.Mesh>

  {#if character}
    <CharacterPreview
      positionX={SLOT_POSITIONS[slotIndex]}
      positionY={CHARACTER_Y_OFFSET}
      positionZ={SLOT_DEPTH}
      selected={character.id === selectedCharacterId}
      characterClass={character.class}
    />
  {/if}

  <CharacterSlotLabel
    {character}
    selected={character?.id === selectedCharacterId}
    positionX={SLOT_POSITIONS[slotIndex]}
    positionZ={SLOT_DEPTH}
    camera={cameraRef}
    onclick={() => onSlotClick(slotIndex)}
    ondblclick={() => onSlotDoubleClick(slotIndex)}
  />
{/each}
