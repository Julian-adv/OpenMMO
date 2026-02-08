<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'
  import DamageText from './DamageText.svelte'

  import type { MonsterData } from '../types/Monster'

  interface FloatingText {
    id: number
    text: string
    color: string
  }

  interface Props {
    position: { x: number; y: number; z: number }
    rotation: number
    monsterState: MonsterData['state']
    id: string
    lastDamageInfo?: MonsterData['lastDamageInfo']
  }

  let { position, rotation, monsterState, id, lastDamageInfo }: Props = $props()

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
  let damageTextRefs: DamageText[] = []
  let lastAppliedOpacity = 1
  let materialsCloned = false
  let corpseTimer = 0
  const CORPSE_FADE_START = 25
  const CORPSE_FADE_DURATION = 5

  function cloneMaterials() {
    if (materialsCloned || !model) return
    materialsCloned = true
    model.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        const mesh = child as THREE.Mesh
        if (Array.isArray(mesh.material)) {
          mesh.material = mesh.material.map((m) => m.clone())
        } else {
          mesh.material = mesh.material.clone()
        }
      }
    })
  }

  function applyOpacity(opacity: number) {
    if (!model || opacity === lastAppliedOpacity) return
    cloneMaterials()
    lastAppliedOpacity = opacity
    model.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        const mesh = child as THREE.Mesh
        const materials = Array.isArray(mesh.material)
          ? mesh.material
          : [mesh.material]
        for (const mat of materials) {
          mat.transparent = true
          mat.opacity = opacity
        }
        mesh.castShadow = opacity >= 0.25
      }
    })
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

      floatingTexts = [...floatingTexts, { id: nextTextId++, text, color }]
    }

    // 3. Update floating damage texts
    if (floatingTexts.length > 0 && camera) {
      for (const ref of damageTextRefs) {
        ref?.update(deltaTime, position.x, position.y, position.z, camera)
      }
      floatingTexts = floatingTexts.filter(
        (_, index) => damageTextRefs[index]?.isAlive() !== false
      )
    }

    // 4. Corpse fade
    if (monsterState === 'dead') {
      corpseTimer += deltaTime
      if (corpseTimer >= CORPSE_FADE_START) {
        const fadeProgress =
          (corpseTimer - CORPSE_FADE_START) / CORPSE_FADE_DURATION
        applyOpacity(Math.max(0, 1 - fadeProgress))
      }
    } else {
      corpseTimer = 0
    }

    // 5. Update mixer
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
{#each floatingTexts as text, index (text.id)}
  <DamageText
    bind:this={damageTextRefs[index]}
    text={text.text}
    color={text.color}
  />
{/each}
