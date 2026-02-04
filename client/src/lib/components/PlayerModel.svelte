<script lang="ts">
  import { T, useLoader } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import type { Vector3 } from 'three'
  import * as THREE from 'three'
  import { GLTFLoader } from 'three/examples/jsm/Addons.js'
  import * as SkeletonUtils from 'three/examples/jsm/utils/SkeletonUtils.js'
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { ANIMATION_ORDER, AnimationIndex } from '../types/animations'

  interface Props {
    position: Vector3
    name: string
    isCurrentPlayer: boolean
    playerState: 'idle' | 'moving'
    speed: number
    rotation: number
    totalDistance?: number
    cameraPosition: Vector3
    chatBubble?: string
  }

  let {
    position,
    name,
    isCurrentPlayer,
    playerState,
    speed: _speed,
    rotation,
    totalDistance,
    cameraPosition,
    chatBubble,
  }: Props = $props()

  // Calculate rotation to face camera in world space
  function calculateBillboardRotation(
    heightOffset: number
  ): [number, number, number] {
    if (!cameraPosition) {
      return [0, 0, 0]
    }

    // Calculate vector from world position to camera
    const worldX = position.x
    const worldY = position.y + heightOffset
    const worldZ = position.z

    const dx = cameraPosition.x - worldX
    const dy = cameraPosition.y - worldY
    const dz = cameraPosition.z - worldZ

    // Calculate yaw angle (y rotation) first - horizontal direction to camera
    const yaw = Math.atan2(dx, dz)

    // Calculate horizontal distance for pitch calculation
    const horizontalDistance = Math.sqrt(dx * dx + dz * dz)

    // Calculate pitch angle (x rotation) - vertical angle to camera
    const pitch = -Math.atan2(dy, horizontalDistance)

    return [pitch, yaw, 0]
  }

  // Calculate nametag rotation to face camera in world space
  function calculateNametagRotation(): [number, number, number] {
    return calculateBillboardRotation(2.5)
  }

  // Calculate chat bubble rotation to face camera in world space
  function calculateChatBubbleRotation(): [number, number, number] {
    return calculateBillboardRotation(3.2)
  }

  // Load animated model
  const gltf = useLoader(GLTFLoader).load('/models/maria.glb')

  // Load sword model
  const swordGltf = useLoader(GLTFLoader).load('/models/sword.glb')

  // Animation system - following gpt-all-in-one.html approach
  let mixer: THREE.AnimationMixer | null = null
  let currentAction: THREE.AnimationAction | null = null
  let modelRoot = $state<THREE.Group | null>(null)
  let clock = new THREE.Clock()

  let validAnimations: THREE.AnimationClip[] = []
  let lastPlayerState: 'idle' | 'moving' | undefined = undefined
  let currentMovementAnimationIndex: number | undefined = undefined // Locked animation for current movement
  const OVERLAP_BEFORE_END = 0.3 // Start next animation overlap 0.3 seconds before current ends

  // Distance thresholds for animation selection
  const WALK_DISTANCE_THRESHOLD = 3 // Distance <= 3 units: walk
  const JOG_DISTANCE_THRESHOLD = 8 // Distance <= 8 units: jog, > 8: run

  // Select movement animation based on total distance
  function selectMovementAnimation(distance: number | undefined): number {
    if (distance === undefined) {
      return AnimationIndex.JOG // Default to jog if no distance
    }
    if (distance <= WALK_DISTANCE_THRESHOLD) {
      return AnimationIndex.WALK
    } else if (distance <= JOG_DISTANCE_THRESHOLD) {
      return AnimationIndex.JOG
    } else {
      return AnimationIndex.RUN
    }
  }

  function playAnimationForState() {
    if (!mixer || validAnimations.length === 0) return

    // Select animation based on player state and distance
    let clip: THREE.AnimationClip
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
      // Lock animation at the start of movement based on total distance
      if (currentMovementAnimationIndex === undefined) {
        currentMovementAnimationIndex = selectMovementAnimation(totalDistance)
      }
      clip = validAnimations[currentMovementAnimationIndex]
    } else {
      return // Unknown state
    }

    const newAction = mixer.clipAction(clip)

    // Setup new action
    newAction.reset()
    newAction.loop = playerState === 'idle' ? THREE.LoopOnce : THREE.LoopRepeat
    newAction.clampWhenFinished = playerState === 'idle'
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

      // Create a safely cloned model using SkeletonUtils - gpt-all-in-one.html 패턴 따름
      const cloned = SkeletonUtils.clone($gltf.scene)
      const newModelRoot = new THREE.Group()
      newModelRoot.add(cloned)

      // Enable shadows on all meshes
      newModelRoot.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.castShadow = true
          child.receiveShadow = true
        }
      })

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

      // Filter animations to only include tracks that match model nodes
      const animations = $gltf.animations || []
      console.log(`Found ${animations.length} animation clips`)

      // Collect all node names in the cloned model
      const modelNodeNames = new SvelteSet()
      cloned.traverse((obj) => {
        if (obj.name) modelNodeNames.add(obj.name)
      })
      console.log(`Model has ${modelNodeNames.size} named nodes`)
      console.log('Model node names:', Array.from(modelNodeNames).slice(0, 10))

      // Find animations by specific track names in order
      validAnimations = ANIMATION_ORDER.map((targetName) => {
        const foundClip = animations.find((clip) => clip.name === targetName)
        if (foundClip) {
          console.log(`✅ Found animation: ${targetName}`)
          return foundClip
        } else {
          console.log(`❌ Missing animation: ${targetName}`)
          return animations[0] // Use first animation as dummy to keep index alignment
        }
      })

      console.log(`Found ${validAnimations.length} valid animations`)

      if (validAnimations.length > 0) {
        // Setup mixer
        mixer = new THREE.AnimationMixer(newModelRoot)

        // Play appropriate animation based on isMoving state
        playAnimationForState()
      } else {
        console.warn('No suitable animations found with strict filtering')

        // Fallback: try to play any animation without filtering
        if (animations.length > 0) {
          console.log(
            'Trying fallback: playing first animation without filtering'
          )
          mixer = new THREE.AnimationMixer(newModelRoot)
          const clip = animations[0]
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
    const checkGltf = () => {
      if ($gltf) {
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

  // Function to update mixer and animation state - called from GameScene gameLoop
  export function updateAnimation() {
    if (!mixer) return

    // Update mixer
    if (currentAction) {
      const deltaTime = clock.getDelta()
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

    // Update animation state
    if (validAnimations.length > 0) {
      // Only update animation if the player state has changed
      if (lastPlayerState !== playerState) {
        lastPlayerState = playerState
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
<Text
  text={name}
  position={[position.x, position.y + 2.5, position.z]}
  rotation={calculateNametagRotation()}
  fontSize={0.3}
  color={isCurrentPlayer ? '#4299e1' : '#ffffff'}
  anchorX="center"
  anchorY="middle"
/>

<!-- Chat bubble (appears above player when they send a message) -->
{#if chatBubble}
  <!-- Chat bubble background -->
  <T.Mesh
    position={[position.x, position.y + 3.2, position.z]}
    rotation={calculateChatBubbleRotation()}
  >
    <T.PlaneGeometry
      args={[Math.min(chatBubble.length * 0.15 + 0.4, 4), 0.5]}
    />
    <T.MeshBasicMaterial color="#000000" opacity={0.7} transparent={true} />
  </T.Mesh>
  <!-- Chat bubble text -->
  <Text
    text={chatBubble.length > 25 ? chatBubble.slice(0, 25) + '...' : chatBubble}
    position={[position.x, position.y + 3.2, position.z + 0.01]}
    rotation={calculateChatBubbleRotation()}
    fontSize={0.25}
    color="#ffffff"
    anchorX="center"
    anchorY="middle"
    maxWidth={3.5}
  />
{/if}
