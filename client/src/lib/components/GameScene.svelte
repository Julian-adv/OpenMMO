<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import { OrbitControls } from '@threlte/extras'
  import * as THREE from 'three'
  import { ClippingGroup, type WebGPURenderer } from 'three/webgpu'
  import { CSMShadowNode } from 'three/addons/csm/CSMShadowNode.js'
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
  import GameSceneGrassLayer from './game-scene/GameSceneGrassLayer.svelte'
  import GameSceneWindParticles from './game-scene/GameSceneWindParticles.svelte'
  import GameSceneHousingLayer from './game-scene/GameSceneHousingLayer.svelte'
  import { drainTileWork } from '../utils/tileWorkQueue'
  import GameScenePlayersLayer from './game-scene/GameScenePlayersLayer.svelte'
  import GameSceneMonstersLayer from './game-scene/GameSceneMonstersLayer.svelte'
  import MapEditorCursor from './map-editor/MapEditorCursor.svelte'
  import HousingEditorCursor from './map-editor/HousingEditorCursor.svelte'
  import { type PlayerState } from '../utils/movementUtils'
  import {
    SUN_MAX_INTENSITY,
    computeSunLightSnapshot,
  } from '../utils/celestialSimulation'
  import { cameraDistance, cameraResetNonce } from '../stores/cameraStore'
  import {
    timeScale,
    sunTimeScale,
    serverGameTime,
  } from '../stores/timeStore'
  import {
    debugVisible,
    cameraRotationEnabled,
    playerDebugInfo,
    mapEditorMode,
    teleportLoading,
    debugSpeedMode,
    refractionEnabled,
    reflectionEnabled,
    housingEditorMode,
  } from '../stores/debugStore'
  import { editorPanOffset, editorHeightManager, editorSplatManager, editorMetaManager, editorGrassDataManager, terrainForceRebuild } from '../stores/editorStore'
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
    TERRAIN_TILE_SIZE,
    type TerrainTile,
  } from './game-scene/terrain-utils'
  import { createLoopProfiler } from './game-scene/loop-profiler'
  import { createSceneLightingController } from './game-scene/scene-lighting'
  import { TerrainHeightManager } from '../managers/terrainHeightManager'
  import { TerrainSplatManager } from '../managers/terrainSplatManager'
  import { TerrainMetaManager } from '../managers/terrainMetaManager'
  import { TerrainGrassDataManager } from '../managers/terrainGrassDataManager'
  import { loadSplatLayers } from '../utils/splatLayerLoader'
  import {
    loadGrassBillboardGeometry,
    loadFlowerBillboardGeometry,
    loadGrassAlphaTexture,
    loadFlowerColorTexture,
  } from '../shaders/grass-material'
  import { createCalendarSystem } from './game-scene/calendar-system'
  import { createTerrainTileManager } from './game-scene/terrain-tile-manager'
  import { registerDebugConsole } from './game-scene/debug-console'
  import { initScene } from './game-scene/scene-init'
  import { createMultiPassRenderer } from './game-scene/multi-pass-rendering'

  interface Props {
    serverUrl: string
    onCurrentPlayerDyingFinished?: () => void
    isCurrentPlayerLoading?: boolean
    isSceneCompiling?: boolean
  }

  let { serverUrl, onCurrentPlayerDyingFinished, isCurrentPlayerLoading = $bindable(false), isSceneCompiling = $bindable(true) }: Props = $props()

  let currentPlayer = $state<LocalPlayer | null>(null)
  let otherPlayers = $state<Map<string, RemotePlayer>>(new Map())
  let chatBubbles = $state<Map<string, ChatBubble>>(new Map())
  let camera = $state<THREE.OrthographicCamera | undefined>(undefined)
  let directionalLight = $state<THREE.DirectionalLight | undefined>(undefined)
  let ambientLight = $state<THREE.AmbientLight | undefined>(undefined)
  let terrainMeshes = $state<(THREE.Mesh | undefined)[]>([])
  let terrainGroup = $state<THREE.Group | undefined>(undefined)
  let syncTileMeshes = $state<() => void>(() => {})
  let terrainGeometry = $state<THREE.BufferGeometry | null>(null)
  let terrainTiles = $state<TerrainTile[]>([])
  let terrainCenterChunk = $state({ x: 0, z: 0 })
  const terrainHeightManager = new TerrainHeightManager()
  const terrainSplatManager = new TerrainSplatManager()
  const terrainMetaManager = new TerrainMetaManager()
  const terrainGrassDataManager = new TerrainGrassDataManager()
  monsterManager.heightManager = terrainHeightManager
  editorHeightManager.set(terrainHeightManager)
  editorSplatManager.set(terrainSplatManager)
  editorMetaManager.set(terrainMetaManager)
  editorGrassDataManager.set(terrainGrassDataManager)
  let waterNormalMap = $state<THREE.Texture | null>(null)
  let waterFoamMap = $state<THREE.Texture | null>(null)
  let waterCausticsMap = $state<THREE.Texture | null>(null)
  let waterTime = $state(0)
  let waterSunDir = $state<THREE.Vector3 | null>(null)
  let waterSunColor = $state<THREE.Color | null>(null)
  let waterCamDir = $state<THREE.Vector3 | null>(null)
  let waterMoonBrightness = $state(0)
  let waterGroup = $state<THREE.Group | undefined>(undefined)
  let waterLayerRef = $state<GameSceneWaterLayer | undefined>(undefined)
  let grassLayerRef = $state<GameSceneGrassLayer | undefined>(undefined)
  let windParticlesRef = $state<GameSceneWindParticles | undefined>(undefined)
  let housingLayerRef = $state<GameSceneHousingLayer | undefined>(undefined)
  let entityClipGroup = $state<ClippingGroup | undefined>(undefined)
  /** ClippingGroup instance with Y=0 clip plane, starts disabled. */
  const entityClipGroupObj = (() => {
    const g = new ClippingGroup()
    g.clippingPlanes = [new THREE.Plane(new THREE.Vector3(0, 1, 0), 0)]
    g.enabled = false
    return g
  })()
  const waterSunDirTmp = new THREE.Vector3()
  const waterCamDirTmp = new THREE.Vector3()
  let refractionManager = $state<import('../managers/refractionRenderManager').RefractionRenderManager | null>(null)
  let refractionTexture = $state<THREE.Texture | null>(null)
  let reflectionManager = $state<import('../managers/reflectionRenderManager').ReflectionRenderManager | null>(null)
  let reflectionTexture = $state<THREE.Texture | null>(null)
  let cameraInitialized = $state(false)
  let playerAttackDuration = $state(1.533) // Default from slash1 animation (data/animation_durations.json)

  const multiPassRenderer = createMultiPassRenderer()

  // Track whether all initial data is loaded (terrain + splat + grass assets).
  // The loading dialog stays until frames render smoothly (pipeline compilation
  // done by Threlte's render loop under the dialog overlay).
  let initialDataReady = false
  let smoothFrameCount = 0
  const SMOOTH_FRAME_THRESHOLD = 3 // consecutive smooth frames to consider ready
  const SMOOTH_FRAME_TIME_MS = 100 // frame must be under this to count


  // Camera follow system
  let cameraTarget = $state<[number, number, number]>([0, 0, 0])

  const { size, renderer: _renderer, scene } = useThrelte()
  // Cast renderer — Threlte types it as WebGLRenderer but we use WebGPURenderer via createRenderer
  const renderer = _renderer as unknown as WebGPURenderer
  let viewportSize = $state({ width: 1, height: 1 })

  const CAMERA_OFFSET = import.meta.hot?.data?.cameraOffset ?? { ...DEFAULT_CAMERA_OFFSET }
  let _hmrCameraZoom: number | null = import.meta.hot?.data?.cameraZoom ?? null
  let _hmrCameraInitialized: boolean = import.meta.hot?.data?.cameraInitialized ?? false

  if (import.meta.hot) {
    import.meta.hot.dispose((data) => {
      data.cameraOffset = { ...CAMERA_OFFSET }
      data.cameraInitialized = cameraInitialized
      data.cameraZoom = camera?.zoom ?? null
    })
  }

  // Initialize camera as soon as both camera ref and currentPlayer are available
  $effect(() => {
    if (!cameraInitialized && camera && currentPlayer) {
      resetCameraToInitialState()
      cameraInitialized = true
    }
  })

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

  // Reset camera and pan offset when entering/leaving map editor mode
  let prevMapEditorMode = $state(false)
  $effect(() => {
    const current = $mapEditorMode
    if (prevMapEditorMode !== current) {
      // Clear pan offset first so resetCameraRotation computes the correct distance
      editorPanOffset.set({ x: 0, z: 0 })
      resetCameraRotation()
    }
    prevMapEditorMode = current
  })

  $effect(() => {
    updateOrthographicFrustum(camera, viewportSize)
  })

  const sceneLighting = createSceneLightingController()

  // Cascaded Shadow Maps for directional light
  const CSM_MAX_FAR = 200
  const CSM_CASCADES = 2
  $effect(() => {
    if (!directionalLight) return
    const csm = new CSMShadowNode(directionalLight, {
      cascades: CSM_CASCADES,
      maxFar: CSM_MAX_FAR,
      mode: 'practical',
      lightMargin: 100,
    })
    csm.fade = true
    directionalLight.shadow.shadowNode = csm
  })

  const calendarSystem = createCalendarSystem({
    onDateChanged: setGameDate,
    onHourChanged: setGameHour,
  })

  let latestServerGameTime: import('../stores/timeStore').ServerGameTime | null = null
  let latestSunTimeScale = 1

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

  // Reference to PlayerControl and PlayersLayer components
  let playerControl = $state<PlayerControl>()
  let playersLayer = $state<GameScenePlayersLayer>()

  // Handle player state changes from PlayerControl
  function handlePlayerStateChange(newState: PlayerState) {
    currentPlayerState = newState
  }

  const tileManager = createTerrainTileManager({
    getTiles: () => terrainTiles,
    setTiles: (tiles) => { terrainTiles = tiles },
    getCenterChunk: () => terrainCenterChunk,
    setCenterChunk: (chunk) => { terrainCenterChunk = chunk },
  })

  // Force terrain rebuild when requested (e.g. after region delete/generate)
  let lastRebuildVersion = 0
  $effect(() => {
    const v = $terrainForceRebuild
    if (v > lastRebuildVersion) {
      lastRebuildVersion = v
      tileManager.resetForForceRebuild()
    }
  })

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
      calendarSystem.advance(sunDeltaSeconds)
      setGameHour(calendarSystem.getGameHour())

      // Calculate camera offset before player movement
      const cameraOffsetStart = performance.now()
      let cameraOffset = calculateCameraOffset()
      loopProfiler.record('cameraOffset', performance.now() - cameraOffsetStart)

      // Update player controls (skip in map editor mode)
      const playerControlStart = performance.now()
      if (playerControl && !$mapEditorMode && !$housingEditorMode) {
        playerControl.checkInteraction()
        playerControl.updateKeyboardMovement()
        playerControl.updatePlayerMovement(deltaTime)
      }
      tileManager.updateFromPlayerPosition(currentPlayer?.position ?? null)
      tileManager.drainQueue()
      drainTileWork()
      syncTileMeshes()
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

      // Update remote shadow light flickering
      playersLayer?.updateRemoteShadowFlicker(deltaTime / 1000)

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

      // Update housing (player-inside detection + front wall toggling)
      {
        const housingStart = performance.now()
        housingLayerRef?.update(deltaTime)
        loopProfiler.record('housingUpdate', performance.now() - housingStart)
      }

      // Update grass wind & trail
      {
        const grassStart = performance.now()
        grassLayerRef?.update(deltaTime)
        loopProfiler.record('grassUpdate', performance.now() - grassStart)
      }

      // Update wind-blown particles (only when grass is visible nearby)
      {
        const windStart = performance.now()
        const windState = grassLayerRef?.getWindState()
        const grassCount = grassLayerRef?.getPlayerChunkGrassCount() ?? 0
        if (windState) windParticlesRef?.update(deltaTime, camera, windState, grassCount)
        loopProfiler.record('windParticles', performance.now() - windStart)
      }

      // Update camera with preserved offset
      const cameraUpdateStart = performance.now()
      updateCameraWithOffset(cameraOffset)
      loopProfiler.record('cameraUpdate', performance.now() - cameraUpdateStart)

      // Update directional light to follow player
      const lightUpdateStart = performance.now()
      updateLightPosition()
      loopProfiler.record('lightUpdate', performance.now() - lightUpdateStart)

      // Update water uniforms — always use real sun direction (not moon)
      waterTime += realDeltaSeconds
      {
        const sunSnapshot = computeSunLightSnapshot(calendarSystem.getGameHour(), calendarSystem.getDate())
        waterSunDirTmp.set(sunSnapshot.direction.x, sunSnapshot.direction.y, sunSnapshot.direction.z)
        waterSunDir = waterSunDirTmp.clone()
      }
      if (directionalLight) {
        waterSunColor = directionalLight.color.clone()
        // Moon brightness: use directional light intensity when sun is below horizon
        waterMoonBrightness = waterSunDirTmp.y <= 0 ? directionalLight.intensity : 0
      }
      if (camera) {
        camera.getWorldDirection(waterCamDirTmp)
        waterCamDir = waterCamDirTmp.clone()
      }

      // Track draw calls across all render passes in this frame.
      // renderer.info auto-resets on each render() call, so we snapshot after each pass.
      // Render wetness pre-pass (small 256x256 RT per water tile).
      // Not gated behind multiPassReady — it's a tiny RT with negligible
      // pipeline overhead, and deferring it causes blocky wet sand.
      {
        const wetnessStart = performance.now()
        waterLayerRef?.renderWetness(renderer)
        loopProfiler.record('wetnessPass', performance.now() - wetnessStart)
      }

      // Multi-pass warmup + refraction/reflection
      multiPassRenderer.tickWarmup(isSceneCompiling)

      multiPassRenderer.renderRefraction({
        camera, refractionManager, refractionEnabled: $refractionEnabled,
        waterGroup, terrainMeshes, entityClipGroup,
        grassGroup: grassLayerRef?.getGroup(),
        windParticlesGroup: windParticlesRef?.getGroup(),
      }, loopProfiler)

      multiPassRenderer.renderReflection({
        camera, reflectionManager, reflectionEnabled: $reflectionEnabled,
        waterGroup, terrainGroup, housingGroup: housingLayerRef?.getGroup(),
        entityClipGroup,
        grassGroup: grassLayerRef?.getGroup(),
        windParticlesGroup: windParticlesRef?.getGroup(),
        getNametagGroups: () => {
          const groups: THREE.Group[] = []
          const ntCurrent = currentPlayerModel?.getNametagGroup()
          if (ntCurrent) groups.push(ntCurrent)
          for (const pm of otherPlayerModels) {
            const nt = pm?.getNametagGroup()
            if (nt) groups.push(nt)
          }
          for (const mm of monsterModels) {
            const nt = mm?.getNametagGroup()
            if (nt) groups.push(nt)
          }
          return groups
        },
      }, loopProfiler)

      const frameWorkMs = performance.now() - frameWorkStart
      loopProfiler.record('frameWork', frameWorkMs)

      // Detect when pipeline compilation is done: once data is ready,
      // wait for a few consecutive smooth frames before hiding the loading dialog.
      if (isSceneCompiling && initialDataReady) {
        if (rawDeltaTime < SMOOTH_FRAME_TIME_MS) {
          smoothFrameCount++
          if (smoothFrameCount >= SMOOTH_FRAME_THRESHOLD) {
            isSceneCompiling = false
          }
        } else {
          smoothFrameCount = 0
        }
      }

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

    if ($mapEditorMode || $housingEditorMode) {
      // Apply editor pan offset so middle-mouse drag moves the viewport
      const pan = $editorPanOffset
      const panPos = {
        x: currentPlayer.position.x + pan.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z + pan.z,
      }
      // Always use fixed CAMERA_OFFSET in editor mode (OrbitControls is disabled,
      // so we must not feed back the computed offset which includes the pan).
      if (camera.zoom < 1) {
        const maxBelow = INITIAL_DISTANCE / Math.SQRT2
        const scale = Math.max(1, (ORTHOGRAPHIC_FRUSTUM_HEIGHT / 2) / (camera.zoom * maxBelow))
        cameraTarget = applyCameraOffset(camera, panPos, {
          x: CAMERA_OFFSET.x * scale,
          y: CAMERA_OFFSET.y * scale,
          z: CAMERA_OFFSET.z * scale,
        })
      } else {
        cameraTarget = applyCameraOffset(camera, panPos, CAMERA_OFFSET)
      }
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
    const calDate = calendarSystem.getDate()
    sceneLighting.update({
      currentPlayerPosition: currentPlayer?.position ?? null,
      localCalendarDate: calDate,
      ambientLight,
      directionalLight,
      scene,
      sunLightSnapshot: computeSunLightSnapshot(
        calendarSystem.getGameHour(),
        calDate,
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

    const cleanupDebugConsole = registerDebugConsole(() => ({
      loopProfiler,
      getLoopProfileEnabled: () => loopProfileEnabled,
      setLoopProfileEnabled: (v) => { loopProfileEnabled = v },
      renderer,
      scene,
      getGrassGroup: () => grassLayerRef?.getGroup(),
      getHousingGroup: () => housingLayerRef?.getGroup(),
      getTerrainGroup: () => terrainGroup,
      refractionEnabled,
      reflectionEnabled,
    }))
    setGameHour(calendarSystem.getGameHour())

    const unsubscribeServerGameTime = serverGameTime.subscribe((gameTime) => {
      latestServerGameTime = gameTime
      calendarSystem.applyServerTimeIfAllowed(latestServerGameTime, latestSunTimeScale)
    })

    const unsubscribeSunTimeScale = sunTimeScale.subscribe((scale) => {
      const wasFastSun = latestSunTimeScale > 1
      latestSunTimeScale = scale
      if (wasFastSun && scale <= 1) {
        calendarSystem.applyServerTimeIfAllowed(latestServerGameTime, latestSunTimeScale)
      }
    })

    const unsubscribeViewportSize = size.subscribe((nextSize) => {
      viewportSize = nextSize
      if (refractionManager) refractionManager.resize(nextSize.width, nextSize.height)
      if (reflectionManager) reflectionManager.resize(nextSize.width, nextSize.height)
    })

    const unsubscribeCameraReset = cameraResetNonce.subscribe((nonce) => {
      // Ignore initial store emission; only react to explicit reset requests.
      if (nonce > 0) {
        resetCameraToInitialState()
      }
    })

    const sceneRes = initScene(renderer, scene, viewportSize.width, viewportSize.height)
    terrainGeometry = sceneRes.terrainGeometry
    waterNormalMap = sceneRes.waterNormalMap
    sceneRes.waterFoamMapPromise.then((tex) => { waterFoamMap = tex })
    sceneRes.waterCausticsMapPromise.then((tex) => { waterCausticsMap = tex })
    refractionManager = sceneRes.refractionManager
    refractionTexture = sceneRes.refractionTexture
    reflectionManager = sceneRes.reflectionManager
    reflectionTexture = sceneRes.reflectionTexture

    // Load all terrain tiles immediately (no staggering during initial load)
    tileManager.rebuild(terrainCenterChunk.x, terrainCenterChunk.z)
    tileManager.drainAll()

    // Pre-fetch all tile heightmaps so they're cached when the TerrainLayer
    // $effect fires. This allows work items to be enqueued immediately.
    const tileCoords = terrainTiles.map((t) => ({
      x: Math.round(t.position[0] / TERRAIN_TILE_SIZE),
      z: Math.round(t.position[2] / TERRAIN_TILE_SIZE),
    }))
    const heightPromises = tileCoords.map((c) =>
      terrainHeightManager.loadHeightmap(c.x, c.z).catch(() => {})
    )

    const splatPromise = loadSplatLayers()

    // Also await grass asset loading so grass materials can be compiled
    const grassAssetsPromise = Promise.all([
      loadGrassBillboardGeometry(),
      loadFlowerBillboardGeometry(),
      loadGrassAlphaTexture(),
      loadFlowerColorTexture(),
    ])

    // Wait for terrain data + grass assets, let the TerrainLayer $effect run
    // and enqueue work, eagerly create grass materials, then let the renderer
    // compile pipelines while the loading dialog is still visible.
    Promise.all([splatPromise, grassAssetsPromise, ...heightPromises]).then(() => {
      // Wait two frames: one for Svelte to flush the $effect that enqueues
      // tile work, and another to ensure all microtask .then() chains complete.
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          drainTileWork(Infinity)

          // Eagerly create grass materials + meshes so Threlte's render loop
          // compiles their pipelines while the loading dialog is still visible.
          grassLayerRef?.ensureMaterialsForCompile()
          // Wind particles: lazy init on first spawn (MeshBasicNodeMaterial
          // compiles fast, not worth blocking the loading screen for)

          // Mark data as ready. Threlte's render loop compiles WebGPU pipelines
          // on-demand (synchronously per frame) under the loading dialog overlay.
          // The smooth frame detector waits until compilation is done.
          initialDataReady = true
        })
      })
    })

    // Start game loop
    lastFrameTime = performance.now()
    initFpsCounting()
    gameLoopId = requestAnimationFrame(gameLoop)

    // Start chat bubble expiration checker
    startChatBubbleChecker()

    networkManager.connect(serverUrl)

    // Initialize camera position: HMR restores previous state,
    // otherwise a reactive $effect waits for camera + player to be ready.
    if (_hmrCameraInitialized) {
      cameraInitialized = true
      if (_hmrCameraZoom !== null) {
        const restoreZoom = _hmrCameraZoom
        requestAnimationFrame(() => {
          if (camera) {
            camera.zoom = restoreZoom
            camera.updateProjectionMatrix()
          }
        })
      }
      _hmrCameraZoom = null
    }

    return () => {
      cleanupDebugConsole()
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
      waterFoamMap?.dispose()
      waterFoamMap = null
      waterCausticsMap?.dispose()
      waterCausticsMap = null
      refractionManager?.dispose()
      refractionManager = null
      refractionTexture = null
      reflectionManager?.dispose()
      reflectionManager = null
      reflectionTexture = null
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
    enableRotate={$mapEditorMode || $housingEditorMode ? false : $cameraRotationEnabled}
    enablePan={false}
    enableZoom={!$mapEditorMode && !$housingEditorMode}
    enabled={!$mapEditorMode && !$housingEditorMode}
    target={cameraTarget}
    minZoom={$debugSpeedMode ? 0.15 : 1}
    maxZoom={2}
  />
</T.OrthographicCamera>

<T.DirectionalLight
  bind:ref={directionalLight}
  position={[10, 10, 10]}
  intensity={SUN_MAX_INTENSITY}
  castShadow
  shadow.bias={-0.0002}
  shadow.normalBias={0.15}
  shadow.mapSize.width={2048}
  shadow.mapSize.height={2048}
/>
<T.AmbientLight
  bind:ref={ambientLight}
  intensity={sceneLighting.ambientDayIntensity}
  color="#ffffff"
/>
<!-- Placeholder shadow-casting PointLight so WebGPU compiles ALL material
     pipelines with point-light shadow support from the start. Without this,
     adding the player's torch later triggers a cascade recompilation of every
     existing material (~12s stall). Intensity 0 = invisible but pipelines
     are compiled with shadow support. -->
<T.PointLight
  position={[0, 0, 0]}
  intensity={0}
  distance={50}
  decay={1.2}
  castShadow
  shadow.mapSize.width={512}
  shadow.mapSize.height={512}
  shadow.camera.near={0.5}
  shadow.camera.far={50}
/>

<GameSceneTerrainLayer
  {terrainGeometry}
  {terrainTiles}
  bind:terrainMeshes={terrainMeshes}
  bind:terrainGroup={terrainGroup}
  bind:syncTileMeshes={syncTileMeshes}
  heightManager={terrainHeightManager}
  splatManager={terrainSplatManager}
  metaManager={terrainMetaManager}
/>

<GameSceneHousingLayer
  bind:this={housingLayerRef}
  playerPosition={currentPlayer?.position ?? null}
/>

<GameSceneGrassLayer
  bind:this={grassLayerRef}
  {terrainTiles}
  grassDataManager={terrainGrassDataManager}
  playerPosition={currentPlayer?.position ?? null}
/>

<GameSceneWindParticles
  bind:this={windParticlesRef}
  playerPosition={currentPlayer?.position ?? null}
/>

<GameSceneWaterLayer
  bind:this={waterLayerRef}
  {terrainGeometry}
  {terrainTiles}
  heightManager={terrainHeightManager}
  normalMap={waterNormalMap}
  foamMap={waterFoamMap}
  causticsMap={waterCausticsMap}
  time={waterTime}
  sunDirection={waterSunDir}
  sunColor={waterSunColor}
  cameraDirection={waterCamDir}
  moonBrightness={waterMoonBrightness}
  refractionMap={refractionTexture}
  reflectionMap={reflectionTexture}
  bind:waterGroup={waterGroup}
/>

<!-- Terrain Field - 3x3 grid of field inspection models (commented out) -->
<!-- <TerrainField /> -->

<T is={entityClipGroupObj} bind:ref={entityClipGroup}>
  <GameScenePlayersLayer
    bind:this={playersLayer}
    {camera}
    {cameraInitialized}
    {currentPlayer}
    {otherPlayers}
    remotePlayers={remotePlayerManager.players}
    {chatBubbles}
    {currentPlayerState}
    {terrainMeshes}
    housingGroup={housingLayerRef?.getGroup() ?? null}
    doorMeshes={housingLayerRef?.getDoorMeshes() ?? []}
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
</T>

{#if $mapEditorMode}
  <MapEditorCursor {camera} {terrainMeshes} {terrainTiles} heightManager={terrainHeightManager} splatManager={terrainSplatManager} metaManager={terrainMetaManager} />
{/if}

{#if $housingEditorMode}
  <HousingEditorCursor {camera} {terrainMeshes} heightManager={terrainHeightManager} grassDataManager={terrainGrassDataManager} housingGroup={housingLayerRef?.getGroup() ?? null} />
{/if}
