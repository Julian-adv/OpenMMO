<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import type { Vector3 } from 'three'
  import * as THREE from 'three'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'
  import { AnimationIndex, AnimationName } from '../types/animations'
  import {
    LOCOMOTION_WAIT_TIMEOUT_MS,
    createCharacterModelRoot,
    getGltfAnimations,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import { type MovementMode } from '../utils/movementUtils'
  import ChatBubble from './ChatBubble.svelte'
  import DamageText from './DamageText.svelte'
  import type { PlayerDamageInfo } from '../stores/gameStore'

  interface Props {
    position: Vector3
    name: string
    isCurrentPlayer: boolean
    playerState: 'idle' | 'moving' | 'attack' | 'dead'
    attackCounter?: number
    speed: number
    rotation: number
    movementMode?: MovementMode
    camera: THREE.Camera | undefined
    chatBubble?: string
    onAttackDuration?: (duration: number) => void
    onDyingFinished?: () => void
    lastDamageInfo?: PlayerDamageInfo
  }

  let {
    position,
    name,
    isCurrentPlayer,
    playerState,
    attackCounter,
    speed: _speed,
    rotation,
    movementMode,
    camera,
    chatBubble,
    onAttackDuration,
    onDyingFinished,
    lastDamageInfo,
  }: Props = $props()

  let nametagScale = $state(1)
  let nametagHeight = $state(2.2)
  let nametagGroup = $state<THREE.Group | undefined>(undefined)
  let chatBubbleInstance = $state<ChatBubble | null>(null)
  let animDebugInfo = $state('')

  // Floating damage text
  let damageTextRef = $state<ReturnType<typeof DamageText>>()

  // Load animated model
  const gltf = useLoader(GLTFLoader).load('/models/maria.glb')
  const locomotionGltf = useLoader(GLTFLoader).load('/models/animations/locomotion.glb')

  // Load sword model
  const swordGltf = useLoader(GLTFLoader).load('/models/sword.glb')

  // Animation system - following gpt-all-in-one.html approach
  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  // Clock removed, using passed deltaTime

  let validAnimations = $state<THREE.AnimationClip[]>([])
  let lastPlayerState = $state<
    'idle' | 'moving' | 'attack' | 'dead' | undefined
  >(undefined)
  let lastAttackCounter = $state(0)
  let dyingFinishedNotified = $state(false)
  let currentMovementAnimationIndex = $state<number | undefined>(undefined) // Locked animation for current movement
  const OVERLAP_BEFORE_END = 0.3 // Start next animation overlap 0.3 seconds before current ends

  // Select movement animation based on movement mode
  function selectMovementAnimation(mode: MovementMode | undefined): number {
    if (mode === 'walk') return AnimationIndex.WALK
    if (mode === 'jog') return AnimationIndex.JOG
    if (mode === 'run') return AnimationIndex.RUN
    return AnimationIndex.JOG // Default fallback
  }

  function playAnimationForState() {
    // Check if mixer and animations are available
    if (!mixer || validAnimations.length === 0) return

    // Select animation based on player state and mode
    let clip: THREE.AnimationClip | undefined
    if (playerState === 'idle') {
      // Reset movement animation lock when idle
      currentMovementAnimationIndex = undefined
      // Randomly select between idle animations
      const idleIndices = [
        AnimationIndex.IDLE1,
        AnimationIndex.IDLE2,
        AnimationIndex.IDLE3,
        AnimationIndex.IDLE4,
      ]
      const idleIndex =
        idleIndices[Math.floor(Math.random() * idleIndices.length)]
      clip = validAnimations[idleIndex]
    } else if (playerState === 'moving') {
      // Lock animation at the start of movement based on movement mode
      if (currentMovementAnimationIndex === undefined) {
        currentMovementAnimationIndex = selectMovementAnimation(movementMode)
      }
      clip = validAnimations[currentMovementAnimationIndex]
    } else if (playerState === 'attack') {
      // Use slash1 animation
      currentMovementAnimationIndex = undefined
      // Find index for slash1 or fallback
      // Assuming AnimationIndex.SLASH1 exists and maps correctly
      clip = validAnimations[AnimationIndex.SLASH1]
    } else if (playerState === 'dead') {
      currentMovementAnimationIndex = undefined
      dyingFinishedNotified = false
      clip = validAnimations[AnimationIndex.DYING]
    } else {
      return // Unknown state
    }

    if (!clip) return

    const newAction = mixer.clipAction(clip)

    // Setup new action
    newAction.reset()
    newAction.loop =
      playerState === 'idle' ||
      playerState === 'attack' ||
      playerState === 'dead'
        ? THREE.LoopOnce
        : THREE.LoopRepeat
    newAction.clampWhenFinished =
      playerState === 'idle' ||
      playerState === 'attack' ||
      playerState === 'dead'
    newAction.paused = false

    // If there's a current action and it's different, crossfade to the new one
    if (currentAction && newAction !== currentAction) {
      const crossfadeDuration = 0.3 // 300ms crossfade

      // Use THREE.js built-in crossfade
      newAction.crossFadeFrom(currentAction, crossfadeDuration, true)
    }

    // Play the new action
    newAction.play()
    currentAction = newAction
  }

  function setupRealAnimation() {
    if ($gltf && !mixer && !modelRoot) {
      console.log('Setting up real animation system')

      const { clonedScene: cloned, modelRoot: newModelRoot } =
        createCharacterModelRoot($gltf.scene)

      // Attach sword to right hand if sword model is loaded
      if ($swordGltf) {
        console.log('Attaching sword to hand')

        // Find the right hand bone (main hand bone, not finger bones)
        let rightHandBone: THREE.Bone | undefined
        cloned.traverse((obj) => {
          if (obj instanceof THREE.Bone) {
            const boneName = obj.name.toLowerCase()
            // Match only the main hand bone, exclude finger bones (thumb, index, middle, ring, pinky)
            if (
              (boneName.includes('righthand') ||
                boneName.includes('right_hand') ||
                boneName.includes('hand_r') ||
                boneName.includes('hand.r')) &&
              !boneName.includes('thumb') &&
              !boneName.includes('index') &&
              !boneName.includes('middle') &&
              !boneName.includes('ring') &&
              !boneName.includes('pinky')
            ) {
              console.log(`Found main right hand bone: ${obj.name}`)
              rightHandBone = obj
            }
          }
        })

        if (rightHandBone) {
          // Clone the sword model
          const swordClone = $swordGltf.scene.clone()

          // Debug: Log sword model info
          console.log('Sword model info:', {
            position: swordClone.position,
            rotation: swordClone.rotation,
            scale: swordClone.scale,
            children: swordClone.children.length,
          })

          // Debug: Log all meshes in sword
          let meshCount = 0
          swordClone.traverse((child) => {
            if (child instanceof THREE.Mesh) {
              meshCount++
              console.log('Sword mesh:', {
                name: child.name,
                geometry: child.geometry,
                material: child.material,
                visible: child.visible,
              })
              child.castShadow = true
              child.receiveShadow = true
            }
          })
          console.log(`Total sword meshes: ${meshCount}`)

          // Adjust sword position and rotation to fit in hand
          // Try much larger scale to make it visible (sword might be very small)
          // swordClone.position.set(0, 0.1, 0)
          // swordClone.rotation.set(-Math.PI / 2, 0, 0)
          // swordClone.scale.set(100, 100, 100)

          // Attach sword to hand bone
          rightHandBone.add(swordClone)
          console.log('Sword attached successfully to', rightHandBone.name)
          console.log('Hand bone position:', rightHandBone.position)
        } else {
          console.warn('Could not find right hand bone')
          // Log all bone names to help with debugging
          const boneNames: string[] = []
          cloned.traverse((obj) => {
            if (obj instanceof THREE.Bone) {
              boneNames.push(obj.name)
            }
          })
          console.log('Available bones:', boneNames)
        }
      }

      const baseAnimations = getGltfAnimations($gltf)
      const locomotionAnimations = getGltfAnimations($locomotionGltf)

      console.log(`Found ${baseAnimations.length} base animation clips`)
      console.log(`Found ${locomotionAnimations.length} locomotion animation clips`)

      // Collect all node names in the cloned model
      const modelNodeNames = new SvelteSet()
      cloned.traverse((obj) => {
        if (obj.name) modelNodeNames.add(obj.name)
      })
      console.log(`Model has ${modelNodeNames.size} named nodes`)
      console.log('Model node names:', Array.from(modelNodeNames).slice(0, 10))

      const orderedSelections = selectOrderedCharacterAnimations(
        baseAnimations,
        locomotionAnimations
      )
      validAnimations = orderedSelections.map(({ clip }) => clip)

      for (const selection of orderedSelections) {
        if (selection.fromFallback) {
          console.log(`❌ Missing animation: ${selection.name} (using fallback)`)
        } else {
          const source =
            selection.source === 'locomotion' ? 'locomotion.glb' : 'maria.glb'
          console.log(`✅ Found animation: ${selection.name} (${source})`)
        }

        if (selection.name === AnimationName.SLASH1 && onAttackDuration) {
          onAttackDuration(selection.clip.duration)
        }
      }

      console.log(`Found ${validAnimations.length} valid animations`)

      if (validAnimations.length > 0) {
        // Setup mixer
        mixer = new THREE.AnimationMixer(newModelRoot)

        // Play appropriate animation based on isMoving state
        playAnimationForState()
      } else {
        console.warn('No suitable animations found with strict filtering')

        // Fallback: try to play any animation without filtering
        const fallbackAnimations =
          baseAnimations.length > 0 ? baseAnimations : locomotionAnimations
        if (fallbackAnimations.length > 0) {
          console.log(
            'Trying fallback: playing first animation without filtering'
          )
          mixer = new THREE.AnimationMixer(newModelRoot)
          const clip = fallbackAnimations[0]
          console.log(
            `Playing fallback animation: ${clip.name}, duration: ${clip.duration}s`
          )

          currentAction = mixer.clipAction(clip)
          currentAction.reset()
          currentAction.loop = THREE.LoopRepeat
          currentAction.paused = false
          currentAction.play()
        } else {
          console.log('No animations available at all')
        }
      }

      modelRoot = newModelRoot
    }
  }

  onMount(() => {
    // Wait for GLTF to load and setup real animation
    const waitStartTime = Date.now()
    const checkGltf = () => {
      const locomotionTimedOut =
        Date.now() - waitStartTime >= LOCOMOTION_WAIT_TIMEOUT_MS
      if ($gltf && ($locomotionGltf || locomotionTimedOut)) {
        if (!$locomotionGltf && locomotionTimedOut) {
          console.warn('Locomotion GLB load timeout, using maria animations only')
        }
        setupRealAnimation()
      } else {
        setTimeout(checkGltf, 100)
      }
    }
    checkGltf()

    // Cleanup on unmount
    return () => {
      if (mixer) {
        mixer.stopAllAction()
        mixer = null
      }
      if (modelRoot) {
        modelRoot = null
      }
    }
  })

  // Function to update mixer and animation state and nametag - called from GameScene gameLoop
  export function update(deltaTime: number) {
    // Update nametag logic (formerly in useTask)
    if (camera && nametagGroup) {
      const nametagPos = new THREE.Vector3(
        position.x,
        position.y + 2.2,
        position.z
      )
      const dist = camera.position.distanceTo(nametagPos)

      // Min distance (zoom in) = 5
      // Max distance (zoom out) = 20
      const minDist = 5
      const maxDist = 20

      // Scale: 0.5 to 1.0
      const minScale = 0.5
      const maxScale = 1.0

      // Height: 1.8 to 2.2
      const minHeight = 1.8
      const maxHeight = 2.2

      let t = (dist - minDist) / (maxDist - minDist)
      t = Math.max(0, Math.min(1, t)) // Clamp between 0 and 1

      nametagScale = minScale + t * (maxScale - minScale)
      nametagHeight = minHeight + t * (maxHeight - minHeight)

      // Update nametag group transform
      nametagGroup.position.set(
        position.x,
        position.y + nametagHeight,
        position.z
      )
      nametagGroup.scale.set(nametagScale, nametagScale, nametagScale)
      nametagGroup.quaternion.copy(camera.quaternion)
    }

    // Update floating damage texts
    if (camera) {
      damageTextRef?.update(
        deltaTime,
        position.x,
        position.y,
        position.z,
        camera
      )
    }

    if (chatBubbleInstance) {
      chatBubbleInstance.update()
    }

    if (!mixer) return

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

    // Update mixer with provided deltaTime
    if (currentAction) {
      mixer.update(deltaTime)

      const clip = currentAction.getClip()
      if (clip && clip.duration > 0) {
        // Calculate remaining time (without modulo)
        const remainingTime = clip.duration - currentAction.time

        // Trigger next animation once when conditions are met (0.3 seconds remaining)
        if (remainingTime <= OVERLAP_BEFORE_END && playerState === 'idle') {
          playAnimationForState()
          return // Early return to prevent duplicate calls below
        }
      }
    }

    if (playerState !== 'dead') {
      dyingFinishedNotified = false
    } else if (
      isCurrentPlayer &&
      onDyingFinished &&
      !dyingFinishedNotified &&
      currentAction
    ) {
      const clip = currentAction.getClip()
      if (
        clip.name === AnimationName.DYING &&
        currentAction.time >= clip.duration - 0.001
      ) {
        dyingFinishedNotified = true
        onDyingFinished()
      }
    }

    // Update animation state
    if (validAnimations.length > 0) {
      // Only update animation if the player state has changed or attack counter increased
      // Note: idle transitions are handled above by OVERLAP_BEFORE_END logic
      if (
        lastPlayerState !== playerState ||
        (playerState === 'attack' && lastAttackCounter !== attackCounter)
      ) {
        lastPlayerState = playerState
        if (attackCounter !== undefined) lastAttackCounter = attackCounter
        playAnimationForState()
      }
    }
  }
</script>

<!-- Character Model -->
{#if modelRoot}
<T.Group
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
  >
    <!-- 3D Character Model with real animations -->
    <T is={modelRoot} />
  </T.Group>
{/if}

<!-- Name tag (separate from character to avoid rotation inheritance) -->
<T.Group bind:ref={nametagGroup}>
  <Text
    text={name}
    fontSize={0.3}
    color={isCurrentPlayer ? '#4299e1' : '#ffffff'}
    anchorX="center"
    anchorY="middle"
  />
  {#if animDebugInfo}
    <Text
      text={animDebugInfo}
      fontSize={0.2}
      color="#ffff00"
      position.y={0.4}
      anchorX="center"
      anchorY="middle"
    />
  {/if}
</T.Group>

<!-- Chat bubble (appears above player when they send a message) -->
{#if chatBubble}
  <ChatBubble
    bind:this={chatBubbleInstance}
    {position}
    {camera}
    message={chatBubble}
  />
{/if}

<!-- Floating Damage Text -->
{#if isCurrentPlayer}
  <DamageText bind:this={damageTextRef} {lastDamageInfo} />
{/if}
