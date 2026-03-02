<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import { OrbitControls } from '@threlte/extras'
  import * as THREE from 'three'
  import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
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
  import type PlayerModel from './PlayerModel.svelte'
  import type PlayerControl from './PlayerControl.svelte'
  import type Monster from './Monster.svelte'
  import GameSceneTerrainLayer from './game-scene/GameSceneTerrainLayer.svelte'
  import GameSceneWaterLayer from './game-scene/GameSceneWaterLayer.svelte'
  import GameScenePlayersLayer from './game-scene/GameScenePlayersLayer.svelte'
  import GameSceneMonstersLayer from './game-scene/GameSceneMonstersLayer.svelte'
  import MapEditorCursor from './map-editor/MapEditorCursor.svelte'
  import { type PlayerState } from '../utils/movementUtils'
  import {
    GAME_START_YEAR,
    SHADOW_CAMERA_EXTENT,
    SHADOW_CAMERA_FAR,
    SUN_DAY_DURATION_SECONDS,
    SUN_MAX_INTENSITY,
    SUN_START_HOUR,
    type CalendarDate,
    computeSunLightSnapshot,
    getCalendarDateFromGameDayIndex,
    getGameCalendarDayIndex,
  } from '../utils/celestialSimulation'
  import { cameraDistance, cameraResetNonce } from '../stores/cameraStore'
  import {
    timeScale,
    sunTimeScale,
    serverGameTime,
    type ServerGameTime,
  } from '../stores/timeStore'
  import {
    debugVisible,
    cameraRotationEnabled,
    playerDebugInfo,
    mapEditorMode,
    teleportLoading,
  } from '../stores/debugStore'
  import { initFpsCounting, tickFps } from './FPSCounter.svelte'
  import { eclipseState, setGameDate, setGameHour } from './GameTimeWidget.svelte'
  import {
    DEFAULT_CAMERA_OFFSET,
    INITIAL_DISTANCE,
    ORTHOGRAPHIC_DEFAULT_ZOOM,
    ORTHOGRAPHIC_FRUSTUM_HEIGHT,
    calculateCameraOffset as getCameraOffsetFromScene,
    resetCameraRotation as resetCameraRotationToDefault,
    resetCameraToInitialState as resetCameraToDefaultState,
    updateCameraWithOffset as applyCameraOffset,
    updateOrthographicFrustum,
  } from './game-scene/camera-utils'
  import {
    TERRAIN_GRID_RADIUS,
    TERRAIN_TILE_SEGMENTS,
    TERRAIN_TILE_SIZE,
    type TerrainTile,
    createTerrainGeometry,
    createTerrainTiles,
    getTerrainChunkFromPosition,
  } from './game-scene/terrain-utils'
  import { createLoopProfiler } from './game-scene/loop-profiler'
  import { createSceneLightingController } from './game-scene/scene-lighting'
  import { TerrainHeightManager } from '../managers/terrainHeightManager'
  import { TerrainSplatManager } from '../managers/terrainSplatManager'
  import { loadWaterNormalMap } from '../shaders/water-normal-gen'

  interface Props {
    serverUrl: string
    onCurrentPlayerDyingFinished?: () => void
    isCurrentPlayerLoading?: boolean
  }

  let { serverUrl, onCurrentPlayerDyingFinished, isCurrentPlayerLoading = $bindable(false) }: Props = $props()

  let currentPlayer = $state<LocalPlayer | null>(null)
  let otherPlayers = $state<Map<string, RemotePlayer>>(new Map())
  let chatBubbles = $state<Map<string, ChatBubble>>(new Map())
  let camera = $state<THREE.OrthographicCamera | undefined>(undefined)
  let directionalLight = $state<THREE.DirectionalLight | undefined>(undefined)
  let ambientLight = $state<THREE.AmbientLight | undefined>(undefined)
  let terrainMeshes = $state<(THREE.Mesh | undefined)[]>([])
  let terrainGeometry = $state<THREE.BufferGeometry | null>(null)
  let terrainTiles = $state<TerrainTile[]>([])
  let terrainCenterChunk = $state({ x: 0, z: 0 })
  const terrainHeightManager = new TerrainHeightManager()
  const terrainSplatManager = new TerrainSplatManager()
  monsterManager.heightManager = terrainHeightManager
  let waterNormalMap = $state<THREE.Texture | null>(null)
  let waterTime = $state(0)
  let waterSunDir = $state<THREE.Vector3 | null>(null)
  let waterSunColor = $state<THREE.Color | null>(null)
  const _waterSunDirTmp = new THREE.Vector3()
  let cameraInitialized = $state(false)
  let playerAttackDuration = $state(1.5) // Default 1.5s

  // Camera follow system
  let cameraTarget = $state<[number, number, number]>([0, 0, 0])

  const { size, renderer, scene } = useThrelte()
  let viewportSize = $state({ width: 1, height: 1 })

  const CAMERA_OFFSET = { ...DEFAULT_CAMERA_OFFSET }

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
    cameraTarget = resetCameraRotationToDefault(camera, currentPlayer.position)
  }

  $effect(() => {
    updateOrthographicFrustum(camera, viewportSize)
  })

  const sceneLighting = createSceneLightingController()

  let localCalendarDate = $state<CalendarDate>({
    year: GAME_START_YEAR,
    month: 1,
    day: 1,
  })
  let localDayElapsedSeconds = $state(
    (SUN_START_HOUR / 24) * SUN_DAY_DURATION_SECONDS
  )

  let latestServerGameTime = $state<ServerGameTime | null>(null)
  let latestSunTimeScale = $state(1)

  function syncCalendarToWidget() {
    setGameDate(
      localCalendarDate.year,
      localCalendarDate.month,
      localCalendarDate.day
    )
  }

  function syncLocalCalendarToServer(gameTime: ServerGameTime) {
    localCalendarDate = {
      year: gameTime.year,
      month: gameTime.month,
      day: gameTime.day,
    }
    localDayElapsedSeconds =
      ((gameTime.hour + gameTime.minute / 60) / 24) * SUN_DAY_DURATION_SECONDS
    syncCalendarToWidget()
    setGameHour(getLocalGameHour())
  }

  function getLocalGameHour() {
    return (localDayElapsedSeconds / SUN_DAY_DURATION_SECONDS) * 24
  }

  function addLocalCalendarDays(daysToAdd: number) {
    if (daysToAdd === 0) return
    const currentDayIndex = getGameCalendarDayIndex(localCalendarDate)
    localCalendarDate = getCalendarDateFromGameDayIndex(
      currentDayIndex + daysToAdd
    )
  }

  function advanceLocalCalendar(deltaSeconds: number) {
    if (deltaSeconds <= 0) return
    localDayElapsedSeconds += deltaSeconds
    if (localDayElapsedSeconds < SUN_DAY_DURATION_SECONDS) return

    const elapsedDays = Math.floor(localDayElapsedSeconds / SUN_DAY_DURATION_SECONDS)
    addLocalCalendarDays(elapsedDays)
    localDayElapsedSeconds -= elapsedDays * SUN_DAY_DURATION_SECONDS
    syncCalendarToWidget()
  }

  function applyServerGameHourIfAllowed() {
    if (latestSunTimeScale > 1) return
    if (latestServerGameTime === null) return
    syncLocalCalendarToServer(latestServerGameTime)
  }

  // Game loop
  let gameLoopId = $state<number | null>(null)
  let lastFrameTime = $state(0)
  const TARGET_FPS = 60
  const FRAME_TIME = 1000 / TARGET_FPS // 16.67ms
  const FRAME_TIME_TOLERANCE = 0.5 // absorb timer jitter (e.g. 16.6ms vs 16.67ms)
  const MAX_CATCH_UP_STEPS = 5

  let loopProfileEnabled = false
  const loopProfiler = createLoopProfiler(() => loopProfileEnabled)

  // Player state from PlayerControl
  let currentPlayerState = $state<PlayerState>({
    state: 'idle',
    speed: 0,
    rotation: 0,
    position: { x: 0, y: 0, z: 0 },
  })

  // References to PlayerModel components
  let currentPlayerModel = $state<PlayerModel | null>(null)
  let otherPlayerModels = $state<(PlayerModel | undefined)[]>([])

  // Reference to PlayerControl component
  let playerControl = $state<PlayerControl>()

  // Handle player state changes from PlayerControl
  function handlePlayerStateChange(newState: PlayerState) {
    currentPlayerState = newState
  }

  function rebuildTerrainTiles(centerChunkX: number, centerChunkZ: number) {
    terrainTiles = createTerrainTiles(
      centerChunkX,
      centerChunkZ,
      TERRAIN_TILE_SIZE,
      TERRAIN_GRID_RADIUS
    )
    terrainMeshes = new Array(terrainTiles.length)
  }

  function updateTerrainTilesFromPlayer() {
    if (!currentPlayer) return
    const nextChunk = getTerrainChunkFromPosition(
      currentPlayer.position,
      TERRAIN_TILE_SIZE
    )
    if (
      nextChunk.x === terrainCenterChunk.x &&
      nextChunk.z === terrainCenterChunk.z
    ) {
      return
    }
    terrainCenterChunk = nextChunk
    rebuildTerrainTiles(nextChunk.x, nextChunk.z)
  }

  gameStore.subscribe((state) => {
    currentPlayer = state.currentPlayer
    otherPlayers = state.otherPlayers
    chatBubbles = state.chatBubbles
  })

  // Monster models reference
  let monsterModels = $state<(Monster | undefined)[]>([])

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

      loopProfiler.onFrame(fixedDeltaTime, FRAME_TIME)

      const frameWorkStart = performance.now()

      const realDeltaSeconds = fixedDeltaTime / 1000

      // Apply time scale for slow motion debugging
      const deltaTime = fixedDeltaTime * $timeScale
      const sunDeltaSeconds = realDeltaSeconds * $sunTimeScale
      advanceLocalCalendar(sunDeltaSeconds)
      setGameHour(getLocalGameHour())

      // Calculate camera offset before player movement
      const cameraOffsetStart = performance.now()
      let cameraOffset = calculateCameraOffset()
      loopProfiler.record('cameraOffset', performance.now() - cameraOffsetStart)

      // Update player controls (skip in map editor mode)
      const playerControlStart = performance.now()
      if (playerControl && !$mapEditorMode) {
        playerControl.updateKeyboardMovement()
        playerControl.updatePlayerMovement(deltaTime)
      }
      updateTerrainTilesFromPlayer()
      // Finalize teleport once full 3x3 heightmap grid is loaded
      if ($teleportLoading && currentPlayer &&
          terrainHeightManager.hasHeightDataForGrid(currentPlayer.position.x, currentPlayer.position.z)) {
        currentPlayer.position.y = terrainHeightManager.getHeightAtWorldPosition(
          currentPlayer.position.x, currentPlayer.position.z)
        teleportLoading.set(false)
        resetCameraToInitialState()
        cameraOffset = { ...CAMERA_OFFSET }
      }
      loopProfiler.record('playerControl', performance.now() - playerControlStart)

      // Update remote player interpolation
      const remoteInterpolationStart = performance.now()
      remotePlayerManager.update(deltaTime)
      loopProfiler.record(
        'remoteInterpolation',
        performance.now() - remoteInterpolationStart
      )

      // Update player model animations
      const currentPlayerAnimationStart = performance.now()
      if (currentPlayerModel) {
        currentPlayerModel.update(deltaTime / 1000)
      }
      loopProfiler.record(
        'currentPlayerAnimation',
        performance.now() - currentPlayerAnimationStart
      )

      // Update other player model animations
      const otherPlayerAnimationStart = performance.now()
      for (const playerModel of otherPlayerModels) {
        if (playerModel) {
          playerModel.update(deltaTime / 1000)
        }
      }
      loopProfiler.record(
        'otherPlayerAnimation',
        performance.now() - otherPlayerAnimationStart
      )

      // Update monster animations
      const monsterAnimationStart = performance.now()
      for (const monsterModel of monsterModels) {
        if (monsterModel) {
          monsterModel.update(deltaTime / 1000, camera) // Convert ms to seconds for THREE.AnimationMixer
        }
      }
      loopProfiler.record('monsterAnimation', performance.now() - monsterAnimationStart)

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
      loopProfiler.record('monsterLogic', performance.now() - monsterLogicStart)

      // Update camera with preserved offset
      const cameraUpdateStart = performance.now()
      updateCameraWithOffset(cameraOffset)
      loopProfiler.record('cameraUpdate', performance.now() - cameraUpdateStart)

      // Update directional light to follow player
      const lightUpdateStart = performance.now()
      updateLightPosition()
      loopProfiler.record('lightUpdate', performance.now() - lightUpdateStart)

      // Update water uniforms
      waterTime += realDeltaSeconds
      if (directionalLight) {
        _waterSunDirTmp.copy(directionalLight.position).sub(directionalLight.target.position).normalize()
        waterSunDir = _waterSunDirTmp.clone()
        waterSunColor = directionalLight.color.clone()
      }

      loopProfiler.record('frameWork', performance.now() - frameWorkStart)

      if (unclampedSteps > MAX_CATCH_UP_STEPS) {
        // Drop excessive backlog after long stalls (tab switch, debugger pause, etc.).
        lastFrameTime = currentTime - FRAME_TIME
      } else {
        lastFrameTime += fixedDeltaTime
      }
    }

    loopProfiler.flush(currentTime)

    // Always continue the loop
    gameLoopId = requestAnimationFrame(gameLoop)
  }

  function calculateCameraOffset() {
    const offset = getCameraOffsetFromScene(
      camera,
      currentPlayer?.position ?? null,
      CAMERA_OFFSET
    )
    if (camera) {
      // Update camera "zoom metric" for debug UI.
      cameraDistance.set(camera.zoom)
    }
    return offset
  }

  function updateCameraWithOffset(offset: { x: number; y: number; z: number }) {
    if (!currentPlayer || !camera) return
    // In map editor mode, scale the base offset to raise the camera when zoomed out
    // so bottom view rays still intersect the ground plane.
    if ($mapEditorMode && camera.zoom < 1) {
      const maxBelow = INITIAL_DISTANCE / Math.SQRT2
      const scale = Math.max(1, (ORTHOGRAPHIC_FRUSTUM_HEIGHT / 2) / (camera.zoom * maxBelow))
      cameraTarget = applyCameraOffset(camera, currentPlayer.position, {
        x: CAMERA_OFFSET.x * scale,
        y: CAMERA_OFFSET.y * scale,
        z: CAMERA_OFFSET.z * scale,
      })
    } else {
      cameraTarget = applyCameraOffset(camera, currentPlayer.position, offset)
    }
  }

  function resetCameraToInitialState() {
    if (!currentPlayer || !camera) return
    cameraTarget = resetCameraToDefaultState(
      camera,
      currentPlayer.position,
      CAMERA_OFFSET
    )
    cameraDistance.set(camera.zoom)
  }

  function updateLightPosition() {
    sceneLighting.update({
      currentPlayerPosition: currentPlayer?.position ?? null,
      localCalendarDate,
      ambientLight,
      directionalLight,
      sunLightSnapshot: computeSunLightSnapshot(
        getLocalGameHour(),
        localCalendarDate
      ),
      eclipseFactor: eclipseState.factor,
    })
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
    loopProfiler.resetWindow(performance.now())
    setGameHour(getLocalGameHour())
    syncCalendarToWidget()

    const unsubscribeServerGameTime = serverGameTime.subscribe((gameTime) => {
      latestServerGameTime = gameTime
      applyServerGameHourIfAllowed()
    })

    const unsubscribeSunTimeScale = sunTimeScale.subscribe((scale) => {
      const wasFastSun = latestSunTimeScale > 1
      latestSunTimeScale = scale
      if (wasFastSun && scale <= 1) {
        applyServerGameHourIfAllowed()
      }
    })

    const unsubscribeViewportSize = size.subscribe((nextSize) => {
      viewportSize = nextSize
    })

    const unsubscribeCameraReset = cameraResetNonce.subscribe((nonce) => {
      // Ignore initial store emission; only react to explicit reset requests.
      if (nonce > 0) {
        resetCameraToInitialState()
      }
    })

    const pmremGenerator = new THREE.PMREMGenerator(renderer)
    scene.environment = pmremGenerator.fromScene(new RoomEnvironment()).texture
    scene.environmentIntensity = 0.5
    pmremGenerator.dispose()

    terrainGeometry = createTerrainGeometry(TERRAIN_TILE_SIZE, TERRAIN_TILE_SEGMENTS)
    loadWaterNormalMap().then((tex) => { waterNormalMap = tex })
    rebuildTerrainTiles(terrainCenterChunk.x, terrainCenterChunk.z)
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
      scene.environment?.dispose()
      scene.environment = null
      unsubscribeViewportSize()
      unsubscribeCameraReset()
      unsubscribeServerGameTime()
      unsubscribeSunTimeScale()
      stopGameLoop()
      stopChatBubbleChecker()
      networkManager.disconnect()
      monsterManager.reset()
      remotePlayerManager.reset()
      playerDebugInfo.set(null)
      terrainHeightManager.destroy()
      terrainSplatManager.destroy()
      waterNormalMap?.dispose()
      waterNormalMap = null
      terrainTiles = []
      terrainMeshes = []
      resetGameStore()
    }
  })
