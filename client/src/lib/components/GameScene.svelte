<script lang="ts">
  import { T } from '@threlte/core'
  import { OrbitControls, Grid } from '@threlte/extras'
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { gameStore, type Player, type ChatBubble } from '../stores/gameStore'
  import {
    startChatBubbleChecker,
    stopChatBubbleChecker,
  } from '../managers/chatBubbleManager'
  import { networkManager } from '../network/socket'
  import PlayerModel from './PlayerModel.svelte'
  import PlayerControl, { type PlayerState } from './PlayerControl.svelte'
  import SplatTerrain from './SplatTerrain.svelte'

  interface Props {
    serverUrl: string
    playerName: string
    password: string
  }

  let { serverUrl, playerName, password: _password }: Props = $props()

  let currentPlayer = $state<Player | null>(null)
  let otherPlayers = $state(new Map())
  let chatBubbles = $state<Map<string, ChatBubble>>(new Map())
  let camera = $state<THREE.PerspectiveCamera | undefined>(undefined)
  let directionalLight = $state<THREE.DirectionalLight | undefined>(undefined)
  let groundMesh = $state<THREE.Mesh | undefined>(undefined)
  let terrainGeometry = $state<THREE.BufferGeometry | null>(null)
  let cameraInitialized = $state(false)

  // Camera follow system
  let cameraTarget = $state<[number, number, number]>([0, 0, 0])
  const CAMERA_OFFSET = { x: 0, y: 5, z: 5 } // Relative to player

  // Light follow system - offset relative to player
  const LIGHT_OFFSET = { x: 10, y: 10, z: 10 }

  // Game loop
  let gameLoopId = $state<number | null>(null)
  let lastFrameTime = $state(0)
  const TARGET_FPS = 120
  const FRAME_TIME = 1000 / TARGET_FPS // 16.67ms

  // Player state from PlayerControl
  let currentPlayerState = $state<PlayerState>({
    state: 'idle',
    speed: 0,
    direction: 0,
    position: { x: 0, y: 0, z: 0 },
  })

  // References to PlayerModel components
  let currentPlayerModel = $state<PlayerModel | null>(null)
  let otherPlayerModels = $state<PlayerModel[]>([])

  // Remote player movement states (for animation)
  let remotePlayerStates = new SvelteMap<
    string,
    { state: 'idle' | 'moving'; speed: number; rotation: number }
  >()

  // Interpolated positions for remote players (separate from store for reactivity)
  let remotePlayerPositions = new SvelteMap<
    string,
    { x: number; y: number; z: number }
  >()

  // Remote player movement data (for acceleration/deceleration)
  let remotePlayerMovement = new SvelteMap<
    string,
    {
      startPos: { x: number; y: number; z: number }
      targetPos: { x: number; y: number; z: number }
      totalDistance: number
      currentSpeed: number
    }
  >()

  // Movement settings for remote players (should match PlayerControl)
  const REMOTE_MOVEMENT_SPEED = 3 // units per second (same as local player)
  const REMOTE_ACCELERATION = 6 // units per second squared
  const REMOTE_DECELERATION = 6 // units per second squared
  const ACCEL_DISTANCE = (REMOTE_MOVEMENT_SPEED * REMOTE_MOVEMENT_SPEED) / (2 * REMOTE_ACCELERATION)
  const DECEL_DISTANCE = (REMOTE_MOVEMENT_SPEED * REMOTE_MOVEMENT_SPEED) / (2 * REMOTE_DECELERATION)
  const MOVEMENT_THRESHOLD = 0.05 // Distance threshold to consider "stopped"

  // Reference to PlayerControl component
  let playerControl: PlayerControl

  // Handle player state changes from PlayerControl
  function handlePlayerStateChange(newState: PlayerState) {
    currentPlayerState = newState
  }

  // Move remote players toward their target positions with acceleration/deceleration
  function updateRemotePlayers(deltaTime: number) {
    const dt = deltaTime / 1000 // Convert to seconds

    otherPlayers.forEach((player, playerId) => {
      if (!player.targetPosition) return

      // Get current interpolated position or initialize from player position
      let currentPos = remotePlayerPositions.get(playerId)
      if (!currentPos) {
        currentPos = {
          x: player.position.x,
          y: player.position.y,
          z: player.position.z,
        }
      }

      const targetPos = player.targetPosition

      // Get or initialize movement data
      let movement = remotePlayerMovement.get(playerId)
      const targetChanged =
        !movement ||
        movement.targetPos.x !== targetPos.x ||
        movement.targetPos.y !== targetPos.y ||
        movement.targetPos.z !== targetPos.z

      if (targetChanged) {
        // New target - initialize movement from current position
        const tdx = targetPos.x - currentPos.x
        const tdy = targetPos.y - currentPos.y
        const tdz = targetPos.z - currentPos.z
        const totalDistance = Math.sqrt(tdx * tdx + tdy * tdy + tdz * tdz)

        movement = {
          startPos: { ...currentPos },
          targetPos: { x: targetPos.x, y: targetPos.y, z: targetPos.z },
          totalDistance,
          currentSpeed: movement?.currentSpeed ?? 0,
        }
        remotePlayerMovement.set(playerId, movement)
      }

      // movement is guaranteed to be defined after above block
      if (!movement) return

      // Calculate distances
      const dx = targetPos.x - currentPos.x
      const dy = targetPos.y - currentPos.y
      const dz = targetPos.z - currentPos.z
      const remainingDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

      if (remainingDistance > MOVEMENT_THRESHOLD) {
        const traveledDistance = movement.totalDistance - remainingDistance

        // Determine speed based on phase (acceleration, cruise, deceleration)
        let newSpeed = movement.currentSpeed
        if (traveledDistance < ACCEL_DISTANCE) {
          // Acceleration phase
          newSpeed = Math.min(newSpeed + REMOTE_ACCELERATION * dt, REMOTE_MOVEMENT_SPEED)
        } else if (remainingDistance > DECEL_DISTANCE) {
          // Cruise phase
          newSpeed = REMOTE_MOVEMENT_SPEED
        } else {
          // Deceleration phase
          newSpeed = Math.max(newSpeed - REMOTE_DECELERATION * dt, 0.1)
        }

        movement.currentSpeed = newSpeed
        remotePlayerMovement.set(playerId, movement)

        // Calculate rotation (direction of movement)
        const rotation = Math.atan2(dx, dz)

        // Move at current speed
        const moveDistance = newSpeed * dt
        let newPos
        if (moveDistance >= remainingDistance) {
          newPos = { x: targetPos.x, y: targetPos.y, z: targetPos.z }
        } else {
          const dirX = dx / remainingDistance
          const dirY = dy / remainingDistance
          const dirZ = dz / remainingDistance
          newPos = {
            x: currentPos.x + dirX * moveDistance,
            y: currentPos.y + dirY * moveDistance,
            z: currentPos.z + dirZ * moveDistance,
          }
        }

        remotePlayerPositions.set(playerId, newPos)
        remotePlayerStates.set(playerId, {
          state: 'moving',
          speed: newSpeed,
          rotation,
        })
      } else {
        // Arrived at destination
        remotePlayerPositions.set(playerId, {
          x: targetPos.x,
          y: targetPos.y,
          z: targetPos.z,
        })

        if (movement) {
          movement.currentSpeed = 0
          remotePlayerMovement.set(playerId, movement)
        }

        remotePlayerStates.set(playerId, {
          state: 'idle',
          speed: 0,
          rotation: remotePlayerStates.get(playerId)?.rotation ?? 0,
        })
      }
    })
  }

  gameStore.subscribe((state) => {
    currentPlayer = state.currentPlayer
    otherPlayers = state.otherPlayers
    chatBubbles = state.chatBubbles
  })

  // Main game loop with 60fps throttling
  function gameLoop(currentTime: number) {
    const deltaTime = currentTime - lastFrameTime

    // Throttle to 60fps
    if (deltaTime >= FRAME_TIME) {
      // Calculate camera offset before player movement
      const cameraOffset = calculateCameraOffset()

      // Update player controls
      if (playerControl) {
        playerControl.updateKeyboardMovement()
        playerControl.updatePlayerMovement(deltaTime)
      }

      // Update remote player interpolation
      updateRemotePlayers(deltaTime)

      // Update player model animations
      if (currentPlayerModel) {
        currentPlayerModel.updateAnimation()
      }

      // Update other player model animations
      for (const playerModel of otherPlayerModels) {
        if (playerModel) {
          playerModel.updateAnimation()
        }
      }

      // Update camera with preserved offset
      updateCameraWithOffset(cameraOffset)

      // Update directional light to follow player
      updateLightPosition()

      lastFrameTime = currentTime
    }

    // Always continue the loop
    gameLoopId = requestAnimationFrame(gameLoop)
  }

  function calculateCameraOffset() {
    if (!currentPlayer || !camera) {
      return { x: CAMERA_OFFSET.x, y: CAMERA_OFFSET.y, z: CAMERA_OFFSET.z }
    }

    // Calculate current distance vector from player to camera
    const currentCameraPos = camera.position
    const playerPos = currentPlayer.position

    // Get the current distance vector (preserving zoom)
    const distanceVector = {
      x: currentCameraPos.x - playerPos.x,
      y: currentCameraPos.y - playerPos.y,
      z: currentCameraPos.z - playerPos.z,
    }

    return distanceVector
  }

  function updateCameraWithOffset(offset: { x: number; y: number; z: number }) {
    if (!currentPlayer || !camera) return

    const playerPos = currentPlayer.position

    // Update camera position by adding the preserved offset to new player position
    const newCameraPosition = {
      x: playerPos.x + offset.x,
      y: playerPos.y + offset.y,
      z: playerPos.z + offset.z,
    }

    camera.position.set(
      newCameraPosition.x,
      newCameraPosition.y,
      newCameraPosition.z
    )

    // Make camera look at player directly
    camera.lookAt(playerPos.x, playerPos.y, playerPos.z)

    // Update camera target to look at player
    cameraTarget = [playerPos.x, playerPos.y, playerPos.z]
  }

  function updateLightPosition() {
    if (!currentPlayer || !directionalLight) return

    const playerPos = currentPlayer.position

    // Update light position to follow player with fixed offset
    directionalLight.position.set(
      playerPos.x + LIGHT_OFFSET.x,
      playerPos.y + LIGHT_OFFSET.y,
      playerPos.z + LIGHT_OFFSET.z
    )

    // Update shadow camera target to look at player
    if (directionalLight.target) {
      directionalLight.target.position.set(playerPos.x, playerPos.y, playerPos.z)
      directionalLight.target.updateMatrixWorld()
    }
  }

  // Stop game loop
  function stopGameLoop() {
    if (gameLoopId !== null) {
      cancelAnimationFrame(gameLoopId)
      gameLoopId = null
    }
  }

  onMount(() => {
    // Build a terrain geometry (XZ plane)
    const plane = new THREE.PlaneGeometry(100, 100, 128, 128)
    plane.rotateX(-Math.PI / 2) // Lay flat on XZ
    terrainGeometry = plane
    // Start game loop
    lastFrameTime = performance.now()
    gameLoopId = requestAnimationFrame(gameLoop)

    // Start chat bubble expiration checker
    startChatBubbleChecker()

    networkManager.connect(serverUrl)

    // Join the game with the player name from login
    setTimeout(() => {
      networkManager.joinGame(playerName)
    }, 1000)

    // Initialize camera position after a short delay to ensure camera ref is available
    setTimeout(() => {
      if (camera && currentPlayer) {
        // Set initial camera position
        camera.position.set(
          currentPlayer.position.x + CAMERA_OFFSET.x,
          currentPlayer.position.y + CAMERA_OFFSET.y,
          currentPlayer.position.z + CAMERA_OFFSET.z
        )
        cameraInitialized = true

        // Make camera look at player directly
        camera.lookAt(
          currentPlayer.position.x,
          currentPlayer.position.y,
          currentPlayer.position.z
        )

        // Set initial camera target to look at player
        cameraTarget = [
          currentPlayer.position.x,
          currentPlayer.position.y,
          currentPlayer.position.z,
        ]
      }
    }, 1100)

    return () => {
      stopGameLoop()
      stopChatBubbleChecker()
      networkManager.disconnect()
    }
  })
