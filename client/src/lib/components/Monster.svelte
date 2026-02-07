<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'

  import type { MonsterData } from '../types/Monster'

  interface FloatingText {
    id: number
    text: string
    color: string
    y: number // Relative height offset
    opacity: number
    life: number // 0 to 1
  }

  interface Props {
    position: { x: number; y: number; z: number }
    rotation: number
    monsterState: MonsterData['state']
    id: string
    lastDamageInfo?: MonsterData['lastDamageInfo']
    camera?: THREE.Camera
  }

  let { position, rotation, monsterState, id, lastDamageInfo, camera }: Props =
    $props()

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
  let floatingTexts = $state<FloatingText[]>([])
  let nextTextId = 0
  let lastDamageTrigger = $state(0)
  let textRefs = new Map<number, THREE.Group>()

  function bindTextRef(id: number, ref: THREE.Group | undefined) {
    if (ref) {
      textRefs.set(id, ref)
    } else {
      textRefs.delete(id)
    }
  }

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

    // 2. Check for new damage
    if (lastDamageInfo && lastDamageInfo.trigger !== lastDamageTrigger) {
      lastDamageTrigger = lastDamageInfo.trigger

      const text = lastDamageInfo.hit ? `${lastDamageInfo.damage}` : 'Miss'
      const color = lastDamageInfo.hit ? '#ff4d4d' : '#a0aec0'

      // Use reassignment for better Svelte 5 reactivity
      floatingTexts = [
        ...floatingTexts,
        {
          id: nextTextId++,
          text,
          color,
          y: 2.5, // Start height offset from monster base
          opacity: 1,
          life: 1.0,
        },
      ]
    }

    // 3. Update floating texts
    if (floatingTexts.length > 0) {
      floatingTexts = floatingTexts
        .map((t) => ({
          ...t,
          life: t.life - deltaTime,
          y: t.y + deltaTime * 1.5,
          opacity: Math.max(0, Math.min(1, t.life * 2)),
        }))
        .filter((t) => t.life > 0)

      // Manual sync for perfect billboarding (Screen-aligned)
      if (camera) {
        floatingTexts.forEach((t) => {
          const ref = textRefs.get(t.id)
          if (ref) {
            // Position relative to monster, but set in world space for simplicity
            // or we could put them inside the group. Here we use world space to match previous working style
            ref.position.set(position.x, position.y + t.y, position.z)
            ref.quaternion.copy(camera.quaternion)
          }
        })
      }
    }

    // 4. Update mixer
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

<!-- Floating Damage Text -->
<!-- position={[text.x, position.y + text.y, text.z]}
      quaternion={[
        camera.quaternion.x,
        camera.quaternion.y,
        camera.quaternion.z,
        camera.quaternion.w,
      ]} -->

<!-- Floating Damage Text -->
{#each floatingTexts as text (text.id)}
  <T.Group
    position={[position.x, position.y + text.y, position.z]}
    on:create={({ ref }) => {
      textRefs.set(text.id, ref)
      return () => {
        textRefs.delete(text.id)
      }
    }}
  >
    <Text
      text={text.text}
      fontSize={0.3}
      color={text.color}
      fillOpacity={text.opacity}
      anchorX="center"
      anchorY="middle"
    />
  </T.Group>
{/each}
