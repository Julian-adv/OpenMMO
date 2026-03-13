<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import { OrbitControls } from '@threlte/extras'
  import * as THREE from 'three'
  import { PMREMGenerator, ClippingGroup, type WebGPURenderer } from 'three/webgpu'
  import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js'
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
  import { drainTileWork } from '../utils/tileWorkQueue'
  import GameScenePlayersLayer from './game-scene/GameScenePlayersLayer.svelte'
  import GameSceneMonstersLayer from './game-scene/GameSceneMonstersLayer.svelte'
  import MapEditorCursor from './map-editor/MapEditorCursor.svelte'
  import { type PlayerState } from '../utils/movementUtils'
  import {
    GAME_START_YEAR,
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
    debugSpeedMode,
    refractionEnabled,
    reflectionEnabled,
  } from '../stores/debugStore'
  import { editorPanOffset, editorHeightManager, editorSplatManager, editorMetaManager, terrainForceRebuild } from '../stores/editorStore'
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
  import { TerrainMetaManager } from '../managers/terrainMetaManager'
  import { loadFoamTexture } from '../shaders/water-foam-gen'
  import { generateCausticsTexture } from '../shaders/caustics-gen'
  import { RefractionRenderManager } from '../managers/refractionRenderManager'
  import { ReflectionRenderManager } from '../managers/reflectionRenderManager'
  import { loadSplatLayers } from '../utils/splatLayerLoader'

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
  monsterManager.heightManager = terrainHeightManager
  editorHeightManager.set(terrainHeightManager)
  editorSplatManager.set(terrainSplatManager)
  editorMetaManager.set(terrainMetaManager)
  let waterNormalMap = $state<THREE.Texture | null>(null)
  let waterFoamMap = $state<THREE.Texture | null>(null)
  let waterCausticsMap = $state<THREE.Texture | null>(null)
  let waterTime = $state(0)
  let waterSunDir = $state<THREE.Vector3 | null>(null)
  let waterSunColor = $state<THREE.Color | null>(null)
  let waterCamDir = $state<THREE.Vector3 | null>(null)
  let waterMoonBrightness = $state(0)
  let waterGroup = $state<THREE.Group | undefined>(undefined)
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
  let refractionManager = $state<RefractionRenderManager | null>(null)
  let refractionTexture = $state<THREE.Texture | null>(null)
  let reflectionManager = $state<ReflectionRenderManager | null>(null)
  let reflectionTexture = $state<THREE.Texture | null>(null)
  let cameraInitialized = $state(false)
  let playerAttackDuration = $state(1.533) // Default from slash1 animation (data/animation_durations.json)

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

  // Reference to PlayerControl and PlayersLayer components
  let playerControl = $state<PlayerControl>()
  let playersLayer = $state<GameScenePlayersLayer>()

  // Handle player state changes from PlayerControl
  function handlePlayerStateChange(newState: PlayerState) {
    currentPlayerState = newState
  }

  // Queue for staggered tile loading: add one new tile per frame
  // to spread geometry cloning + heightmap application across frames.
  let pendingTileQueue: TerrainTile[] = []

  function rebuildTerrainTiles(centerChunkX: number, centerChunkZ: number) {
    const allTiles = createTerrainTiles(
      centerChunkX,
      centerChunkZ,
      TERRAIN_TILE_SIZE,
      TERRAIN_GRID_RADIUS
    )

    const newTileIds = new Set(allTiles.map((t) => t.id))
    const keptTiles = terrainTiles.filter((t) => newTileIds.has(t.id))
    const keptIds = new Set(keptTiles.map((t) => t.id))

    // Immediately keep existing tiles and remove stale ones.
    // Do NOT reset terrainMeshes to a new sparse array — that would
    // null out kept tile mesh refs during transitions.
    // The #each block's keyed bind:mesh will naturally update the indices.
    terrainTiles = keptTiles

    // Queue truly new tiles for one-per-frame loading
    pendingTileQueue = allTiles.filter((t) => !keptIds.has(t.id))
  }

  function drainTileQueue() {
    if (pendingTileQueue.length === 0) return
    const tile = pendingTileQueue.shift()!
    terrainTiles = [...terrainTiles, tile]
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

  // Force terrain rebuild when requested (e.g. after region delete/generate)
  let lastRebuildVersion = 0
  $effect(() => {
    const v = $terrainForceRebuild
    if (v > lastRebuildVersion) {
      lastRebuildVersion = v
      // Clear all existing tiles so they are treated as new and reload from server
      terrainTiles = []
      pendingTileQueue = []
      terrainCenterChunk = { x: NaN, z: NaN }
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
      drainTileQueue()
      drainTileWork(2)
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
        const sunSnapshot = computeSunLightSnapshot(getLocalGameHour(), localCalendarDate)
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

      // Render refraction pass (scene without water or entities — terrain only)
      if (refractionManager && $refractionEnabled) {
        if (camera) refractionManager.setCamera(camera)
        if (waterGroup) refractionManager.setWaterGroup(waterGroup)

        // Hide brush/grid overlay during refraction so it doesn't show through water
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const brushUniforms = (terrainMeshes[0]?.material as any)?.userData?.uniforms
        let savedBrushActive: number | undefined
        let savedGridVisible: number | undefined
        if (brushUniforms) {
          savedBrushActive = brushUniforms.brushActive.value
          savedGridVisible = brushUniforms.gridVisible.value
          brushUniforms.brushActive.value = 0.0
          brushUniforms.gridVisible.value = 0.0
        }

        // Hide entities so they only appear via the reflection pass
        const savedEntityVisible = entityClipGroup?.visible
        if (entityClipGroup) entityClipGroup.visible = false

        refractionManager.render()

        if (entityClipGroup) entityClipGroup.visible = savedEntityVisible ?? true

        if (brushUniforms) {
          brushUniforms.brushActive.value = savedBrushActive
          brushUniforms.gridVisible.value = savedGridVisible
        }
      } else if (refractionManager) {
        refractionManager.clear()
      }

      // Render reflection pass (entities only, mirrored camera)
      if (reflectionManager && $reflectionEnabled) {
        if (camera) reflectionManager.setCamera(camera)
        reflectionManager.setTerrainGroup(terrainGroup ?? null)
        if (waterGroup) reflectionManager.setWaterGroup(waterGroup)
        if (entityClipGroup) reflectionManager.setEntityClipGroup(entityClipGroup)

        // Hide nametags/HP bars during reflection render
        const nametagGroups: THREE.Group[] = []
        const ntCurrent = currentPlayerModel?.getNametagGroup()
        if (ntCurrent) { nametagGroups.push(ntCurrent); ntCurrent.visible = false }
        for (const pm of otherPlayerModels) {
          const nt = pm?.getNametagGroup()
          if (nt) { nametagGroups.push(nt); nt.visible = false }
        }
        for (const mm of monsterModels) {
          const nt = mm?.getNametagGroup()
          if (nt) { nametagGroups.push(nt); nt.visible = false }
        }

        reflectionManager.render()

        for (const nt of nametagGroups) nt.visible = true
      } else if (reflectionManager) {
        reflectionManager.clear()
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

    if ($mapEditorMode) {
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
    sceneLighting.update({
      currentPlayerPosition: currentPlayer?.position ?? null,
      localCalendarDate,
      ambientLight,
      directionalLight,
      scene,
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
      if (refractionManager) refractionManager.resize(nextSize.width, nextSize.height)
      if (reflectionManager) reflectionManager.resize(nextSize.width, nextSize.height)
    })

    const unsubscribeCameraReset = cameraResetNonce.subscribe((nonce) => {
      // Ignore initial store emission; only react to explicit reset requests.
      if (nonce > 0) {
        resetCameraToInitialState()
      }
    })

    const pmremGenerator = new PMREMGenerator(renderer)
    pmremGenerator.fromSceneAsync(new RoomEnvironment()).then((rt) => {
      scene.environment = rt.texture
      scene.environmentIntensity = 0.5
      pmremGenerator.dispose()
    })

    terrainGeometry = createTerrainGeometry(TERRAIN_TILE_SIZE, TERRAIN_TILE_SEGMENTS)
    {
      const loader = new THREE.TextureLoader()
      const tex = loader.load('/textures/waternormals.jpg')
      tex.wrapS = tex.wrapT = THREE.RepeatWrapping
      waterNormalMap = tex
    }
    loadFoamTexture().then((tex) => { waterFoamMap = tex })
    waterCausticsMap = generateCausticsTexture()

    // Initialize refraction render manager
    const refMgr = new RefractionRenderManager(renderer, scene, viewportSize.width, viewportSize.height)
    refractionManager = refMgr
    refractionTexture = refMgr.texture

    // Initialize reflection render manager (planar reflection for entities)
    const reflMgr = new ReflectionRenderManager(renderer, scene, viewportSize.width, viewportSize.height)
    reflectionManager = reflMgr
    reflectionTexture = reflMgr.texture


    rebuildTerrainTiles(terrainCenterChunk.x, terrainCenterChunk.z)

    // Pre-compile all WebGPU shaders once materials are ready
    loadSplatLayers().then(() => {
      // Allow Svelte to render the terrain meshes, then compile
      requestAnimationFrame(() => {
        if (camera) {
          renderer.compileAsync(scene, camera)
            .then(() => { isSceneCompiling = false })
            .catch(() => { isSceneCompiling = false })
        } else {
          isSceneCompiling = false
        }
      })
    })

    // Start game loop
    lastFrameTime = performance.now()
    initFpsCounting()
    gameLoopId = requestAnimationFrame(gameLoop)

    // Start chat bubble expiration checker
    startChatBubbleChecker()

    networkManager.connect(serverUrl)

    // Initialize camera position after a short delay to ensure camera ref is available
    if (_hmrCameraInitialized) {
      // HMR: restore previous camera state instead of resetting
      cameraInitialized = true
      if (_hmrCameraZoom !== null) {
        const restoreZoom = _hmrCameraZoom
        // Defer until camera ref is bound by Threlte
        requestAnimationFrame(() => {
          if (camera) {
            camera.zoom = restoreZoom
            camera.updateProjectionMatrix()
          }
        })
      }
      _hmrCameraZoom = null
    } else {
      setTimeout(() => {
        if (camera && currentPlayer) {
          resetCameraToInitialState()
          cameraInitialized = true
        }
      }, 1100)
    }

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
    enableRotate={$mapEditorMode ? false : $cameraRotationEnabled}
    enablePan={false}
    enableZoom={!$mapEditorMode}
    enabled={!$mapEditorMode}
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

<GameSceneWaterLayer
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
