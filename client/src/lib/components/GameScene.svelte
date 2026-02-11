<script lang="ts">
  import { T } from '@threlte/core'
  import { OrbitControls, Grid } from '@threlte/extras'
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import {
    gameStore,
    resetGameStore,
    type Player,
    type ChatBubble,
  } from '../stores/gameStore'
  import {
    startChatBubbleChecker,
    stopChatBubbleChecker,
  } from '../managers/chatBubbleManager'
  import { remotePlayerManager } from '../managers/remotePlayerManager'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import PlayerModel from './PlayerModel.svelte'
  import PlayerControl from './PlayerControl.svelte'
  import SplatTerrain from './SplatTerrain.svelte'
  import Monster from './Monster.svelte'
  import { type PlayerState } from '../utils/movementUtils'
  import { cameraDistance, cameraResetNonce } from '../stores/cameraStore'
  import { timeScale } from '../stores/timeStore'
  import { debugVisible, cameraRotationEnabled } from '../stores/debugStore'

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
  let playerAttackDuration = $state(1.5) // Default 1.5s

  // Camera follow system
  let cameraTarget = $state<[number, number, number]>([0, 0, 0])

  // Initial camera position relative to player (Distance 10, 45 degree pitch)
  const INITIAL_DISTANCE = 10
  const INITIAL_PITCH = Math.PI / 4
  const CAMERA_OFFSET = {
    x: 0,
    y: INITIAL_DISTANCE * Math.sin(INITIAL_PITCH),
    z: INITIAL_DISTANCE * Math.cos(INITIAL_PITCH),
  }

  // Reset camera rotation to default angle when debug mode is turned off or rotation is disabled
  let prevDebugVisible = $state(false)
  let prevRotationEnabled = $state(false)
  $effect(() => {
    const currentDebug = $debugVisible
    const currentRotation = $cameraRotationEnabled

    // Reset if debug mode was turned off OR rotation was just disabled
    if ((prevDebugVisible && !currentDebug) || (prevRotationEnabled && !currentRotation)) {
      resetCameraRotation()
    }

    prevDebugVisible = currentDebug
    prevRotationEnabled = currentRotation
  })

  function resetCameraRotation() {
    if (!currentPlayer || !camera) return

    const playerPos = currentPlayer.position
    const dx = camera.position.x - playerPos.x
    const dy = camera.position.y - playerPos.y
    const dz = camera.position.z - playerPos.z
    const currentDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

    camera.position.set(
      playerPos.x,
      playerPos.y + currentDistance * Math.sin(INITIAL_PITCH),
      playerPos.z + currentDistance * Math.cos(INITIAL_PITCH)
    )
    camera.lookAt(playerPos.x, playerPos.y, playerPos.z)
    cameraTarget = [playerPos.x, playerPos.y, playerPos.z]
  }

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
    rotation: 0,
    position: { x: 0, y: 0, z: 0 },
  })

  // References to PlayerModel components
  let currentPlayerModel = $state<PlayerModel | null>(null)
  let otherPlayerModels = $state<PlayerModel[]>([])

  // Reference to PlayerControl component
  let playerControl = $state<PlayerControl>()

  // Handle player state changes from PlayerControl
  function handlePlayerStateChange(newState: PlayerState) {
    currentPlayerState = newState
  }

  gameStore.subscribe((state) => {
    currentPlayer = state.currentPlayer
    otherPlayers = state.otherPlayers
    chatBubbles = state.chatBubbles
  })

  // Monster models reference
  let monsterModels = $state<Monster[]>([])

  // Main game loop with 60fps throttling
  function gameLoop(currentTime: number) {
    const rawDeltaTime = currentTime - lastFrameTime

    // Throttle to 60fps
    if (rawDeltaTime >= FRAME_TIME) {
      // Apply time scale for slow motion debugging
      const deltaTime = rawDeltaTime * $timeScale

      // Calculate camera offset before player movement
      const cameraOffset = calculateCameraOffset()

      // Update player controls
      if (playerControl) {
        playerControl.updateKeyboardMovement()
        playerControl.updatePlayerMovement(deltaTime)
      }

      // Update remote player interpolation
      remotePlayerManager.update(deltaTime, otherPlayers)

      // Update player model animations
      if (currentPlayerModel) {
        currentPlayerModel.update(deltaTime / 1000)
      }

      // Update other player model animations
      for (const playerModel of otherPlayerModels) {
        if (playerModel) {
          playerModel.update(deltaTime / 1000)
        }
      }

      // Update monster animations
      for (const monsterModel of monsterModels) {
        if (monsterModel) {
          monsterModel.update(deltaTime / 1000, camera) // Convert ms to seconds for THREE.AnimationMixer
        }
      }

      // Update monster spawning logic
      if (currentPlayer) {
        monsterManager.update(deltaTime, currentPlayer.position)
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

    // Calculate and update camera distance
    const distance = Math.sqrt(
      distanceVector.x * distanceVector.x +
        distanceVector.y * distanceVector.y +
        distanceVector.z * distanceVector.z
    )
    cameraDistance.set(distance)

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

  function resetCameraToInitialState() {
    if (!currentPlayer || !camera) return

    camera.position.set(
      currentPlayer.position.x + CAMERA_OFFSET.x,
      currentPlayer.position.y + CAMERA_OFFSET.y,
      currentPlayer.position.z + CAMERA_OFFSET.z
    )
    camera.lookAt(
      currentPlayer.position.x,
      currentPlayer.position.y,
      currentPlayer.position.z
    )
    cameraTarget = [
      currentPlayer.position.x,
      currentPlayer.position.y,
      currentPlayer.position.z,
    ]

    const initialDistance = Math.sqrt(
      CAMERA_OFFSET.x * CAMERA_OFFSET.x +
        CAMERA_OFFSET.y * CAMERA_OFFSET.y +
        CAMERA_OFFSET.z * CAMERA_OFFSET.z
    )
    cameraDistance.set(initialDistance)
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
      directionalLight.target.position.set(
        playerPos.x,
        playerPos.y,
        playerPos.z
      )
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
    const unsubscribeCameraReset = cameraResetNonce.subscribe((nonce) => {
      // Ignore initial store emission; only react to explicit reset requests.
      if (nonce > 0) {
        resetCameraToInitialState()
      }
    })

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
        resetCameraToInitialState()
        cameraInitialized = true
      }
    }, 1100)

    return () => {
      unsubscribeCameraReset()
      stopGameLoop()
      stopChatBubbleChecker()
      networkManager.disconnect()
      monsterManager.reset()
      remotePlayerManager.reset()
      resetGameStore()
    }
  })
