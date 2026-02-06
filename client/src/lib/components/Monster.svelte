<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'

  interface Props {
    position: { x: number; y: number; z: number }
    rotation?: number
  }

  let { position, rotation = 0 }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/scp939.glb')

  let mixer: THREE.AnimationMixer | undefined
  let currentAction: THREE.AnimationAction | undefined
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
      const clonedScene = SkeletonUtils.clone($gltf.scene) as THREE.Group
      model = clonedScene

      // Setup mixer on the cloned scene
      mixer = new THREE.AnimationMixer(clonedScene)

      // Find idle animation
      const idleClip = $gltf.animations.find((clip) => clip.name === '939_Idle')

      if (idleClip) {
        currentAction = mixer.clipAction(idleClip)
        currentAction.play()
      } else {
        console.warn(
          '939_Idle animation not found in model',
          $gltf.animations.map((c) => c.name)
        )
        // Fallback: play first animation if available
        if ($gltf.animations.length > 0) {
          currentAction = mixer.clipAction($gltf.animations[0])
          currentAction.play()
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
