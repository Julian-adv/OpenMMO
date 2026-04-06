<script lang="ts">
  import { T } from '@threlte/core'
  import TextLabel from './TextLabel.svelte'
  import type { Vector3 } from 'three'
  import * as THREE from 'three'
  import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { onMount } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { get } from 'svelte/store'
  import { timeScale } from '../stores/timeStore'
  import { AnimationIndex, AnimationName } from '../types/animations'
  import {
    createCharacterModelRoot,
    getGltfAnimations,
    retargetOrderedCharacterAnimationsForModel,
    selectOrderedCharacterAnimations,
  } from '../utils/characterAnimationUtils'
  import {
    CHARACTER_ANIMATION_PACK_PATHS,
    getCharacterModelPath,
    getDefaultWeaponModel,
    getWeaponModelPath,
  } from '../utils/modelPaths'
  import { loadGLB } from '../utils/gltfCache'
  import { inventoryStore } from '../stores/inventoryStore'
  import { getItemDef } from '../data/itemDefs'
  import type { CharacterClass, Gender } from '../network/networkTypes'
  import { type MovementMode } from '../utils/movementUtils'
  import ChatBubble from './ChatBubble.svelte'
  import DamageText from './DamageText.svelte'
  import type { PlayerDamageInfo } from '../stores/gameStore'
  import { torchLightEnabled } from '../stores/debugStore'
  import { applyTorchFlicker, TORCH_BASE_INTENSITY, TORCH_BASE_DISTANCE, TORCH_BASE_DECAY, TORCH_BASE_POSITION } from '../utils/torchFlicker'

  export type TorchMode = 'local' | 'shadow' | 'light-only' | 'off'

  interface Props {
    position: Vector3
    name: string
    isCurrentPlayer: boolean
    playerState: 'idle' | 'moving' | 'attack' | 'dead' | 'interact'
    interactionAnim?: string
    interactOffsetY?: number
    attackCounter?: number
    speed: number
    rotation: number
    movementMode?: MovementMode
    camera: THREE.Camera | undefined
    chatBubble?: string
    characterClass: CharacterClass
    gender: Gender
    health: number
    maxHealth: number
    onAttackDuration?: (duration: number) => void
    onDyingFinished?: () => void
    isLoading?: boolean
    lastDamageInfo?: PlayerDamageInfo
    lastRegenInfo?: PlayerDamageInfo
    torchMode?: TorchMode
  }

  let {
    position,
    name,
    isCurrentPlayer,
    playerState,
    interactionAnim,
    interactOffsetY = 0,
    attackCounter,
    speed: _speed,
    rotation,
    movementMode,
    camera,
    chatBubble,
    characterClass,
    gender,
    health,
    maxHealth,
    onAttackDuration,
    onDyingFinished,
    isLoading = $bindable(false),
    lastDamageInfo,
    lastRegenInfo,
    torchMode = 'off',
  }: Props = $props()

  let nametagScale = $state(1)
  let nametagHeight = $state(2.7)
  let nametagGroup = $state<THREE.Group | undefined>(undefined)
  let chatBubbleInstance = $state<ChatBubble | null>(null)
  let animDebugInfo = $state('')

  // Floating damage text
  let damageTextRef = $state<ReturnType<typeof DamageText>>()

  // Blob shadow for remote torch (shared across instances)
  const BLOB_SHADOW_RADIUS = 0.45
  const blobShadowGeometry = new THREE.CircleGeometry(BLOB_SHADOW_RADIUS, 16)
  const blobShadowMaterial = new THREE.MeshBasicMaterial({
    color: 0x000000,
    transparent: true,
    opacity: 0.35,
    depthWrite: false,
    polygonOffset: true,
    polygonOffsetFactor: -1,
    polygonOffsetUnits: -1,
  })

  // Torch light flickering
  let torchLight = $state<THREE.PointLight | undefined>(undefined)
  let torchFlickerTime = 0

  // Load only the active character model + shared animation packs via shared cache.
  // This cache persists across Threlte Canvas lifecycles, so GLBs loaded in
  // character select don't re-download when entering the game scene.
  let activeGltfData = $state<GLTF | null>(null)
  let locomotionGltfData = $state<GLTF | null>(null)
  let combatMeleeGltfData = $state<GLTF | null>(null)
  let weaponGltfData = $state<GLTF | null>(null)

  // svelte-ignore state_referenced_locally
  const defaultWeaponModel = getDefaultWeaponModel(characterClass)
  // svelte-ignore state_referenced_locally
  const modelPath = getCharacterModelPath(characterClass, gender)
  const modelPromise = loadGLB(modelPath).then((g) => { activeGltfData = g })
  const locomotionPromise = loadGLB(CHARACTER_ANIMATION_PACK_PATHS.locomotion).then((g) => { locomotionGltfData = g })
  const combatMeleePromise = loadGLB(CHARACTER_ANIMATION_PACK_PATHS.combatMelee).then((g) => { combatMeleeGltfData = g })
  const weaponPromise = defaultWeaponModel
    ? loadGLB(getWeaponModelPath(defaultWeaponModel)).then((g) => { weaponGltfData = g })
    : Promise.resolve()
  const glbReady = Promise.all([modelPromise, locomotionPromise, combatMeleePromise, weaponPromise])

  // Animation system - following gpt-all-in-one.html approach
  let mixer = $state<THREE.AnimationMixer | null>(null)
  let currentAction = $state<THREE.AnimationAction | null>(null)
  let modelRoot = $state<THREE.Group | null>(null)
  let modelGroup = $state<THREE.Group | undefined>(undefined)
  // Clock removed, using passed deltaTime

  let clonedScene: THREE.Object3D | null = null
  let footOffsetApplied = false
  let validAnimations = $state<THREE.AnimationClip[]>([])
  let socialClipsByName = new SvelteMap<string, THREE.AnimationClip>()
  let socialLoading = false
  let lastPlayerState = $state<
    'idle' | 'moving' | 'attack' | 'dead' | 'interact' | undefined
  >(undefined)
  let lastAttackCounter = $state(0)
  let dyingFinishedNotified = $state(false)
  let currentMovementAnimationIndex = $state<number | undefined>(undefined) // Locked animation for current movement
  let weaponAttached = $state(false)
  let weaponObject: THREE.Object3D | null = null
  const OVERLAP_BEFORE_END = 0.3 // Start next animation overlap 0.3 seconds before current ends
  const _nametagPos = new THREE.Vector3()

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

  function attachWeaponModel(gltfScene: THREE.Object3D, characterRoot: THREE.Object3D): boolean {
    const rightHandBone = findMainRightHandBone(characterRoot)
    if (!rightHandBone) {
      console.warn('Could not find right hand bone for weapon attachment')
      return false
    }

    weaponObject = gltfScene.clone()
    // Offset from wrist bone toward palm so weapon looks gripped
    weaponObject.position.set(0, 0.08, 0)
    rightHandBone.add(weaponObject)
    weaponAttached = true
    return true
  }

  function tryAttachWeapon(characterRoot: THREE.Object3D): boolean {
    if (!defaultWeaponModel || weaponAttached || !weaponGltfData) return false
    return attachWeaponModel(weaponGltfData.scene, characterRoot)
  }

  function detachWeapon() {
    if (weaponObject && weaponObject.parent) {
      weaponObject.parent.remove(weaponObject)
    }
    weaponObject = null
    weaponAttached = false
  }

  const equippedMainHandItemId = $derived(
    isCurrentPlayer
      ? ($inventoryStore.equipped.main_hand?.item_def_id ?? null)
      : null
  )

  let attachedWeaponItemId: string | null = null
  let weaponAttachGeneration = 0

  $effect(() => {
    const itemDefId = equippedMainHandItemId
    // Read modelRoot so effect re-runs when model finishes loading
    const root = modelRoot
    if (!root || !clonedScene) return

    if (itemDefId === attachedWeaponItemId) return

    detachWeapon()
    attachedWeaponItemId = null

    if (!itemDefId) return

    const itemDef = getItemDef(itemDefId)
    if (!itemDef?.worldModel) return

    const gen = ++weaponAttachGeneration
    const weaponModelPath = getWeaponModelPath(itemDef.worldModel)
    loadGLB(weaponModelPath).then((gltf) => {
      if (gen !== weaponAttachGeneration || !clonedScene) return

      attachWeaponModel(gltf.scene, clonedScene)
      attachedWeaponItemId = itemDefId
    })
  })

  // Select movement animation based on movement mode
  function selectMovementAnimation(mode: MovementMode | undefined): number {
    if (mode === 'walk') return AnimationIndex.WALK
    if (mode === 'jog') return AnimationIndex.JOG
    if (mode === 'run') return AnimationIndex.RUN
    return AnimationIndex.JOG // Default fallback
  }

  async function loadSocialAnimations() {
    if (socialLoading || socialClipsByName.size > 0) return
    socialLoading = true
    try {
      const socialGltf = await loadGLB(CHARACTER_ANIMATION_PACK_PATHS.social)
      const socialRawClips = getGltfAnimations(socialGltf)
      for (const clip of socialRawClips) {
        socialClipsByName.set(clip.name, clip)
      }
    } finally {
      socialLoading = false
    }
    if (mixer && playerState === 'interact') playAnimationForState()
  }

  function playAnimationForState() {
    // Check if mixer and animations are available
    if (!mixer || validAnimations.length === 0) return

    // Hide weapon during interact animations
    if (weaponObject) {
      weaponObject.visible = playerState !== 'interact'
    }

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
        AnimationIndex.IDLE5,
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
    } else if (playerState === 'interact') {
      currentMovementAnimationIndex = undefined
      clip = interactionAnim
        ? socialClipsByName.get(interactionAnim)
        : undefined
      if (!clip) {
        loadSocialAnimations()
        return
      }
    } else {
      return // Unknown state
    }

    if (!clip) return

    const newAction = mixer.clipAction(clip)

    const playOnce = playerState !== 'moving'
    newAction.reset()
    newAction.loop = playOnce ? THREE.LoopOnce : THREE.LoopRepeat
    newAction.clampWhenFinished = playOnce
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

  async function setupRealAnimation() {
    const activeGltf = activeGltfData
    if (activeGltf && !mixer && !modelRoot) {
      console.log('Setting up real animation system')

      const { clonedScene: cloned, modelRoot: newModelRoot } =
        createCharacterModelRoot(activeGltf.scene)

      // For current player, weapon is reactively managed by $effect watching inventory.
      // For other players, attach based on character class.
      if (!isCurrentPlayer) tryAttachWeapon(cloned)

      const baseAnimations = getGltfAnimations(activeGltf)
      const locomotionAnimations = getGltfAnimations(locomotionGltfData)
      const combatMeleeAnimations = getGltfAnimations(combatMeleeGltfData)

      console.log(`Found ${baseAnimations.length} base animation clips`)
      console.log(`Found ${locomotionAnimations.length} locomotion animation clips`)
      console.log(
        `Found ${combatMeleeAnimations.length} combat melee animation clips`
      )

      // Collect all node names in the cloned model
      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const modelNodeNames = new Set()
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
      validAnimations = await retargetOrderedCharacterAnimationsForModel(
        newModelRoot,
        orderedSelections,
        {
          base: activeGltf.scene,
          locomotion: locomotionGltfData?.scene,
          combatMelee: combatMeleeGltfData?.scene,
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

      clonedScene = cloned
      modelRoot = newModelRoot
    }
  }

  onMount(() => {
    // Wait for all GLTFs (character model + animation packs) to load
    isLoading = true
    glbReady.then(() => setupRealAnimation()).then(() => {
      isLoading = false
    })

    // Cleanup on unmount
    return () => {
      if (mixer) {
        mixer.stopAllAction()
        mixer = null
      }
      if (modelRoot) {
        modelRoot = null
      }
      clonedScene = null
      footOffsetApplied = false
      weaponAttached = false
      attachedWeaponItemId = null
    }
  })

  export function getNametagGroup() {
    return nametagGroup
  }

  export function getTorchLight() {
    return torchLight
  }

  // Function to update mixer and animation state and nametag - called from GameScene gameLoop
  export function update(deltaTime: number) {
    // Sync Three.js group position directly from the Vector3 prop
    // (Svelte cannot track mutations on THREE.Vector3 objects)
    if (modelGroup) {
      const yOffset = playerState === 'interact' ? interactOffsetY : 0
      modelGroup.position.set(position.x, position.y + yOffset, position.z)
    }

    // Update nametag logic (formerly in useTask)
    if (camera && nametagGroup) {
      _nametagPos.set(position.x, position.y + 2.2, position.z)
      const dist = camera.position.distanceTo(_nametagPos)

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

    // Torch light flickering (works for both local and remote torches)
    const torchActive = isCurrentPlayer ? get(torchLightEnabled) : torchMode === 'light-only'
    if (torchLight && torchActive) {
      torchFlickerTime = applyTorchFlicker(torchLight, torchFlickerTime, deltaTime)
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

      // After first animation frame, measure foot bone positions and
      // shift the model up so the lowest foot bone sits just above origin.
      // This accounts for animation-time root offsets that the bind-pose
      // bounding box cannot capture.
      if (!footOffsetApplied && clonedScene && modelRoot) {
        footOffsetApplied = true
        const _boneVec = new THREE.Vector3()
        const groupWorldY = modelGroup
          ? modelGroup.getWorldPosition(_boneVec).y
          : position.y
        let lowestFootY = Infinity
        modelRoot.traverse((child) => {
          if (
            child instanceof THREE.Bone &&
            /foot|toe/i.test(child.name)
          ) {
            child.getWorldPosition(_boneVec)
            const localY = _boneVec.y - groupWorldY
            if (localY < lowestFootY) lowestFootY = localY
          }
        })
        if (lowestFootY < Infinity) {
          // Place lowest foot bone ~1cm above origin (shoe sole margin)
          const correction = -lowestFootY + 0.01
          if (correction > 0.001) {
            clonedScene.position.y += correction
          }
        }
      }

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
    bind:ref={modelGroup}
    position={[position.x, position.y, position.z]}
    rotation={[0, rotation, 0]}
  >
    <!-- 3D Character Model with real animations -->
    <T is={modelRoot} />

    <!-- Torch point light (always mounted, controlled via intensity to avoid WebGPU shader recompilation) -->
    <!-- castShadow is static: true for local player only. WebGPU PointShadowNode crashes
         if castShadow is toggled dynamically (depthTexture null on shadow map resources). -->
    <T.PointLight
      bind:ref={torchLight}
      position={[TORCH_BASE_POSITION.x, TORCH_BASE_POSITION.y, TORCH_BASE_POSITION.z]}
      color="#ffcc66"
      intensity={isCurrentPlayer
        ? ($torchLightEnabled ? TORCH_BASE_INTENSITY : 0)
        : (torchMode === 'light-only' ? TORCH_BASE_INTENSITY : 0)}
      distance={TORCH_BASE_DISTANCE}
      decay={TORCH_BASE_DECAY}
      castShadow={isCurrentPlayer}
      shadow.mapSize.width={512}
      shadow.mapSize.height={512}
      shadow.camera.near={0.5}
      shadow.camera.far={TORCH_BASE_DISTANCE}
      shadow.bias={-0.005}
      shadow.normalBias={0.05}
      shadow.radius={5}
    />

    <!-- Blob shadow circle for remote torches (no real shadow casting) -->
    {#if !isCurrentPlayer && torchMode === 'light-only'}
      <T.Mesh
        geometry={blobShadowGeometry}
        material={blobShadowMaterial}
        position.y={0.05}
        rotation.x={-Math.PI / 2}
      />
    {/if}
  </T.Group>
{/if}

<!-- Name tag (separate from character to avoid rotation inheritance) -->
<T.Group bind:ref={nametagGroup}>
  <TextLabel
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
    <TextLabel
      text={animDebugInfo}
      fontSize={0.2}
      color="#ffff00"
      position={[0, 0.4, 0]}
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
