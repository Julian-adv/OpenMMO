<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'

  interface Props {
    position: { x: number; y: number; z: number }
    rotation?: number
    monsterState?: 'idle' | 'moving' | 'attack'
  }

  let { position, rotation = 0, monsterState = 'idle' }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/scp939.glb')

  let mixer = $state<THREE.AnimationMixer | undefined>(undefined)
  let currentAction = $state<THREE.AnimationAction | undefined>(undefined)
  let model: THREE.Group | undefined = $state(undefined)

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
      const clipName = monsterState === 'moving' ? '939_Running' : '939_Idle'
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
        console.warn(`Animation ${clipName} not found`)
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
</script>

{#if model}
  <T.Group
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
    scale={[1, 1, 1]}
  >
    <T is={model} castShadow receiveShadow />
  </T.Group>
{/if}
