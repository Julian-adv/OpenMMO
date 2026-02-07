<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'

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
  let nametagGroup = $state<THREE.Group | undefined>(undefined)
  let animDebugInfo = $state('')
  let isDeadAnimationFinished = $state(false)
  let lastMonsterState = $state<MonsterData['state'] | undefined>(undefined)
  let lastDeadAnimFinished = $state(false)

  function playAnimation() {
    if (!mixer || !$gltf) return

    let clipName = '939_Idle'
    if (monsterState === 'walk') clipName = '939_Walking'
    if (monsterState === 'run') clipName = '939_Running'
    if (monsterState === 'attack') clipName = '939_Attack1'
    if (monsterState === 'hit') clipName = '939_AddStagger1'
    if (monsterState === 'dead') {
      clipName = isDeadAnimationFinished ? '939_Dead' : '939_Die'
    }

    const clip = $gltf.animations.find((c) => c.name === clipName)

    if (clip) {
      const newAction = mixer.clipAction(clip)
      if (newAction !== currentAction) {
        if (currentAction) {
          currentAction.fadeOut(0.2)
        }

        newAction.reset().fadeIn(0.2).play()

        if (monsterState === 'dead') {
          if (clipName === '939_Die') {
            newAction.setLoop(THREE.LoopOnce, 1)
            newAction.clampWhenFinished = true
          } else {
            // 939_Dead should loop or stay idle
            newAction.setLoop(THREE.LoopRepeat, Infinity)
            newAction.clampWhenFinished = false
          }
        } else {
          newAction.setLoop(THREE.LoopRepeat, Infinity)
          newAction.clampWhenFinished = false
          isDeadAnimationFinished = false
        }

        currentAction = newAction
      }
    } else {
      console.warn(
        `Animation ${clipName} not found used for state ${monsterState}`
      )
      if (!currentAction && $gltf.animations.length > 0) {
        const firstClip = $gltf.animations[0]
        const newAction = mixer.clipAction(firstClip)
        newAction.play()
        currentAction = newAction
      }
    }
  }

  // Export update function to be called from parent
  export function update(deltaTime: number, camera?: THREE.Camera) {
    // 1. Sync animation with state
    if (
      lastMonsterState !== monsterState ||
      lastDeadAnimFinished !== isDeadAnimationFinished
    ) {
      lastMonsterState = monsterState
      lastDeadAnimFinished = isDeadAnimationFinished
      playAnimation()
    }

    // 2. Update mixer
    if (mixer) {
      mixer.update(deltaTime)

      // Update debug info for slow mode
      const currentTS = get(timeScale)
      if (currentTS < 1.0 && currentAction) {
        const time = currentAction.time.toFixed(2)
        const duration = currentAction.getClip().duration.toFixed(2)
        const animName = currentAction.getClip().name
        animDebugInfo = `[${animName}] ${time}s / ${duration}s`
      } else {
        animDebugInfo = ''
      }
    }

    // Update nametag to face camera
    if (camera && nametagGroup) {
      nametagGroup.position.set(position.x, position.y + 2.5, position.z)
      nametagGroup.quaternion.copy(camera.quaternion)
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

        mixer.addEventListener('finished', (e) => {
          console.log(`Animation finished: ${e.action.getClip().name}`)
          if (e.action.getClip().name === '939_Die') {
            isDeadAnimationFinished = true
          }
        })

        const animationNames = $gltf.animations.map((c) => c.name)
        console.log(`Monster ${id} animations:`, animationNames)
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

<!-- Name tag / Debug info -->
<T.Group bind:ref={nametagGroup}>
  {#if animDebugInfo}
    <Text
      text={id}
      fontSize={0.2}
      color="#ffffff"
      position.y={0.3}
      anchorX="center"
      anchorY="middle"
    />
    <Text
      text={animDebugInfo}
      fontSize={0.2}
      color="#ffff00"
      position.y={0.6}
      anchorX="center"
      anchorY="middle"
    />
  {/if}
</T.Group>