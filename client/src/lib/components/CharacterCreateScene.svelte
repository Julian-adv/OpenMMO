<script module lang="ts">
  import { RectAreaLightNode } from 'three/webgpu'
  import { RectAreaLightTexturesLib } from 'three/addons/lights/RectAreaLightTexturesLib.js'

  RectAreaLightNode.setLTC(RectAreaLightTexturesLib.init())
</script>

<script lang="ts">
  import { T, useThrelte, useTask } from '@threlte/core'
  import * as THREE from 'three'
  import { PMREMGenerator, type WebGPURenderer } from 'three/webgpu'
  import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
  import { onMount } from 'svelte'
  import type {
    CharacterClass,
    CharacterAppearance,
    Gender,
    VisibleEquipment,
  } from '../network/networkTypes'
  import CharacterPreview from './CharacterPreview.svelte'

  interface Props {
    characterClass: CharacterClass
    gender: Gender
    appearance: CharacterAppearance
    facePreview?: boolean
  }

  let {
    characterClass,
    gender,
    appearance,
    facePreview = false,
  }: Props = $props()

  const STARTER_EQUIPMENT: Partial<Record<CharacterClass, VisibleEquipment>> = {
    bard: {
      armor: {
        head: 'worn_bard_helmet',
        chest: 'worn_bard_top',
        pants: 'worn_bard_pants',
        boots: 'worn_bard_boots',
      },
    },
    priest: {
      main_hand: 'worn_mace',
      armor: {
        head: 'worn_priest_helmet',
        chest: 'worn_priest_top',
        pants: 'worn_priest_pants',
        boots: 'worn_priest_boots',
      },
    },
    ranger: {
      armor: {
        chest: 'worn_ranger_top',
        pants: 'worn_ranger_pants',
        hands: 'worn_ranger_gloves',
        boots: 'worn_ranger_boots',
      },
    },
    knight: {
      armor: {
        chest: 'worn_breastplate',
        pants: 'worn_plate_greaves',
        boots: 'worn_plate_boots',
        hands: 'worn_plate_gauntlets',
      },
    },
    rogue: {
      main_hand: 'worn_dagger',
      armor: {
        chest: 'worn_rogue_top',
        pants: 'worn_rogue_pants',
        hands: 'worn_rogue_gloves',
        boots: 'worn_rogue_boots',
      },
    },
    barbarian: {
      armor: {
        chest: 'worn_barbarian_armor',
        pants: 'worn_barbarian_pants',
        boots: 'worn_barbarian_boots',
        hands: 'worn_barbarian_bracers',
      },
    },
    caveman: {
      armor: {
        chest: 'worn_caveman_top',
        pants: 'worn_caveman_pants',
        boots: 'worn_caveman_boots',
        hands: 'worn_caveman_bracers',
      },
    },
  }
  const FEMALE_STARTER_EQUIPMENT: typeof STARTER_EQUIPMENT = {
    valkyrie: {
      armor: {
        head: 'worn_valkyrie_helmet',
        chest: 'worn_valkyrie_top',
        pants: 'worn_valkyrie_pants',
        hands: 'worn_valkyrie_gloves',
        boots: 'worn_valkyrie_boots',
      },
    },
    rogue: STARTER_EQUIPMENT.rogue,
    caveman: STARTER_EQUIPMENT.caveman,
    ranger: STARTER_EQUIPMENT.ranger,
    bard: {
      armor: {
        ...STARTER_EQUIPMENT.bard?.armor,
        hands: 'worn_bard_gloves',
      },
    },
    priest: {
      ...STARTER_EQUIPMENT.priest,
      armor: {
        ...STARTER_EQUIPMENT.priest?.armor,
        hands: 'worn_priest_gloves',
      },
    },
    knight: {
      armor: { ...STARTER_EQUIPMENT.knight?.armor, head: 'worn_plate_helmet' },
    },
    barbarian: {
      armor: {
        ...STARTER_EQUIPMENT.barbarian?.armor,
        head: 'worn_barbarian_helmet',
      },
    },
  }

  // Helmets hide the chosen hair, so the preview takes them off; the bard hat keeps it in view.
  function bareHeaded(equipment?: VisibleEquipment) {
    if (!equipment?.armor?.head || characterClass === 'bard') return equipment
    const { head: _, ...armor } = equipment.armor
    return { ...equipment, armor }
  }

  const CAMERA_FOV = 42
  const CAMERA_POSITION_Y = 1.4
  const CAMERA_POSITION_Z = 7.6
  const CAMERA_LOOK_AT_Y = 0.85
  const FACE_CAMERA_Y = 1.85
  const FACE_CAMERA_DISTANCE = 0.98
  const FACE_LOOK_AT_Y = 1.82
  const FEMALE_FACE_DROP = 0.04

  const PLATFORM_RADIUS = 0.92
  const PLATFORM_THICKNESS = 0.1
  const PLATFORM_Y = PLATFORM_THICKNESS / 2 + 0.002
  const CHARACTER_Y_OFFSET = PLATFORM_THICKNESS
  const CHARACTER_Z = 2.3

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

  const keyLight = new THREE.RectAreaLight('#fff2e3', 3.5, 3, 2.5)
  keyLight.position.set(-1.8, 2.5, CHARACTER_Z + 3.5)
  keyLight.lookAt(0, 1.55, CHARACTER_Z)

  const fillLight = new THREE.RectAreaLight('#edf4ff', 1.4, 3, 3)
  fillLight.position.set(1.8, 1.7, CHARACTER_Z + 3)
  fillLight.lookAt(0, 1.45, CHARACTER_Z)

  let portraitLightsAdded = false

  $effect(() => {
    if (!cameraRef) return
    const drop = gender === 'female' ? FEMALE_FACE_DROP : 0
    cameraRef.position.set(
      0,
      facePreview ? FACE_CAMERA_Y - drop : CAMERA_POSITION_Y,
      facePreview ? CHARACTER_Z + FACE_CAMERA_DISTANCE : CAMERA_POSITION_Z
    )
    cameraRef.lookAt(
      0,
      facePreview ? FACE_LOOK_AT_Y - drop : CAMERA_LOOK_AT_Y,
      CHARACTER_Z
    )
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
    let mounted = true
    let environment: THREE.RenderTarget | undefined
    const previousEnvironment = scene.environment
    const previousIntensity = scene.environmentIntensity
    const studioRenderer = renderer as unknown as WebGPURenderer
    void studioRenderer.init().then(() => {
      if (!mounted) return
      const room = new RoomEnvironment()
      const generator = new PMREMGenerator(studioRenderer)
      environment = generator.fromScene(room)
      generator.dispose()
      room.dispose()
      scene.environment = environment.texture
      scene.environmentIntensity = 0.18
    })

    const canvas = renderer.domElement
    canvas.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)

    return () => {
      mounted = false
      scene.background = null
      if (scene.environment === environment?.texture) {
        scene.environment = previousEnvironment
        scene.environmentIntensity = previousIntensity
      }
      environment?.dispose()
      canvas.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', onPointerUp)
      if (portraitLightsAdded) {
        scene.remove(keyLight, fillLight)
        portraitLightsAdded = false
      }
      keyLight.dispose()
      fillLight.dispose()
    }
  })

  // Game loop for single character preview
  useTask((delta) => {
    if (!characterPreview) return

    if (characterPreview.isGltfReady() && !characterPreview.isSetUp()) {
      characterPreview.setup()
      if (!portraitLightsAdded) {
        scene.add(keyLight, fillLight)
        portraitLightsAdded = true
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

<T.HemisphereLight args={['#fff3e6', '#9294a0', 0.45]} />
<T.DirectionalLight
  position={[-3, 3.5, CHARACTER_Z - 3]}
  intensity={0.6}
  color="#dbe7ff"
/>

<T.Mesh
  rotation.x={-Math.PI / 2}
  position={[0, -0.01, CHARACTER_Z]}
  receiveShadow
>
  <T.PlaneGeometry args={[28, 28]} />
  <T.MeshStandardMaterial color="#1a2535" opacity={0.6} transparent />
</T.Mesh>

<T.Mesh position={[0, PLATFORM_Y, CHARACTER_Z]} receiveShadow>
  <T.CylinderGeometry
    args={[PLATFORM_RADIUS, PLATFORM_RADIUS, PLATFORM_THICKNESS, 40]}
  />
  <T.MeshStandardMaterial color="#2f3f52" opacity={1.0} transparent />
</T.Mesh>

{#key `${characterClass}-${gender}-${appearance.face}`}
  <CharacterPreview
    bind:this={characterPreview}
    positionX={0}
    positionY={CHARACTER_Y_OFFSET}
    positionZ={CHARACTER_Z}
    selected={true}
    {characterClass}
    {gender}
    {appearance}
    equipment={gender === 'male'
      ? STARTER_EQUIPMENT[characterClass]
      : bareHeaded(FEMALE_STARTER_EQUIPMENT[characterClass])}
    rotationY={modelRotationY}
  />
{/key}