</script>

<T.PerspectiveCamera bind:ref={camera} makeDefault fov={75}>
  <OrbitControls
    enableRotate={true}
    enablePan={false}
    enableZoom={true}
    target={cameraTarget}
    minDistance={5}
    maxDistance={20}
  />
</T.PerspectiveCamera>

<T.DirectionalLight
  bind:ref={directionalLight}
  position={[10, 10, 10]}
  intensity={1.5}
  castShadow
  shadow.camera.left={-50}
  shadow.camera.right={50}
  shadow.camera.top={50}
  shadow.camera.bottom={-50}
  shadow.camera.near={0.5}
  shadow.camera.far={100}
  shadow.mapSize.width={2048}
  shadow.mapSize.height={2048}
/>
<T.AmbientLight intensity={0.4} />

<Grid
  infiniteGrid
  gridSize={100}
  sectionColor="#4a5568"
  sectionThickness={1.2}
  fadeDistance={100}
  position={[0, -1.1, 0]}
/>

{#if terrainGeometry}
  <SplatTerrain geometry={terrainGeometry} bind:mesh={groundMesh} />
{/if}

<!-- Terrain Field - 3x3 grid of field inspection models (commented out) -->
<!-- <TerrainField /> -->

<!-- PlayerControl component handles input and updates player state -->
<PlayerControl
  bind:this={playerControl}
  onStateChange={handlePlayerStateChange}
  {camera}
  {groundMesh}
/>

{#if currentPlayer && cameraInitialized && camera}
  <PlayerModel
    bind:this={currentPlayerModel}
    position={currentPlayer.position}
    name={currentPlayer.name}
    isCurrentPlayer={true}
    playerState={currentPlayerState.state}
    speed={currentPlayerState.speed}
    rotation={currentPlayerState.direction}
    cameraPosition={camera.position}
    chatBubble={chatBubbles.get(currentPlayer.id)?.message}
  />
{/if}

{#if cameraInitialized && camera}
  {#each [...otherPlayers.values()] as player, index (player.id)}
    {@const remoteState = remotePlayerStates.get(player.id) || {
      state: 'idle',
      speed: 0,
      rotation: 0,
    }}
    {@const interpolatedPos = remotePlayerPositions.get(player.id)}
    {@const displayPosition = interpolatedPos
      ? new THREE.Vector3(interpolatedPos.x, interpolatedPos.y, interpolatedPos.z)
      : player.position}
    <PlayerModel
      bind:this={otherPlayerModels[index]}
      position={displayPosition}
      name={player.name}
      isCurrentPlayer={false}
      playerState={remoteState.state}
      speed={remoteState.speed}
      rotation={remoteState.rotation}
      cameraPosition={camera.position}
      chatBubble={chatBubbles.get(player.id)?.message}
    />
  {/each}
{/if}
