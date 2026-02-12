<script lang="ts">
  import { T } from '@threlte/core'
  import { OrbitControls, Grid } from '@threlte/extras'
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import {
    gameStore,
    resetGameStore,
    type LocalPlayer,
    type RemotePlayer,
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
  import {
    debugVisible,
    cameraRotationEnabled,
    playerDebugInfo,
  } from '../stores/debugStore'
  import { initFpsCounting, tickFps } from './FPSCounter.svelte'

  interface Props {
    serverUrl: string
  }

  let { serverUrl }: Props = $props()

  let currentPlayer = $state<LocalPlayer | null>(null)
  let otherPlayers = $state<Map<string, RemotePlayer>>(new Map())
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
  const TARGET_FPS = 60
  const FRAME_TIME = 1000 / TARGET_FPS // 16.67ms
  const FRAME_TIME_TOLERANCE = 0.5 // absorb timer jitter (e.g. 16.6ms vs 16.67ms)
  const MAX_CATCH_UP_STEPS = 5

  type LoopProfileSection =
    | 'frameWork'
    | 'cameraOffset'
    | 'playerControl'
    | 'remoteInterpolation'
    | 'currentPlayerAnimation'
    | 'otherPlayerAnimation'
    | 'monsterAnimation'
    | 'monsterLogic'
    | 'cameraUpdate'
    | 'lightUpdate'

  const LOOP_PROFILE_SECTIONS: readonly LoopProfileSection[] = [
    'frameWork',
    'cameraOffset',
    'playerControl',
    'remoteInterpolation',
    'currentPlayerAnimation',
    'otherPlayerAnimation',
    'monsterAnimation',
    'monsterLogic',
    'cameraUpdate',
    'lightUpdate',
  ] as const

  interface LoopProfileStats {
    totalMs: number
    maxMs: number
    count: number
  }

  const loopProfileStats = new Map<LoopProfileSection, LoopProfileStats>(
    LOOP_PROFILE_SECTIONS.map((section) => [
      section,
      { totalMs: 0, maxMs: 0, count: 0 },
    ])
  )
  let loopProfileEnabled = false
  let loopProfileWindowStart = 0
  let loopProfileFrameCount = 0
  let loopProfileFrameDropCount = 0
  let loopProfileRawDeltaTotal = 0
  let loopProfileRawDeltaMax = 0

  function resetLoopProfileWindow(windowStart: number) {
    loopProfileWindowStart = windowStart
    loopProfileFrameCount = 0
    loopProfileFrameDropCount = 0
    loopProfileRawDeltaTotal = 0
    loopProfileRawDeltaMax = 0
    for (const section of LOOP_PROFILE_SECTIONS) {
      const stats = loopProfileStats.get(section)!
      stats.totalMs = 0
      stats.maxMs = 0
      stats.count = 0
    }
  }

  function recordLoopProfile(section: LoopProfileSection, durationMs: number) {
    const stats = loopProfileStats.get(section)
    if (!stats) return
    stats.totalMs += durationMs
    stats.maxMs = Math.max(stats.maxMs, durationMs)
    stats.count += 1
  }

  function flushLoopProfile(now: number) {
    const elapsed = now - loopProfileWindowStart
    if (!loopProfileEnabled || elapsed < 1000 || loopProfileFrameCount === 0) return

    const rows = LOOP_PROFILE_SECTIONS.map((section) => {
      const stats = loopProfileStats.get(section)!
      const avgMs = stats.count > 0 ? stats.totalMs / stats.count : 0
      return {
        section,
        avg_ms: Number(avgMs.toFixed(3)),
        max_ms: Number(stats.maxMs.toFixed(3)),
        samples: stats.count,
      }
    })

    const avgRawDelta = loopProfileRawDeltaTotal / loopProfileFrameCount
    console.groupCollapsed(
      `[LoopProfile] frames=${loopProfileFrameCount} dropped=${loopProfileFrameDropCount} avgDelta=${avgRawDelta.toFixed(2)}ms maxDelta=${loopProfileRawDeltaMax.toFixed(2)}ms`
    )
    console.table(rows)
    console.groupEnd()

    resetLoopProfileWindow(now)
  }

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
    tickFps(currentTime)

    const rawDeltaTime = currentTime - lastFrameTime
    const shouldRunFrame = rawDeltaTime >= FRAME_TIME - FRAME_TIME_TOLERANCE

    // Throttle to 60fps
    if (shouldRunFrame) {
      const unclampedSteps = Math.max(
        1,
        Math.floor((rawDeltaTime + FRAME_TIME_TOLERANCE) / FRAME_TIME)
      )
      const stepCount = Math.min(unclampedSteps, MAX_CATCH_UP_STEPS)
      const fixedDeltaTime = FRAME_TIME * stepCount

      if (loopProfileEnabled) {
        loopProfileFrameCount += 1
        loopProfileRawDeltaTotal += fixedDeltaTime
        loopProfileRawDeltaMax = Math.max(loopProfileRawDeltaMax, fixedDeltaTime)
        if (fixedDeltaTime > FRAME_TIME * 1.5) {
          loopProfileFrameDropCount += 1
        }
      }

      const frameWorkStart = performance.now()

      // Apply time scale for slow motion debugging
      const deltaTime = fixedDeltaTime * $timeScale

      // Calculate camera offset before player movement
      const cameraOffsetStart = performance.now()
      const cameraOffset = calculateCameraOffset()
      if (loopProfileEnabled) {
        recordLoopProfile('cameraOffset', performance.now() - cameraOffsetStart)
      }

      // Update player controls
      const playerControlStart = performance.now()
      if (playerControl) {
        playerControl.updateKeyboardMovement()
        playerControl.updatePlayerMovement(deltaTime)
      }
      if (loopProfileEnabled) {
        recordLoopProfile('playerControl', performance.now() - playerControlStart)
      }

      // Update remote player interpolation
      const remoteInterpolationStart = performance.now()
      remotePlayerManager.update(deltaTime)
      if (loopProfileEnabled) {
        recordLoopProfile(
          'remoteInterpolation',
          performance.now() - remoteInterpolationStart
        )
      }

      // Update player model animations
      const currentPlayerAnimationStart = performance.now()
      if (currentPlayerModel) {
        currentPlayerModel.update(deltaTime / 1000)
      }
      if (loopProfileEnabled) {
        recordLoopProfile(
          'currentPlayerAnimation',
          performance.now() - currentPlayerAnimationStart
        )
      }

      // Update other player model animations
      const otherPlayerAnimationStart = performance.now()
      for (const playerModel of otherPlayerModels) {
        if (playerModel) {
          playerModel.update(deltaTime / 1000)
        }
      }
      if (loopProfileEnabled) {
        recordLoopProfile(
          'otherPlayerAnimation',
          performance.now() - otherPlayerAnimationStart
        )
      }

      // Update monster animations
      const monsterAnimationStart = performance.now()
      for (const monsterModel of monsterModels) {
        if (monsterModel) {
          monsterModel.update(deltaTime / 1000, camera) // Convert ms to seconds for THREE.AnimationMixer
        }
      }
      if (loopProfileEnabled) {
        recordLoopProfile('monsterAnimation', performance.now() - monsterAnimationStart)
      }

      // Update monster spawning logic
      const monsterLogicStart = performance.now()
      if (currentPlayer) {
        monsterManager.update(deltaTime, currentPlayer.position)
        playerDebugInfo.set({
          position: {
            x: currentPlayer.position.x,
            y: currentPlayer.position.y,
            z: currentPlayer.position.z,
          },
          rotation: currentPlayerState.rotation,
        })
      } else {
        playerDebugInfo.set(null)
      }
      if (loopProfileEnabled) {
        recordLoopProfile('monsterLogic', performance.now() - monsterLogicStart)
      }

      // Update camera with preserved offset
      const cameraUpdateStart = performance.now()
      updateCameraWithOffset(cameraOffset)
      if (loopProfileEnabled) {
        recordLoopProfile('cameraUpdate', performance.now() - cameraUpdateStart)
      }

      // Update directional light to follow player
      const lightUpdateStart = performance.now()
      updateLightPosition()
      if (loopProfileEnabled) {
        recordLoopProfile('lightUpdate', performance.now() - lightUpdateStart)
      }

      if (loopProfileEnabled) {
        recordLoopProfile('frameWork', performance.now() - frameWorkStart)
      }

      if (unclampedSteps > MAX_CATCH_UP_STEPS) {
        // Drop excessive backlog after long stalls (tab switch, debugger pause, etc.).
        lastFrameTime = currentTime - FRAME_TIME
      } else {
        lastFrameTime += fixedDeltaTime
      }
    }

    if (loopProfileEnabled) {
      flushLoopProfile(currentTime)
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
    loopProfileEnabled = false
    resetLoopProfileWindow(performance.now())

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
    initFpsCounting()
    gameLoopId = requestAnimationFrame(gameLoop)

    // Start chat bubble expiration checker
    startChatBubbleChecker()

    networkManager.connect(serverUrl)

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
      playerDebugInfo.set(null)
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
    {#if remotePlayer}
      <PlayerModel
        bind:this={otherPlayerModels[index]}
        position={new THREE.Vector3(
          remotePlayer.position.x,
          remotePlayer.position.y,
          remotePlayer.position.z
        )}
        name={player.name}
        isCurrentPlayer={false}
        playerState={remotePlayer.state}
        attackCounter={remotePlayer.attackCounter}
        speed={remotePlayer.speed}
        rotation={remotePlayer.rotation}
        movementMode={remotePlayer.movementMode}
        {camera}
        chatBubble={chatBubbles.get(player.id)?.message}
      />
    {/if}
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
