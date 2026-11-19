<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { PMREMGenerator, type WebGPURenderer } from 'three/webgpu'
  import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
  import { createCharacterModelRoot } from '../utils/characterAnimationUtils'
  import { getCharacterModelPath } from '../utils/modelPaths'
  import {
    loadCharacterModel,
    applyCharacterArmor,
    disposeCharacterSkeletons,
  } from '../utils/characterModel'
  import { equippedArmor } from '../stores/inventoryStore'
  import type {
    CharacterAppearance,
    CharacterClass,
    Gender,
  } from '../network/networkTypes'

  interface Props {
    characterClass: CharacterClass
    gender: Gender
    appearance: CharacterAppearance
  }

  let { characterClass, gender, appearance }: Props = $props()

  const { renderer: _renderer, scene, invalidate } = useThrelte()
  const renderer = _renderer as unknown as WebGPURenderer

  $effect(() => {
    let envRt: THREE.RenderTarget | null = null
    let disposed = false
    renderer.init().then(() => {
      if (disposed) return
      const pmremGenerator = new PMREMGenerator(renderer)
      const room = new RoomEnvironment()
      envRt = pmremGenerator.fromScene(room)
      scene.environment = envRt.texture
      scene.environmentIntensity = 0.55
      room.traverse((o) => (o as THREE.Mesh).geometry?.dispose())
      pmremGenerator.dispose()
      invalidate()
    })
    return () => {
      disposed = true
      scene.environment = null
      envRt?.dispose()
    }
  })

  let modelRoot = $state<THREE.Group | null>(null)
  // Head height of the loaded body, which the camera frames.
  let headY = $state(1.65)
  const modelPath = $derived(getCharacterModelPath(characterClass, gender))
  const face = $derived(appearance.face)
  // A helmet would hide the hair being chosen.
  const armor = $derived({ ...$equippedArmor, head: undefined })

  $effect(() => {
    let cancelled = false
    loadCharacterModel(modelPath, { face })
      .then((gltf) => {
        if (cancelled) return
        const { modelRoot: root } = createCharacterModelRoot(gltf.scene)
        const head = root.getObjectByName('Head')
        if (head) {
          root.updateMatrixWorld(true)
          headY = head.getWorldPosition(new THREE.Vector3()).y + 0.07
        }
        modelRoot = root
        invalidate()
      })
      .catch((error) => console.error('Failed to load salon preview', error))
    return () => {
      cancelled = true
      if (modelRoot) disposeCharacterSkeletons(modelRoot)
      modelRoot = null
    }
  })

  $effect(() => {
    if (!modelRoot) return
    applyCharacterArmor(modelRoot, armor, appearance).then(invalidate)
    invalidate()
  })
</script>

{#key headY}
  <T.PerspectiveCamera
    makeDefault
    fov={30}
    position={[0.28, headY + 0.04, 1.2]}
    oncreate={(camera) => camera.lookAt(0, headY + 0.01, 0)}
  />
{/key}

<T.AmbientLight intensity={0.4} />
<T.DirectionalLight position={[1.5, 3, 2.5]} intensity={1.4} />

{#if modelRoot}
  <T is={modelRoot} />
{/if}