</script>

<T.OrthographicCamera
  bind:ref={camera}
  makeDefault
  zoom={ORTHOGRAPHIC_DEFAULT_ZOOM}
>
  <OrbitControls
    enableRotate={$mapEditorMode ? false : $cameraRotationEnabled}
    enablePan={false}
    enableZoom={!$mapEditorMode}
    enabled={!$mapEditorMode}
    target={cameraTarget}
    minZoom={1}
    maxZoom={2}
  />
</T.OrthographicCamera>

<T.DirectionalLight
  bind:ref={directionalLight}
  position={[10, 10, 10]}
  intensity={SUN_MAX_INTENSITY}
  castShadow
  shadow.camera.left={-SHADOW_CAMERA_EXTENT}
  shadow.camera.right={SHADOW_CAMERA_EXTENT}
  shadow.camera.top={SHADOW_CAMERA_EXTENT}
  shadow.camera.bottom={-SHADOW_CAMERA_EXTENT}
  shadow.camera.near={1}
  shadow.camera.far={SHADOW_CAMERA_FAR}
  shadow.bias={-0.0002}
  shadow.normalBias={0.01}
  shadow.mapSize.width={2048}
  shadow.mapSize.height={2048}
/>
<T.AmbientLight
  bind:ref={ambientLight}
  intensity={sceneLighting.ambientDayIntensity}
  color="#ffffff"
