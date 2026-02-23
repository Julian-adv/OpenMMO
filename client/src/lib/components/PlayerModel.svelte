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
    normalizeCharacterModelScale,
    retargetAnimationsForCharacterModel,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import {
    CHARACTER_ANIMATION_SOURCE_MODEL_PATH,
    CHARACTER_ANIMATION_PACK_PATHS,
    WARRIOR_CHARACTER_MODEL_PATH,
    KNIGHT_CHARACTER_MODEL_PATH,
  } from '../utils/modelPaths'
  import type { CharacterClass } from '../network/networkTypes'
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
    characterClass: CharacterClass
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
    characterClass,
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

  // Load animated model (both models are loaded; Threlte caches by URL)
  const warriorGltf = useLoader(GLTFLoader).load(WARRIOR_CHARACTER_MODEL_PATH)
  const knightGltf = useLoader(GLTFLoader).load(KNIGHT_CHARACTER_MODEL_PATH)
  const locomotionGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.locomotion
  )
  const combatMeleeGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.combatMelee
  )
  const retargetSourceGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_SOURCE_MODEL_PATH
  )

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
  const MIN_SAFE_SCALE_COMPONENT = 0.0001
  const MIN_SWORD_SCALE_COMPENSATION = 0.25
  const MAX_SWORD_SCALE_COMPENSATION = 4
  const ENABLE_SWORD_SCALE_COMPENSATION = true

  function isMainRightHandBone(bone: THREE.Bone): boolean {
    const boneName = bone.name.toLowerCase()
    return (
      (boneName.includes('righthand') ||
        boneName.includes('right_hand') ||
        boneName.includes('hand_r') ||
        boneName.includes('hand.r')) &&
      !boneName.includes('thumb') &&
      !boneName.includes('index') &&
      !boneName.includes('middle') &&
      !boneName.includes('ring') &&
      !boneName.includes('pinky')
    )
  }

  function findMainRightHandBone(root: THREE.Object3D): THREE.Bone | undefined {
    let rightHandBone: THREE.Bone | undefined
    root.traverse((obj) => {
      if (!(obj instanceof THREE.Bone)) return
      if (!isMainRightHandBone(obj)) return
      rightHandBone = obj
    })
    return rightHandBone
  }

  function getObjectChainScale(object: THREE.Object3D): THREE.Vector3 {
    const chainScale = new THREE.Vector3(1, 1, 1)
    let current: THREE.Object3D | null = object

    while (current) {
      chainScale.x *= Math.abs(current.scale.x)
      chainScale.y *= Math.abs(current.scale.y)
      chainScale.z *= Math.abs(current.scale.z)
      current = current.parent
    }

    return chainScale
  }

  function getObjectHeight(object: THREE.Object3D): number {
    object.updateMatrixWorld(true)
    const bounds = new THREE.Box3().setFromObject(object)
    if (bounds.isEmpty()) return 0
    const size = new THREE.Vector3()
    bounds.getSize(size)
    return Number.isFinite(size.y) ? size.y : 0
  }

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
    const activeGltf = characterClass === 'warrior' ? $warriorGltf : $knightGltf
    if (activeGltf && !mixer && !modelRoot) {
      console.log('Setting up real animation system')

      const { clonedScene: cloned, modelRoot: newModelRoot } =
        createCharacterModelRoot(activeGltf.scene)

      // Attach sword to right hand if sword model is loaded
      if ($swordGltf) {
        console.log('Attaching sword to hand')

        const rightHandBone = findMainRightHandBone(cloned)
        if (rightHandBone) {
          console.log(`Found main right hand bone: ${rightHandBone.name}`)
        }

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

          // Keep sword size consistent across rigs by compensating cumulative
          // scale differences against the maria reference rig.
          const targetHandChainScale = getObjectChainScale(rightHandBone)
          const targetModelHeight = getObjectHeight(cloned)

          const swordScaleReferenceScene = $warriorGltf?.scene
          const sourceHandBone =
            swordScaleReferenceScene &&
            findMainRightHandBone(swordScaleReferenceScene)
          if (
            ENABLE_SWORD_SCALE_COMPENSATION &&
            sourceHandBone &&
            swordScaleReferenceScene
          ) {
            const sourceHandChainScale = getObjectChainScale(sourceHandBone)
            const sourceModelHeight = getObjectHeight(swordScaleReferenceScene)
            const modelHeightCompensation =
              sourceModelHeight > MIN_SAFE_SCALE_COMPONENT
                ? targetModelHeight / sourceModelHeight
                : 1

            const compensationX = THREE.MathUtils.clamp(
              (sourceHandChainScale.x /
                Math.max(
                  targetHandChainScale.x,
                  MIN_SAFE_SCALE_COMPONENT
                )) *
                modelHeightCompensation,
              MIN_SWORD_SCALE_COMPENSATION,
              MAX_SWORD_SCALE_COMPENSATION
            )
            const compensationY = THREE.MathUtils.clamp(
              (sourceHandChainScale.y /
                Math.max(
                  targetHandChainScale.y,
                  MIN_SAFE_SCALE_COMPONENT
                )) *
                modelHeightCompensation,
              MIN_SWORD_SCALE_COMPENSATION,
              MAX_SWORD_SCALE_COMPENSATION
            )
            const compensationZ = THREE.MathUtils.clamp(
              (sourceHandChainScale.z /
                Math.max(
                  targetHandChainScale.z,
                  MIN_SAFE_SCALE_COMPONENT
                )) *
                modelHeightCompensation,
              MIN_SWORD_SCALE_COMPENSATION,
              MAX_SWORD_SCALE_COMPENSATION
            )

            swordClone.scale.set(
              swordClone.scale.x * compensationX,
              swordClone.scale.y * compensationY,
              swordClone.scale.z * compensationZ
            )
          }

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

      const baseAnimations = getGltfAnimations(activeGltf)
      const locomotionAnimations = getGltfAnimations($locomotionGltf)
      const combatMeleeAnimations = getGltfAnimations($combatMeleeGltf)

      console.log(`Found ${baseAnimations.length} base animation clips`)
      console.log(`Found ${locomotionAnimations.length} locomotion animation clips`)
      console.log(
        `Found ${combatMeleeAnimations.length} combat melee animation clips`
      )

      // Collect all node names in the cloned model
      const modelNodeNames = new SvelteSet()
      cloned.traverse((obj) => {
        if (obj.name) modelNodeNames.add(obj.name)
      })
      console.log(`Model has ${modelNodeNames.size} named nodes`)
      console.log('Model node names:', Array.from(modelNodeNames).slice(0, 10))

      const orderedSelections = selectOrderedCharacterAnimations(
        baseAnimations,
        locomotionAnimations,
        combatMeleeAnimations
      )
      validAnimations = retargetAnimationsForCharacterModel(
        newModelRoot,
        $retargetSourceGltf?.scene,
        orderedSelections.map(({ clip }) => clip)
      )

      for (const selection of orderedSelections) {
        if (selection.fromFallback) {
          console.log(`❌ Missing animation: ${selection.name} (using fallback)`)
        } else {
          const source =
            selection.source === 'locomotion'
              ? 'locomotion.glb'
              : selection.source === 'combat_melee'
                ? 'combat_melee.glb'
                : 'maria.glb'
          console.log(`✅ Found animation: ${selection.name} (${source})`)
        }

        if (selection.name === AnimationName.SLASH1 && onAttackDuration) {
          onAttackDuration(selection.clip.duration)
        }
      }

      console.log(`Found ${validAnimations.length} valid animations`)

      if (validAnimations.length > 0) {
        try {
          // Setup mixer
          mixer = new THREE.AnimationMixer(newModelRoot)

          // Play appropriate animation based on isMoving state
          playAnimationForState()
        } catch (error) {
          console.warn('Failed to start player animation clips', error)
          if (mixer) {
            mixer.stopAllAction()
            mixer = null
          }
          currentAction = null
          validAnimations = []
        }
      } else {
        console.warn('No suitable animations found with strict filtering')

        // Fallback: try to play any animation without filtering
        const fallbackAnimations =
          baseAnimations.length > 0
            ? baseAnimations
            : combatMeleeAnimations.length > 0
              ? combatMeleeAnimations
              : locomotionAnimations
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

      normalizeCharacterModelScale(newModelRoot)
      modelRoot = newModelRoot
    }
  }

  onMount(() => {
    // Wait for GLTF to load and setup real animation
    const waitStartTime = Date.now()
    const checkGltf = () => {
      const animationPackTimedOut =
        Date.now() - waitStartTime >= LOCOMOTION_WAIT_TIMEOUT_MS
      const animationPacksReady =
        ($locomotionGltf && $combatMeleeGltf) || animationPackTimedOut
      const retargetSourceReady = !!$retargetSourceGltf || animationPackTimedOut
      const activeGltf = characterClass === 'warrior' ? $warriorGltf : $knightGltf
      if (activeGltf && animationPacksReady && retargetSourceReady) {
        if (!$locomotionGltf && animationPackTimedOut) {
          console.warn('Locomotion GLB load timeout, using maria animations only')
        }
        if (!$combatMeleeGltf && animationPackTimedOut) {
          console.warn(
            'Combat melee GLB load timeout, using maria/locomotion animations only'
          )
        }
        if (!$retargetSourceGltf && animationPackTimedOut) {
          console.warn(
            'Retarget source GLB load timeout, using non-retargeted animation clips'
          )
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
