<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { WebGPURenderer } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'
  import { windDebugVisible } from '../../stores/debugStore'
  import { getThinnedInstanceData } from '../../utils/grass-data'
  import {
    SUB_CHUNK_SIZE,
    tileSubChunkRange,
    isKeyInTileRange,
    partitionKeysFromRawData,
  } from '../../utils/grass-sub-chunks'
  import {
    loadFlowerColorTexture,
    TALL_GRASS_CONFIG,
    FLOWER_CONFIG,
    type GrassMaterialConfig,
    type WindState,
  } from '../../shaders/grass-material'
  import { GUST_WAVE_COUNT } from '../../shaders/grass-shared'
  import { createBladeGeometry, createStarGeometry } from '../../shaders/grass-blade-geometry'
  import {
    createBladeMaterial,
    createSharedComputeUniforms,
    createGrassComputeContext,
    writeBladeData,
    type GrassComputeContext,
    type GrassComputeUniforms,
  } from '../../shaders/grass-compute'

  interface Props {
    terrainTiles: TerrainTile[]
    grassDataManager: TerrainGrassDataManager | null
    playerPosition?: THREE.Vector3 | null
  }

  let {
    terrainTiles,
    grassDataManager = null,
    playerPosition = null,
  }: Props = $props()

  // ── Sub-chunk grass rendering ──────────────────────────
  const SUB_CHUNK_GRID_RADIUS = 1 // 1 = 3×3 grid (96m coverage)
  const GRID_COUNT = (SUB_CHUNK_GRID_RADIUS * 2 + 1) ** 2 // 9
  const MESH_CAPACITY = 131072
  const FLOWER_MESH_CAPACITY = 2048

  // ── Geometry & materials ──────────────────────────────
  // All geometry is created synchronously; only flower texture is async
  const _shortGrassGeometry: THREE.BufferGeometry = createBladeGeometry(5)
  const _tallGrassGeometry: THREE.BufferGeometry = createBladeGeometry(10)
  const _flowerGeometry: THREE.BufferGeometry = createStarGeometry(3, 0.35, 0.7, 1.0)
  let _flowerCfg: GrassMaterialConfig | null = null
  // Shared compute uniforms (one set per grass type)
  let shortComputeUniforms: GrassComputeUniforms | null = null
  let tallComputeUniforms: GrassComputeUniforms | null = null
  let flowerComputeUniforms: GrassComputeUniforms | null = null
  let assetsReady = $state(false)

  // ── Mesh management via THREE.Group (no Svelte proxy) ──
  const grassGroup = new THREE.Group()

  /** Expose grassGroup for visibility toggling during render passes */
  export function getGroup(): THREE.Group {
    return grassGroup
  }

  /** Grass instance count in the player's current sub-chunk. */
  export function getPlayerChunkGrassCount(): number {
    const chunk = subChunkCache.get(`${curScx},${curScz}`)
    if (!chunk) return 0
    return chunk.short.count + chunk.tall.count
  }

  /** Expose current wind state for particle systems */
  export function getWindState(): WindState {
    return {
      windDirX: cachedWindDirX,
      windDirZ: cachedWindDirZ,
      windStrength: windStrengthMul,
      time: elapsedTime,
    }
  }

  /** Warm up WebGPU compute + render pipelines for every grass slot by
   *  preallocating all GRID_COUNT slots per grass type with a single dummy
   *  blade far below the world. Without this, each new slot that activates
   *  mid-movement stalls the main thread for 100–800ms compiling pipelines
   *  (each slot has its own TSL-generated compute + render pipeline). */
  export function warmupGrassPipelines(r: WebGPURenderer): boolean {
    if (!ensureMaterials()) return false
    const dummyXZ = new Float32Array([0, 0])
    const dummyY = new Float32Array([-100000])
    const dummyRot = new Float32Array([0])
    const dummyScale = new Float32Array([1])
    const warmupSlots: BladeSlot[] = []
    const warmupAllSlots = (slots: (BladeSlot | null)[], geom: THREE.BufferGeometry,
      uniforms: GrassComputeUniforms, cfg?: GrassMaterialConfig, capacity = MESH_CAPACITY) => {
      for (let i = 0; i < GRID_COUNT; i++) {
        const slot = ensureBladeSlot(slots, i, geom, uniforms, cfg, capacity)
        writeBladeData(slot.ctx, dummyXZ, dummyY, dummyRot, dummyScale, 1)
        ;(slot.ctx.computeUpdate as { count: number }).count = 1
        r.compute(slot.ctx.computeUpdate)
        slot.mesh.count = 1
        slot.ctx.count = 1
        // Frustum culling disabled during warmup so the dummy blade is
        // guaranteed to be included in the compile traversal (a below-world
        // bounding sphere would get culled). Reset below after Threlte has
        // rendered the mesh once.
        slot.mesh.frustumCulled = false
        if (!slot.mesh.parent) grassGroup.add(slot.mesh)
        warmupSlots.push(slot)
      }
    }
    warmupAllSlots(shortSlots, _shortGrassGeometry, shortComputeUniforms!)
    warmupAllSlots(tallSlots, _tallGrassGeometry, tallComputeUniforms!, TALL_GRASS_CONFIG)
    warmupAllSlots(flowerSlots, _flowerGeometry, flowerComputeUniforms!, _flowerCfg!, FLOWER_MESH_CAPACITY)

    // After a few frames, cull/silence the dummy slots. They stay allocated so
    // writeBladeSlotData() can reassign them to real sub-chunks instantly, but
    // they no longer cost a compute dispatch or a draw call every frame.
    let framesLeft = 3
    const tick = () => {
      if (--framesLeft > 0) {
        requestAnimationFrame(tick)
        return
      }
      for (const slot of warmupSlots) {
        slot.mesh.count = 0
        slot.ctx.count = 0
        slot.mesh.frustumCulled = true
        slot.mesh.boundingSphere = new THREE.Sphere(
          new THREE.Vector3(0, -100000, 0), 1)
      }
      // Warmup overwrites any real data that was already loaded into slots.
      // Clear slot assignments so the next rebuild re-writes real data.
      shortKeyToSlot.clear()
      tallKeyToSlot.clear()
      flowerKeyToSlot.clear()
      needsRebuild = true
    }
    requestAnimationFrame(tick)
    return true
  }
  // Per-sub-chunk slot arrays. Short/tall now have paired compute contexts.
  interface BladeSlot {
    mesh: THREE.InstancedMesh
    ctx: GrassComputeContext
    capacity: number
  }
  let shortSlots: (BladeSlot | null)[] = Array.from({ length: GRID_COUNT }, () => null)
  let tallSlots: (BladeSlot | null)[] = Array.from({ length: GRID_COUNT }, () => null)
  let flowerSlots: (BladeSlot | null)[] = Array.from({ length: GRID_COUNT }, () => null)

  // Load flower texture asynchronously (geometry is now procedural)
  let _flowerColorMap: THREE.Texture | null = null

  loadFlowerColorTexture().then((tex) => {
    _flowerColorMap = tex
    assetsReady = true
  })

  /** Create shared compute uniforms for all grass types on first use. */
  function ensureMaterials(): boolean {
    if (shortComputeUniforms && tallComputeUniforms && flowerComputeUniforms) return true
    if (!_flowerColorMap) return false

    shortComputeUniforms = createSharedComputeUniforms()
    tallComputeUniforms = createSharedComputeUniforms(TALL_GRASS_CONFIG)
    flowerComputeUniforms = createSharedComputeUniforms(FLOWER_CONFIG)
    _flowerCfg = { ...FLOWER_CONFIG, colorMap: _flowerColorMap! }
    return true
  }

  /** Create a blade slot: compute context + material + InstancedMesh. */
  function createBladeSlot(
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    cfg?: GrassMaterialConfig,
    capacity = MESH_CAPACITY,
  ): BladeSlot {
    const ctx = createGrassComputeContext(capacity, uniforms)
    const mat = createBladeMaterial(ctx, cfg)
    const geom = baseGeometry.clone()
    const mesh = new THREE.InstancedMesh(geom, mat, capacity)
    // Do NOT set mesh.count = 0 here! WebGPU allocates GPU buffers based on
    // mesh.count at first render. If 0, the buffer can never grow later.
    mesh.castShadow = false
    mesh.receiveShadow = true
    mesh.frustumCulled = true
    return { mesh, ctx, capacity }
  }

  /** Ensure a blade slot exists at the given index. */
  function ensureBladeSlot(
    slots: (BladeSlot | null)[],
    index: number,
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    cfg?: GrassMaterialConfig,
    capacity = MESH_CAPACITY,
  ): BladeSlot {
    if (!slots[index]) {
      slots[index] = createBladeSlot(baseGeometry, uniforms, cfg, capacity)
    }
    return slots[index]!
  }

  // ── Wind debug arrow ──────────────────────────────────────
  const WIND_ARROW_COLOR = 0x00ff88
  const windArrowDir = new THREE.Vector3(1, 0, 0)
  const windArrow = new THREE.ArrowHelper(
    windArrowDir,
    new THREE.Vector3(),
    3,
    WIND_ARROW_COLOR,
    0.6,
    0.3,
  )
  windArrow.visible = false

  let elapsedTime = 0

  // ── Wind parameters ──────────────────────────────────────
  const WIND_STR_MIN = 0.3
  const WIND_STR_MAX = 1.0

  function smoothstep(t: number): number {
    return t * t * (3 - 2 * t)
  }

  const WAVE_HOLD_MIN = 5
  const WAVE_HOLD_MAX = 15
  const WAVE_FADE_DURATION = 2.0
  const waveAngles = [0, 0.4, -0.3]
  const waveAmplitudes = [1, 1, 1]
  // vec4(freq, speed, amp, Q) per wave
  const waveParams = [
    new THREE.Vector4(0.35, 0.7, 1.5, 0.75),
    new THREE.Vector4(0.31, 0.8, 1.6, 0.87),
    new THREE.Vector4(0.39, 1.5, 1.7, 0.95),
  ]
  const wavePhases: ('hold' | 'fade-out' | 'fade-in')[] = ['hold', 'hold', 'hold']
  const waveTimers = Array.from({ length: GUST_WAVE_COUNT }, () => 0)
  const waveDurations = Array.from({ length: GUST_WAVE_COUNT }, () => 0)

  function startWaveHold(i: number) {
    wavePhases[i] = 'hold'
    waveAmplitudes[i] = 1
    waveDurations[i] = WAVE_HOLD_MIN + Math.random() * (WAVE_HOLD_MAX - WAVE_HOLD_MIN)
    waveTimers[i] = waveDurations[i]
  }

  function startWaveFadeOut(i: number) {
    wavePhases[i] = 'fade-out'
    waveDurations[i] = WAVE_FADE_DURATION
    waveTimers[i] = WAVE_FADE_DURATION
  }

  function randomWaveParams(): THREE.Vector4 {
    const freq = 0.2 + Math.random() * 0.3 // 0.2 ~ 0.5
    // speed scales with wind strength: weak → 0.3~0.8, strong → 0.6~1.6
    const speedBase = 0.3 + windStrengthMul * 0.3
    const speed = speedBase + Math.random() * (speedBase * 1.5)
    // amp scales with wind strength: weak wind → 0.6~1.2, strong wind → 1.2~2.4
    const ampBase = 0.6 + windStrengthMul * 0.6
    const amp = ampBase + Math.random() * (ampBase * 0.8)
    const Q = 0.6 + Math.random() * 0.4 // 0.6 ~ 1.0
    return new THREE.Vector4(freq, speed, amp, Q)
  }

  const MAX_WAVE_OFFSET = Math.PI / 4 // ±45° from windAngle

  function startWaveFadeIn(i: number) {
    // Random direction within ±45° of main wind
    waveAngles[i] = (Math.random() * 2 - 1) * MAX_WAVE_OFFSET
    waveParams[i] = randomWaveParams()
    wavePhases[i] = 'fade-in'
    waveAmplitudes[i] = 0
    waveDurations[i] = WAVE_FADE_DURATION
    waveTimers[i] = WAVE_FADE_DURATION
  }

  function pickStrengthTransition() {
    windStrengthTarget = WIND_STR_MIN + Math.random() * (WIND_STR_MAX - WIND_STR_MIN)
    windStrengthDuration = 4 + Math.random() * 8
    windStrengthTimer = windStrengthDuration
    windStrengthStart = windStrengthMul
  }

  let windAngle = Math.random() * Math.PI * 2
  let cachedWindDirX = Math.cos(windAngle)
  let cachedWindDirZ = Math.sin(windAngle)

  // Wind direction change state machine:
  // steady → fading-out (waves + strength → 0) → snap angle → fading-in (strength back) → steady
  const WIND_DIR_FADE_OUT_DURATION = 3.0
  const WIND_DIR_FADE_IN_DURATION = 1.5
  let windDirPhase: 'steady' | 'fading-out' | 'fading-in' = 'steady'
  let pendingWindAngle = windAngle
  let windDirFadeTimer = 0
  let windStrengthBeforeFade = 0.5
  let windDirTimer = 15 + Math.random() * 25 // first change in 15~40s

  function triggerWindDirectionChange() {
    const shift = (Math.PI / 6) + Math.random() * (Math.PI / 3) // ±30°~90°
    pendingWindAngle = windAngle + (Math.random() < 0.5 ? shift : -shift)
    windDirPhase = 'fading-out'
    windDirFadeTimer = WIND_DIR_FADE_OUT_DURATION
    windStrengthBeforeFade = windStrengthMul

    // Force all waves to start fading out
    for (let i = 0; i < GUST_WAVE_COUNT; i++) {
      if (wavePhases[i] !== 'fade-out') {
        startWaveFadeOut(i)
      }
    }
  }

  // Strength interpolation
  let windStrengthMul = 0.5
  let windStrengthStart = windStrengthMul
  let windStrengthTarget = windStrengthMul
  let windStrengthTimer = 0
  let windStrengthDuration = 0

  // ── Player sub-chunk tracking ─────────────────────────
  let curScx = 0
  let curScz = 0
  let computeFrameCount = 0

  export function update(deltaTime: number, renderer?: WebGPURenderer) {
    if (!assetsReady) return
    const dt = Math.min(deltaTime / 1000, 0.1)
    elapsedTime += dt

    if (playerPosition) {
      const scx = Math.floor(playerPosition.x / SUB_CHUNK_SIZE)
      const scz = Math.floor(playerPosition.z / SUB_CHUNK_SIZE)
      if (scx !== curScx || scz !== curScz) {
        curScx = scx
        curScz = scz
        needsRebuild = true
      }
    }

    if (needsRebuild) rebuildGrassBuffers()

    // ── Per-wave direction (hold → fade-out → snap → fade-in → hold) ──
    for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
      waveTimers[wi] -= dt
      switch (wavePhases[wi]) {
        case 'hold': {
          if (waveTimers[wi] <= 0) startWaveFadeOut(wi)
          break
        }
        case 'fade-out': {
          waveAmplitudes[wi] = smoothstep(waveTimers[wi] / waveDurations[wi])
          if (waveTimers[wi] <= 0) {
            if (windDirPhase !== 'steady') {
              // Park wave at 0 until direction change completes
              waveAmplitudes[wi] = 0
            } else {
              startWaveFadeIn(wi)
            }
          }
          break
        }
        case 'fade-in': {
          waveAmplitudes[wi] = smoothstep(1 - waveTimers[wi] / waveDurations[wi])
          if (waveTimers[wi] <= 0) startWaveHold(wi)
          break
        }
      }
    }

    // ── Wind direction change (state machine) ──
    if (windDirPhase === 'steady') {
      windDirTimer -= dt
      if (windDirTimer <= 0) {
        triggerWindDirectionChange()
      }
    } else if (windDirPhase === 'fading-out') {
      windDirFadeTimer -= dt
      // Fade wind strength toward 0
      const t = smoothstep(Math.min(1, 1 - windDirFadeTimer / WIND_DIR_FADE_OUT_DURATION))
      windStrengthMul = windStrengthBeforeFade * (1 - t)
      // Wait for both strength ~0 and all waves ~0
      const allFaded = waveAmplitudes.every((a) => a < 0.01)
      if (windDirFadeTimer <= 0 && allFaded) {
        // Snap direction (strength is 0 so no visual pop)
        windAngle = pendingWindAngle
        windStrengthMul = 0
        windDirPhase = 'fading-in'
        windDirFadeTimer = WIND_DIR_FADE_IN_DURATION
        for (let i = 0; i < GUST_WAVE_COUNT; i++) {
          startWaveFadeIn(i)
        }
      }
    } else if (windDirPhase === 'fading-in') {
      windDirFadeTimer -= dt
      const t = smoothstep(Math.min(1, 1 - windDirFadeTimer / WIND_DIR_FADE_IN_DURATION))
      windStrengthMul = windStrengthBeforeFade * t
      if (windDirFadeTimer <= 0) {
        windStrengthMul = windStrengthBeforeFade
        // Resume independent strength transitions from current value
        windStrengthStart = windStrengthMul
        windStrengthTarget = windStrengthMul
        windStrengthTimer = 0
        windDirPhase = 'steady'
        windDirTimer = 20 + Math.random() * 30 // next change in 20~50s
      }
    }

    // ── Wind strength (independent timer, paused during direction change) ──
    if (windDirPhase === 'steady') {
      windStrengthTimer -= dt
      if (windStrengthDuration > 0) {
        const t = smoothstep(Math.min(1, 1 - windStrengthTimer / windStrengthDuration))
        windStrengthMul = windStrengthStart + (windStrengthTarget - windStrengthStart) * t
      }
      if (windStrengthTimer <= 0) {
        windStrengthMul = windStrengthTarget
        pickStrengthTransition()
      }
    }

    cachedWindDirX = Math.cos(windAngle)
    cachedWindDirZ = Math.sin(windAngle)

    // Update shared compute uniforms (all grass types)
    const computeUniformSets: GrassComputeUniforms[] = []
    if (shortComputeUniforms) computeUniformSets.push(shortComputeUniforms)
    if (tallComputeUniforms) computeUniformSets.push(tallComputeUniforms)
    if (flowerComputeUniforms) computeUniformSets.push(flowerComputeUniforms)

    for (const u of computeUniformSets) {
      u.uTime.value = elapsedTime
      u.uDeltaTime.value = dt
      u.uWindDir.value.set(cachedWindDirX, cachedWindDirZ)
      u.uGustStrength.value = windStrengthMul
      for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
        u.uWaveAngles[wi].value = waveAngles[wi]
        u.uWaveAmps[wi].value = waveAmplitudes[wi]
        u.uWaveParams[wi].value.copy(waveParams[wi])
      }
      // Player position: asymmetric lerp creates natural trail effect
      if (playerPosition) {
        u.uPlayerPos.value.set(playerPosition.x, playerPosition.z, 1.0)
      } else {
        u.uPlayerPos.value.set(99999, 99999, 0)
      }
    }

    // Apply windStrengthMul to base wind strengths
    if (shortComputeUniforms) {
      shortComputeUniforms.uWindStrength.value = (0.06) * windStrengthMul
    }
    if (tallComputeUniforms) {
      tallComputeUniforms.uWindStrength.value = (TALL_GRASS_CONFIG.windStrength ?? 0.12) * windStrengthMul
    }
    if (flowerComputeUniforms) {
      flowerComputeUniforms.uWindStrength.value = (FLOWER_CONFIG.windStrength ?? 0.04) * windStrengthMul
    }

    // ── Dispatch compute shaders for active blade slots (every other frame) ──
    computeFrameCount++
    if (renderer && computeFrameCount % 2 === 0) {
      for (const slots of [shortSlots, tallSlots, flowerSlots]) {
        for (const slot of slots) {
          if (slot && slot.ctx.count > 0) {
            // Dispatch only for actual blade count, not full capacity (131K)
            ;(slot.ctx.computeUpdate as { count: number }).count = slot.ctx.count
            renderer.compute(slot.ctx.computeUpdate)
          }
        }
      }
    }

    const showArrow = $windDebugVisible
    windArrow.visible = showArrow
    if (showArrow && playerPosition) {
      const arrowLen = 1.5 + windStrengthMul * 3.5
      windArrowDir.set(cachedWindDirX, 0, cachedWindDirZ)
      windArrow.position.set(playerPosition.x, playerPosition.y + 3, playerPosition.z)
      windArrow.setDirection(windArrowDir)
      windArrow.setLength(arrowLen, arrowLen * 0.2, arrowLen * 0.1)
      windArrow.setColor(WIND_ARROW_COLOR)
    }
  }

  // ── Sub-chunk data cache ──────────────────────────────
  interface SubChunkData {
    worldXZ: Float32Array
    worldY: Float32Array
    rotations: Float32Array
    scales: Float32Array
    count: number
  }

  interface SubChunkBundle {
    short: SubChunkData
    tall: SubChunkData
    flower: SubChunkData
  }

  const EMPTY_SUB_CHUNK: SubChunkData = { worldXZ: new Float32Array(0), worldY: new Float32Array(0), rotations: new Float32Array(0), scales: new Float32Array(0), count: 0 }

  // Non-reactive internal caches — intentionally plain Map/Set for performance
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const subChunkCache = new Map<string, SubChunkBundle>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fetchedTiles = new Set<string>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()
  let needsRebuild = false

  // ── Partition raw instance data into sub-chunks ──────────
  function partitionIntoSubChunks(rawData: Float32Array): Map<string, SubChunkData> {
    const count = rawData.length / 5
    if (count === 0) return new Map()

    const groups = partitionKeysFromRawData(rawData)

    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const result = new Map<string, SubChunkData>()
    for (const [key, indices] of groups) {
      const n = indices.length
      const worldXZ = new Float32Array(n * 2)
      const worldY = new Float32Array(n)
      const rotations = new Float32Array(n)
      const scales = new Float32Array(n)

      for (let j = 0; j < n; j++) {
        const base = indices[j] * 5
        const x = rawData[base]
        const y = rawData[base + 1]
        const z = rawData[base + 2]
        const rot = rawData[base + 3]
        const scale = rawData[base + 4]

        worldXZ[j * 2] = x
        worldXZ[j * 2 + 1] = z
        worldY[j] = y
        rotations[j] = rot
        scales[j] = scale
      }

      result.set(key, { worldXZ, worldY, rotations, scales, count: n })
    }
    return result
  }

  // ── Collect active sub-chunk keys (3×3 grid around player) ──
  function getActiveSubChunkKeys(): string[] {
    const keys: string[] = []
    for (let dz = -SUB_CHUNK_GRID_RADIUS; dz <= SUB_CHUNK_GRID_RADIUS; dz++) {
      for (let dx = -SUB_CHUNK_GRID_RADIUS; dx <= SUB_CHUNK_GRID_RADIUS; dx++) {
        keys.push(`${curScx + dx},${curScz + dz}`)
      }
    }
    return keys
  }

  // Key-based slot assignment: track which sub-chunk key each slot displays.
  const shortKeyToSlot = new SvelteMap<string, number>()
  const tallKeyToSlot = new SvelteMap<string, number>()
  const flowerKeyToSlot = new SvelteMap<string, number>()
  // Track sub-chunk keys whose cache entry was updated since last slot write.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const dirtySubChunks = new Set<string>()

  function rebuildGrassBuffers() {
    needsRebuild = false
    if (!ensureMaterials()) return

    const wantedKeys = new Set(getActiveSubChunkKeys())

    rebuildBladeType(shortSlots, _shortGrassGeometry, shortComputeUniforms!, shortKeyToSlot, wantedKeys, (c) => c?.short)
    rebuildBladeType(tallSlots, _tallGrassGeometry, tallComputeUniforms!, tallKeyToSlot, wantedKeys, (c) => c?.tall, TALL_GRASS_CONFIG)
    rebuildBladeType(flowerSlots, _flowerGeometry!, flowerComputeUniforms!, flowerKeyToSlot, wantedKeys, (c) => c?.flower,
      _flowerCfg!, FLOWER_MESH_CAPACITY)
    dirtySubChunks.clear()
  }

  /** Rebuild blade grass slots (short, tall, or flower) using compute contexts. */
  function rebuildBladeType(
    slots: (BladeSlot | null)[],
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    keyToSlot: Map<string, number>,
    wantedKeys: Set<string>,
    getData: (cached: SubChunkBundle | undefined) => SubChunkData | undefined,
    cfg?: GrassMaterialConfig,
    capacity = MESH_CAPACITY,
  ) {
    const freeSlots: number[] = []
    for (const [key, slot] of keyToSlot) {
      if (!wantedKeys.has(key)) {
        const s = slots[slot]
        if (s) {
          s.mesh.count = 0
          s.ctx.count = 0
          if (s.mesh.parent) s.mesh.parent.remove(s.mesh)
        }
        keyToSlot.delete(key)
        freeSlots.push(slot)
      }
    }

    const usedSlots = new Set(keyToSlot.values())
    for (let i = 0; i < GRID_COUNT; i++) {
      if (!usedSlots.has(i)) freeSlots.push(i)
    }

    for (const key of wantedKeys) {
      if (keyToSlot.has(key)) {
        // Re-upload data if cache was updated since last write
        if (dirtySubChunks.has(key)) {
          const data = getData(subChunkCache.get(key))
          if (data && data.count > 0) {
            writeBladeSlotData(slots[keyToSlot.get(key)!]!, data, key)
          }
        }
        continue
      }

      const data = getData(subChunkCache.get(key))
      if (!data || data.count === 0) continue
      if (freeSlots.length === 0) continue
      const slot = freeSlots.pop()!

      const bladeSlot = ensureBladeSlot(slots, slot, baseGeometry, uniforms, cfg, capacity)
      writeBladeSlotData(bladeSlot, data, key)
      keyToSlot.set(key, slot)
    }
  }

  // Half-diagonal of a 32×32 sub-chunk + vertical margin for grass height
  const SUB_CHUNK_HALF_DIAG = Math.sqrt(SUB_CHUNK_SIZE * SUB_CHUNK_SIZE * 0.5 + 10 * 10)

  function setBoundingSphere(mesh: THREE.InstancedMesh, subChunkKey: string) {
    const [scx, scz] = subChunkKey.split(',').map(Number)
    mesh.boundingSphere = new THREE.Sphere(
      new THREE.Vector3((scx + 0.5) * SUB_CHUNK_SIZE, 0, (scz + 0.5) * SUB_CHUNK_SIZE),
      SUB_CHUNK_HALF_DIAG,
    )
  }

  /** Write blade data to a compute slot's buffers + set up the mesh. */
  function writeBladeSlotData(slot: BladeSlot, data: SubChunkData, subChunkKey: string) {
    const count = Math.min(data.count, slot.capacity)

    // Write placement data into compute bladeData + bladeScale buffers
    writeBladeData(slot.ctx, data.worldXZ, data.worldY, data.rotations, data.scales, count)

    // instanceMatrix: not used for position (all in bladeData), but InstancedMesh
    // requires it. Leave as default identity matrices.
    slot.mesh.count = count

    setBoundingSphere(slot.mesh, subChunkKey)

    if (slot.mesh.parent) slot.mesh.parent.remove(slot.mesh)
    grassGroup.add(slot.mesh)
  }


  // ── Tile data lifecycle ─────────────────────────────────
  $effect(() => {
    const gMgr = grassDataManager
    if (!gMgr) return

    for (const tile of terrainTiles) {
      const tk = tile.id
      if (fetchedTiles.has(tk) || pendingTiles.has(tk)) continue

      const tileX = Math.round(tile.position[0] / TERRAIN_TILE_SIZE)
      const tileZ = Math.round(tile.position[2] / TERRAIN_TILE_SIZE)

      pendingTiles.add(tk)

      gMgr
        .loadGrassData(tileX, tileZ)
        .then((grassData) => {
          if (!pendingTiles.has(tk)) return
          pendingTiles.delete(tk)

          if (grassData) {
            const shortChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'short'))
            const tallChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'tall'))
            const flowerChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'flower'))

            // Only cache sub-chunks within this tile's spatial range.
            // Grass placement jitter can push instances across tile boundaries;
            // those spillover instances must be ignored so a later-loading tile
            // doesn't overwrite a neighbor's dense data with a handful of strays.
            const tileRange = tileSubChunkRange(tileX, tileZ)
            const allKeys = new Set([...shortChunks.keys(), ...tallChunks.keys(), ...flowerChunks.keys()])
            for (const key of allKeys) {
              if (!isKeyInTileRange(key, tileRange)) continue
              subChunkCache.set(key, {
                short: shortChunks.get(key) ?? EMPTY_SUB_CHUNK,
                tall: tallChunks.get(key) ?? EMPTY_SUB_CHUNK,
                flower: flowerChunks.get(key) ?? EMPTY_SUB_CHUNK,
              })
              dirtySubChunks.add(key)
            }
          }

          fetchedTiles.add(tk)
          needsRebuild = true
        })
        .catch(() => {
          pendingTiles.delete(tk)
        })
    }

    // Clean up tiles no longer in the scene
    const tileIds = new Set(terrainTiles.map((t) => t.id))
    for (const tk of fetchedTiles) {
      if (!tileIds.has(tk)) {
        fetchedTiles.delete(tk)
        const parts = tk.split('_')
        const tileX = parseInt(parts[0])
        const tileZ = parseInt(parts[1])
        clearSubChunksForTile(tileX, tileZ)
        needsRebuild = true
      }
    }
  })

  /** Compute sub-chunk index range for a tile and clear related caches. */
  function clearSubChunksForTile(tileX: number, tileZ: number) {
    const { scMinX, scMaxX, scMinZ, scMaxZ } = tileSubChunkRange(tileX, tileZ)
    for (let sz = scMinZ; sz <= scMaxZ; sz++) {
      for (let sx = scMinX; sx <= scMaxX; sx++) {
        const key = `${sx},${sz}`
        subChunkCache.delete(key)
        shortKeyToSlot.delete(key)
        tallKeyToSlot.delete(key)
        flowerKeyToSlot.delete(key)
      }
    }
  }

  // ── Listen for grass data updates (e.g. housing placement) ──
  $effect(() => {
    const gMgr = grassDataManager
    if (!gMgr) return

    return gMgr.onTileUpdated((tileX, tileZ) => {
      const grassData = gMgr.getCachedGrassData(tileX, tileZ)
      if (!grassData) return

      clearSubChunksForTile(tileX, tileZ)

      // Re-partition updated data
      const shortChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'short'))
      const tallChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'tall'))
      const flowerChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'flower'))

      const allKeys = new Set([...shortChunks.keys(), ...tallChunks.keys(), ...flowerChunks.keys()])
      for (const key of allKeys) {
        subChunkCache.set(key, {
          short: shortChunks.get(key) ?? EMPTY_SUB_CHUNK,
          tall: tallChunks.get(key) ?? EMPTY_SUB_CHUNK,
          flower: flowerChunks.get(key) ?? EMPTY_SUB_CHUNK,
        })
      }

      needsRebuild = true
    })
  })
</script>

<T is={grassGroup} />
<T is={windArrow} />