/>

<GameSceneTerrainLayer
  {terrainGeometry}
  {terrainTiles}
  bind:terrainMeshes={terrainMeshes}
  heightManager={terrainHeightManager}
  splatManager={terrainSplatManager}
/>

<GameSceneWaterLayer
  {terrainGeometry}
  {terrainTiles}
  heightManager={terrainHeightManager}
  normalMap={waterNormalMap}
  time={waterTime}
  sunDirection={waterSunDir}
  sunColor={waterSunColor}
/>

<!-- Terrain Field - 3x3 grid of field inspection models (commented out) -->
<!-- <TerrainField /> -->

<GameScenePlayersLayer
  {camera}
  {cameraInitialized}
  {currentPlayer}
  {otherPlayers}
  remotePlayers={remotePlayerManager.players}
  {chatBubbles}
  {currentPlayerState}
  {terrainMeshes}
  {monsterModels}
  {playerAttackDuration}
  heightManager={terrainHeightManager}
  onStateChange={handlePlayerStateChange}
  onAttackDuration={(duration) => (playerAttackDuration = duration)}
  {onCurrentPlayerDyingFinished}
  bind:isCurrentPlayerLoading={isCurrentPlayerLoading}
  bind:playerControl={playerControl}
  bind:currentPlayerModel={currentPlayerModel}
  bind:otherPlayerModels={otherPlayerModels}
/>

<GameSceneMonstersLayer
  monsters={monsterManager.monsters}
  bind:monsterModels={monsterModels}
/>

{#if $mapEditorMode}
  <MapEditorCursor {camera} {terrainMeshes} {terrainTiles} heightManager={terrainHeightManager} splatManager={terrainSplatManager} />
{/if}
