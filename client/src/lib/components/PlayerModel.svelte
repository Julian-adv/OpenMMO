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
    retargetOrderedCharacterAnimationsForModel,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import {
    CHARACTER_ANIMATION_PACK_PATHS,
    WARRIOR_CHARACTER_MODEL_PATH,
    KNIGHT_CHARACTER_MODEL_PATH,
    THIEF_CHARACTER_MODEL_PATH,
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
    health: number
    maxHealth: number
    onAttackDuration?: (duration: number) => void
    onDyingFinished?: () => void
    lastDamageInfo?: PlayerDamageInfo
    lastRegenInfo?: PlayerDamageInfo
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
    health,
    maxHealth,
    onAttackDuration,
    onDyingFinished,
    lastDamageInfo,
    lastRegenInfo,
  }: Props = $props()

  let nametagScale = $state(1)
  let nametagHeight = $state(2.7)
  let nametagGroup = $state<THREE.Group | undefined>(undefined)
  let chatBubbleInstance = $state<ChatBubble | null>(null)
  let animDebugInfo = $state('')

  // Floating damage text
  let damageTextRef = $state<ReturnType<typeof DamageText>>()

  // Load animated model (both models are loaded; Threlte caches by URL)
  const warriorGltf = useLoader(GLTFLoader).load(WARRIOR_CHARACTER_MODEL_PATH)
  const knightGltf = useLoader(GLTFLoader).load(KNIGHT_CHARACTER_MODEL_PATH)
  const thiefGltf = useLoader(GLTFLoader).load(THIEF_CHARACTER_MODEL_PATH)
  const locomotionGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.locomotion
  )
  const combatMeleeGltf = useLoader(GLTFLoader).load(
    CHARACTER_ANIMATION_PACK_PATHS.combatMelee
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
  let swordAttached = $state(false)
  const OVERLAP_BEFORE_END = 0.3 // Start next animation overlap 0.3 seconds before current ends
  const ENABLE_SWORD_ATTACHMENT = true
  const IDLE2_Y_OFFSET = -0.16
  let currentAnimationYOffset = $state(0)

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

  function findPrimarySkinnedMesh(
    root: THREE.Object3D
  ): THREE.SkinnedMesh | undefined {
    let primarySkinnedMesh: THREE.SkinnedMesh | undefined
    root.traverse((obj) => {
      if (!(obj instanceof THREE.SkinnedMesh) || !obj.skeleton) return
      if (
        !primarySkinnedMesh ||
        obj.skeleton.bones.length > primarySkinnedMesh.skeleton.bones.length
      ) {
        primarySkinnedMesh = obj
      }
    })
    return primarySkinnedMesh
  }

  function findMainRightHandBone(root: THREE.Object3D): THREE.Bone | undefined {
    const primarySkinnedMesh = findPrimarySkinnedMesh(root)
    if (primarySkinnedMesh) {
      const skeletonBones = primarySkinnedMesh.skeleton.bones
      const byName = new Map(
        skeletonBones.map((bone) => [bone.name.toLowerCase(), bone])
      )
      const preferredBoneNames = [
        'righthand',
        'right_hand',
        'hand_r',
        'hand.r',
        'mixamorig:righthand',
        'mixamorigrighthand',
      ]

      for (const preferredName of preferredBoneNames) {
        const preferredBone = byName.get(preferredName)
        if (preferredBone) return preferredBone
      }

      const matchedSkeletonBone = skeletonBones.find((bone) =>
        isMainRightHandBone(bone)
      )
      if (matchedSkeletonBone) return matchedSkeletonBone
    }

    // Fallback: search entire hierarchy if skeleton lookup was unavailable.
    let fallbackBone: THREE.Bone | undefined
    root.traverse((obj) => {
      if (!(obj instanceof THREE.Bone)) return
      if (!isMainRightHandBone(obj)) return
      fallbackBone = obj
    })
    return fallbackBone
  }

  function prepareSwordMeshes(root: THREE.Object3D): void {
    root.traverse((child) => {
      if (!(child instanceof THREE.Mesh)) return
      child.castShadow = true
      child.receiveShadow = true
      child.visible = true
      child.frustumCulled = false

      const materials = Array.isArray(child.material)
        ? child.material
        : [child.material]
      for (const material of materials) {
        if (!material) continue
        // Right-hand bone chains can include mirrored (negative) scale, which
        // may invert face winding and hide single-sided meshes.
        material.side = THREE.DoubleSide
        material.needsUpdate = true
      }
    })
  }

  function getAverageAbsWorldScale(object: THREE.Object3D): number {
    const worldScale = new THREE.Vector3()
    object.getWorldScale(worldScale)
    const sx = Math.abs(worldScale.x)
    const sy = Math.abs(worldScale.y)
    const sz = Math.abs(worldScale.z)
    const avg = (sx + sy + sz) / 3
    return Number.isFinite(avg) && avg > 0 ? avg : 1
  }

  function tryAttachSword(characterRoot: THREE.Object3D): boolean {
    if (!ENABLE_SWORD_ATTACHMENT || swordAttached || !$swordGltf) return false

    const rightHandBone = findMainRightHandBone(characterRoot)
    if (!rightHandBone) {
      console.warn('Could not find right hand bone for sword attachment')
      return false
    }

    const swordClone = $swordGltf.scene.clone()
    // Use model-authored pivot/orientation, but clear accidental scene offsets.
    swordClone.position.set(0, 0, 0)
    swordClone.rotation.set(0, 0, 0)
    swordClone.scale.set(1, 1, 1)
    prepareSwordMeshes(swordClone)

    rightHandBone.add(swordClone)

    // Keep prop world size stable when armature/hand chain uses non-unit scale.
    const handAvgScale = getAverageAbsWorldScale(rightHandBone)
    if (handAvgScale > 0) {
      swordClone.scale.multiplyScalar(1 / handAvgScale)
    }
    swordAttached = true
    console.log('Sword attached successfully to', rightHandBone.name)
    return true
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
      currentAnimationYOffset = idleIndex === AnimationIndex.IDLE2 ? IDLE2_Y_OFFSET : 0
    } else if (playerState === 'moving') {
      currentAnimationYOffset = 0
      // Lock animation at the start of movement based on movement mode
      if (currentMovementAnimationIndex === undefined) {
        currentMovementAnimationIndex = selectMovementAnimation(movementMode)
      }
      clip = validAnimations[currentMovementAnimationIndex]
    } else if (playerState === 'attack') {
      currentAnimationYOffset = 0
      // Use slash1 animation
      currentMovementAnimationIndex = undefined
      // Find index for slash1 or fallback
      // Assuming AnimationIndex.SLASH1 exists and maps correctly
      clip = validAnimations[AnimationIndex.SLASH1]
    } else if (playerState === 'dead') {
      currentAnimationYOffset = 0
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
    const activeGltf = characterClass === 'warrior' ? $warriorGltf : characterClass === 'thief' ? $thiefGltf : $knightGltf
    if (activeGltf && !mixer && !modelRoot) {
      console.log('Setting up real animation system')

      const { clonedScene: cloned, modelRoot: newModelRoot } =
        createCharacterModelRoot(activeGltf.scene)
      normalizeCharacterModelScale(newModelRoot)

      if (ENABLE_SWORD_ATTACHMENT && !$swordGltf) {
        console.log('Sword GLB not ready yet; will attach when loaded')
      }
      tryAttachSword(cloned)

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
      validAnimations = retargetOrderedCharacterAnimationsForModel(
        newModelRoot,
        orderedSelections,
        {
          base: activeGltf.scene,
          locomotion: $locomotionGltf?.scene,
          combatMelee: $locomotionGltf?.scene,
        }
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
                : 'female_knight.glb'
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
      const activeGltf = characterClass === 'warrior' ? $warriorGltf : characterClass === 'thief' ? $thiefGltf : $knightGltf
      if (activeGltf && animationPacksReady) {
        if (!$locomotionGltf && animationPackTimedOut) {
          console.warn('Locomotion GLB load timeout, using maria animations only')
        }
        if (!$combatMeleeGltf && animationPackTimedOut) {
          console.warn(
            'Combat melee GLB load timeout, using maria/locomotion animations only'
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
      swordAttached = false
    }
  })

  $effect(() => {
    if (!ENABLE_SWORD_ATTACHMENT || swordAttached || !modelRoot || !$swordGltf) {
      return
    }
    tryAttachSword(modelRoot)
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

      // Height: 2.3 to 2.7
      const minHeight = 2.0
      const maxHeight = 2.5

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
    position={[position.x, position.y + currentAnimationYOffset, position.z]}
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

  <!-- Health Bar -->
  {#if isCurrentPlayer}
    <T.Group position.y={-0.3}>
      <!-- Background (black) -->
      <T.Mesh>
        <T.PlaneGeometry args={[1.0, 0.08]} />
        <T.MeshBasicMaterial color="#000000" transparent opacity={0.5} />
      </T.Mesh>
      <!-- Foreground (red) -->
      <T.Mesh
        position.x={-0.5 + (1.0 * Math.max(0, Math.min(1, health / (maxHealth || 1)))) / 2}
        scale.x={Math.max(0.001, Math.min(1, health / (maxHealth || 1)))}
      >
        <T.PlaneGeometry args={[1.0, 0.08]} />
        <T.MeshBasicMaterial color="#ff0000" />
      </T.Mesh>
    </T.Group>
  {/if}

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
  <DamageText bind:this={damageTextRef} {lastDamageInfo} {lastRegenInfo} />
{/if}
