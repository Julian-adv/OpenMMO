<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'

  import type { MonsterData } from '../types/Monster'

  interface Props {
    position: { x: number; y: number; z: number }
    rotation: number
    monsterState: MonsterData['state']
    id: string
  }

  let { position, rotation, monsterState, id }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/scp939.glb')

  let mixer = $state<THREE.AnimationMixer | undefined>(undefined)
  let currentAction = $state<THREE.AnimationAction | undefined>(undefined)
  let model: THREE.Group | undefined = $state(undefined)
  let group = $state<THREE.Group>()

  // Export update function to be called from parent
  export function update(deltaTime: number) {
    if (mixer) {
      mixer.update(deltaTime)
    }
  }

  $effect(() => {
    if ($gltf) {
      // Clone the model for this instance
      if (!model) {
        const clonedScene = SkeletonUtils.clone($gltf.scene) as THREE.Group

        // Enable shadows on all meshes
        clonedScene.traverse((child) => {
          if ((child as THREE.Mesh).isMesh) {
            child.castShadow = true
            child.receiveShadow = true
            // Add user data to identify as monster part
            child.userData.monsterId = id
          }
        })

        model = clonedScene
        // Setup mixer on the cloned scene
        mixer = new THREE.AnimationMixer(clonedScene)
        console.log(
          'Monster animations:',
          $gltf.animations.map((c) => c.name)
        )
      }
    }
  })

  $effect(() => {
    if (mixer && $gltf) {
      let clipName = '939_Idle'
      if (monsterState === 'walk') clipName = '939_Walking'
      if (monsterState === 'run') clipName = '939_Running'
      // if (monsterState === 'attack') clipName = '939_Attack1'

      const clip = $gltf.animations.find((c) => c.name === clipName)

      if (clip) {
        const newAction = mixer.clipAction(clip)
        if (newAction !== currentAction) {
          if (currentAction) {
            currentAction.fadeOut(0.2)
          }
          newAction.reset().fadeIn(0.2).play()
          currentAction = newAction
        }
      } else {
        console.warn(
          `Animation ${clipName} not found used for state ${monsterState}`
        )
        // Fallback: play first animation if available and nothing is playing
        if (!currentAction && $gltf.animations.length > 0) {
          const firstClip = $gltf.animations[0]
          const newAction = mixer.clipAction(firstClip)
          newAction.play()
          currentAction = newAction
        }
      }
    }
  })

  // Export the model group for raycasting from parent
  export function getMeshGroup() {
    return group
  }
</script>

{#if model}
  <T.Group
    bind:ref={group}
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
    scale={[1, 1, 1]}
  >
    <T is={model} castShadow receiveShadow />
  </T.Group>
{/if}