</script>

<T.PerspectiveCamera bind:ref={camera} makeDefault fov={75}>
  <OrbitControls
    enableRotate={$cameraRotationEnabled}
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

{#if camera && groundMesh}
  <!-- PlayerControl component handles input and updates player state -->
  <PlayerControl
    bind:this={playerControl}
    onStateChange={handlePlayerStateChange}
    {camera}
    {groundMesh}
    monsterMeshes={monsterModels
      .map((m) => m?.getMeshGroup())
      .filter((g) => g !== undefined) as THREE.Group[]}
    attackCooldown={playerAttackDuration}
  />
{/if}

{#if currentPlayer && cameraInitialized && camera}
  <PlayerModel
    bind:this={currentPlayerModel}
    position={currentPlayer.position}
    name={currentPlayer.name}
    isCurrentPlayer={true}
    playerState={currentPlayerState.state}
    attackCounter={currentPlayerState.attackCounter}
    speed={currentPlayerState.speed}
    rotation={currentPlayerState.rotation}
    movementMode={currentPlayerState.movementMode}
    {camera}
    chatBubble={chatBubbles.get(currentPlayer.id)?.message}
    onAttackDuration={(duration) => (playerAttackDuration = duration)}
    lastDamageInfo={currentPlayer.lastDamageInfo}
  />
{/if}

{#if cameraInitialized && camera}
  {#each [...otherPlayers.values()] as player, index (player.id)}
    {@const remotePlayer = remotePlayerManager.players.get(player.id)}
    {@const displayPosition = remotePlayer
      ? new THREE.Vector3(
          remotePlayer.position.x,
          remotePlayer.position.y,
          remotePlayer.position.z
        )
      : player.position}
    <PlayerModel
      bind:this={otherPlayerModels[index]}
      position={displayPosition}
      name={player.name}
      isCurrentPlayer={false}
      playerState={remotePlayer?.state ?? 'idle'}
      speed={remotePlayer?.speed ?? 0}
      rotation={remotePlayer?.rotation ?? 0}
      movementMode={remotePlayer?.movementMode}
      {camera}
      chatBubble={chatBubbles.get(player.id)?.message}
    />
  {/each}
{/if}

{#each [...monsterManager.monsters.values()] as monster, index (monster.id)}
  <Monster
    bind:this={monsterModels[index]}
    id={monster.id}
    type={monster.type}
    position={monster.position}
    rotation={monster.rotation}
    monsterState={monster.state}
    lastDamageInfo={monster.lastDamageInfo}
  />
{/each}
