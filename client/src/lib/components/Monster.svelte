<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'

  interface Props {
    position: { x: number; y: number; z: number }
  }

  let { position }: Props = $props()

  const gltf = useLoader(GLTFLoader).load('/models/scp939.glb')

  let mixer: THREE.AnimationMixer | undefined
  let currentAction: THREE.AnimationAction | undefined

  // Export update function to be called from parent
  export function update(deltaTime: number) {
    if (mixer) {
      mixer.update(deltaTime)
    }
  }

  $effect(() => {
    if ($gltf) {
      // Setup mixer
      mixer = new THREE.AnimationMixer($gltf.scene)

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

{#if $gltf}
  <T.Group
    position={[position.x, position.y, position.z]}
    rotation={[0, 0, 0]}
    scale={[1, 1, 1]}
  >
    <T is={$gltf.scene} castShadow receiveShadow />
  </T.Group>
{/if}
