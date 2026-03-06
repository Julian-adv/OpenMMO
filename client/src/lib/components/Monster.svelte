<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import TextLabel from './TextLabel.svelte'
  import { SkeletonUtils, GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as THREE from 'three'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'
  import DamageText from './DamageText.svelte'

  import type { MonsterData } from '../types/Monster'
  import { getMonsterDef } from '../data/monsterDefs'

  interface Props {
    position: { x: number; y: number; z: number }
    rotation: number
    monsterState: MonsterData['state']
    id: string
    type: string
    lastDamageInfo?: MonsterData['lastDamageInfo']
  }

  let { position, rotation, monsterState, id, type, lastDamageInfo }: Props =
    $props()

  const def = $derived(getMonsterDef(type))

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
  let damageTextRef = $state<ReturnType<typeof DamageText>>()
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

    let clipName = def?.animIdle ?? 'Idle'
    if (monsterState === 'walk') clipName = def?.animWalk ?? 'Walk'
    if (monsterState === 'run') clipName = def?.animRun ?? 'Run'
    if (monsterState === 'attack') clipName = def?.animAttack ?? 'Attack'
    if (monsterState === 'hit') clipName = def?.animHit ?? 'Hit'
    if (monsterState === 'dead') {
      clipName = isDeadAnimationFinished
        ? (def?.animDead ?? 'Dead')
        : (def?.animDie ?? 'Die')
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
    // 0. Sync Three.js group position imperatively so the refraction render
    //    (which runs during the game loop, before Svelte's reactive updates)
    //    sees the monster at its current position.
    if (group) {
      group.position.set(position.x, position.y, position.z)
      group.rotation.y = rotation
    }

    // 1. Sync animation with state
    if (
      lastMonsterState !== monsterState ||
      lastDeadAnimFinished !== isDeadAnimationFinished
    ) {
      lastMonsterState = monsterState
      lastDeadAnimFinished = isDeadAnimationFinished
      playAnimation()
    }

    // 2. Update damage texts
    if (camera) {
      damageTextRef?.update(
        deltaTime,
        position.x,
        position.y,
        position.z,
        camera
      )
    }

    // 3. Corpse fade
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
          if (e.action.getClip().name === (def?.animDie ?? 'Die')) {
            isDeadAnimationFinished = true
          }
        })
      }
    }
  })

  // Export the model group for raycasting from parent
  export function getMeshGroup() {
    return group
  }

  export function getNametagGroup() {
    return nametagGroup
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
    <TextLabel
      text={id}
      fontSize={0.2}
      color="#ffffff"
      position={[0, 0.3, 0]}
      anchorX="center"
      anchorY="middle"
    />
    <TextLabel
      text={animDebugInfo}
      fontSize={0.2}
      color="#ffff00"
      position={[0, 0.6, 0]}
      anchorX="center"
      anchorY="middle"
    />
  {/if}
</T.Group>

<!-- Floating Damage Text -->
<DamageText bind:this={damageTextRef} {lastDamageInfo} />
