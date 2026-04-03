<script lang="ts">
  import { T, useThrelte, useTask } from '@threlte/core'
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import type { CharacterClass, Gender } from '../network/networkTypes'
  import { getCharacterModelYOffset } from '../utils/modelPaths'
  import CharacterPreview from './CharacterPreview.svelte'

  interface Props {
    characterClass: CharacterClass
    gender: Gender
  }

  let { characterClass, gender }: Props = $props()

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

  const { scene, renderer } = useThrelte()
  let cameraRef = $state<THREE.PerspectiveCamera | undefined>(undefined)

  // Drag-to-rotate
  let modelRotationY = $state(0)
  let dragging = false
  let lastPointerX = 0
  const DRAG_SENSITIVITY = 0.01

  interface CharacterPreviewInstance {
    isGltfReady(): boolean
    isSetUp(): boolean
    setup(): void
    update(delta: number): void
    dispose(): void
  }

  // CharacterPreview ref managed by bind:this
  let characterPreview: CharacterPreviewInstance | undefined = $state(undefined)

  // Single pair of spotlights — created once via Three.js
  const spotlightTarget = new THREE.Object3D()
  spotlightTarget.position.set(0, CHARACTER_Y_OFFSET + 0.9, CHARACTER_Z)

  const keyLight = new THREE.SpotLight('#ffffff', 9.0, 14, 0.34, 0.22, 1.2)
  keyLight.position.set(0, CHARACTER_Y_OFFSET + 4.0, CHARACTER_Z + 1.2)
  keyLight.castShadow = true
  keyLight.shadow.mapSize.set(2048, 2048)
  keyLight.shadow.camera.near = 0.5
  keyLight.shadow.camera.far = 18
  keyLight.shadow.bias = -0.0002
  keyLight.shadow.normalBias = 0.02
  keyLight.target = spotlightTarget

  const fillLight = new THREE.SpotLight('#fff2d8', 3.4, 12, 0.52, 0.8, 1.2)
  fillLight.position.set(0, CHARACTER_Y_OFFSET + 2.5, CHARACTER_Z + 3.1)
  fillLight.target = spotlightTarget

  let spotlightsAdded = false

  $effect(() => {
    if (!cameraRef) return
    cameraRef.lookAt(0, CAMERA_LOOK_AT_Y, CHARACTER_Z)
  })

  function onPointerDown(e: PointerEvent) {
    dragging = true
    lastPointerX = e.clientX
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return
    const dx = e.clientX - lastPointerX
    lastPointerX = e.clientX
    modelRotationY += dx * DRAG_SENSITIVITY
  }

  function onPointerUp() {
    dragging = false
  }

  onMount(() => {
    scene.background = new THREE.Color('#1a2a40')

    const canvas = renderer.domElement
    canvas.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)

    return () => {
      scene.background = null
      canvas.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', onPointerUp)
      if (spotlightsAdded) {
        scene.remove(keyLight, fillLight, spotlightTarget)
        spotlightsAdded = false
      }
    }
  })

  // Game loop for single character preview
  useTask((delta) => {
    if (!characterPreview) return

    if (characterPreview.isGltfReady() && !characterPreview.isSetUp()) {
      characterPreview.setup()
      if (!spotlightsAdded) {
        scene.add(spotlightTarget, keyLight, fillLight)
        spotlightsAdded = true
      }
    }

    if (characterPreview.isSetUp()) {
      characterPreview.update(delta)
    }
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

{#key `${characterClass}-${gender}`}
  <CharacterPreview
    bind:this={characterPreview}
    positionX={0}
    positionY={CHARACTER_Y_OFFSET + getCharacterModelYOffset(characterClass, gender)}
    positionZ={CHARACTER_Z}
    selected={true}
    {characterClass}
    {gender}
    rotationY={modelRotationY}
  />
{/key}
