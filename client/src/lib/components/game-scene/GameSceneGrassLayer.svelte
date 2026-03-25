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
    loadFlowerBillboardGeometry,
    loadFlowerColorTexture,
    createGrassMaterial,
    GUST_WAVE_COUNT,
    TALL_GRASS_CONFIG,
    FLOWER_CONFIG,
    GRASS_INSTANCE_POS_ATTR,
    GRASS_INSTANCE_ROT_ATTR,
    type GrassMaterialConfig,
    type GrassMaterialUniforms,
    type WindState,
  } from '../../shaders/grass-material'
  import { createBladeGeometry } from '../../shaders/grass-blade-geometry'
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
  const SUB_CHUNK_SIZE = 32
  const SUB_CHUNK_GRID_RADIUS = 1 // 1 = 3×3 grid (96m coverage)
  const GRID_COUNT = (SUB_CHUNK_GRID_RADIUS * 2 + 1) ** 2 // 9
  const MESH_CAPACITY = 131072
  const FLOWER_MESH_CAPACITY = 2048

  // ── Geometry & materials ──────────────────────────────
  // Blade geometry is created synchronously; only flower assets are async
  const _shortGrassGeometry: THREE.BufferGeometry = createBladeGeometry(5)
  const _tallGrassGeometry: THREE.BufferGeometry = createBladeGeometry(10)
  let _flowerGeometry: THREE.BufferGeometry | null = null
  let _flowerMaterial: THREE.Material | null = null
  // Flower uses the old uniform system
  let flowerUniforms: GrassMaterialUniforms | null = null
  let flowerBaseWindStrength = 0
  // Blade grass uses shared compute uniforms (one set per type)
  let shortComputeUniforms: GrassComputeUniforms | null = null
  let tallComputeUniforms: GrassComputeUniforms | null = null
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

  /** Eagerly create grass materials + one mesh per type so compileAsync can
   *  pre-compile the grass shader pipelines. Returns true when done. */
  export function ensureMaterialsForCompile(): boolean {
    if (!ensureMaterials() || !_flowerGeometry || !_flowerMaterial) return false
    // Create at least one slot per type for pipeline compilation.
    // Short/tall slots get compute contexts + blade materials lazily in rebuildType.
    // Flower just needs a slot mesh.
    ensureFlowerSlotMesh(0)
    return true
  }
  // Per-sub-chunk slot arrays. Short/tall now have paired compute contexts.
  interface BladeSlot {
    mesh: THREE.InstancedMesh
    ctx: GrassComputeContext
  }
  let shortSlots: (BladeSlot | null)[] = Array.from({ length: GRID_COUNT }, () => null)
  let tallSlots: (BladeSlot | null)[] = Array.from({ length: GRID_COUNT }, () => null)
  let flowerMeshes: THREE.InstancedMesh[] = []

  // Load flower assets asynchronously (blade geometry is already created above)
  let _flowerColorMap: THREE.Texture | null = null

  Promise.all([
    loadFlowerBillboardGeometry(),
    loadFlowerColorTexture(),
  ]).then(([flowerGeometry, flowerColorMap]) => {
    _flowerGeometry = flowerGeometry
    _flowerColorMap = flowerColorMap
    assetsReady = true
  })

  /** Create shared compute uniforms and flower material on first use. */
  function ensureMaterials(): boolean {
    if (shortComputeUniforms && tallComputeUniforms && _flowerMaterial) return true
    if (!_flowerGeometry || !_flowerColorMap) return false

    shortComputeUniforms = createSharedComputeUniforms()
    tallComputeUniforms = createSharedComputeUniforms(TALL_GRASS_CONFIG)
    const flowerResult = createGrassMaterial({ ...FLOWER_CONFIG, colorMap: _flowerColorMap })
    _flowerMaterial = flowerResult.material
    flowerUniforms = flowerResult.uniforms
    flowerBaseWindStrength = flowerResult.uniforms.uWindStrength.value
    return true
  }

  /** Create a blade slot: compute context + material + InstancedMesh. */
  function createBladeSlot(
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    cfg?: GrassMaterialConfig,
  ): BladeSlot {
    const ctx = createGrassComputeContext(MESH_CAPACITY, uniforms)
    const mat = createBladeMaterial(ctx, cfg)
    const geom = baseGeometry.clone()
    const mesh = new THREE.InstancedMesh(geom, mat, MESH_CAPACITY)
    // Do NOT set mesh.count = 0 here! WebGPU allocates GPU buffers based on
    // mesh.count at first render. If 0, the buffer can never grow later.
    mesh.castShadow = false
    mesh.receiveShadow = true
    mesh.frustumCulled = true
    return { mesh, ctx }
  }

  /** Ensure a blade slot exists at the given index. */
  function ensureBladeSlot(
    slots: (BladeSlot | null)[],
    index: number,
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    cfg?: GrassMaterialConfig,
  ): BladeSlot {
    if (!slots[index]) {
      slots[index] = createBladeSlot(baseGeometry, uniforms, cfg)
    }
    return slots[index]!
  }

  /** Ensure a flower mesh exists at the given index. */
  function ensureFlowerSlotMesh(index: number): THREE.InstancedMesh {
    if (!flowerMeshes[index]) {
      const geom = _flowerGeometry!.clone()
      geom.setAttribute(
        GRASS_INSTANCE_POS_ATTR,
        new THREE.InstancedBufferAttribute(new Float32Array(FLOWER_MESH_CAPACITY * 2), 2),
      )
      geom.setAttribute(
        GRASS_INSTANCE_ROT_ATTR,
        new THREE.InstancedBufferAttribute(new Float32Array(FLOWER_MESH_CAPACITY), 1),
      )
      const mesh = new THREE.InstancedMesh(geom, _flowerMaterial!, FLOWER_MESH_CAPACITY)
      mesh.castShadow = false
      mesh.receiveShadow = true
      mesh.frustumCulled = true
      flowerMeshes[index] = mesh
    }
    return flowerMeshes[index]
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
  let hasPlayer = $state(false)
  let curScx = 0
  let curScz = 0

  export function update(deltaTime: number, renderer?: WebGPURenderer) {
    if (!assetsReady) return
    const dt = Math.min(deltaTime / 1000, 0.1)
    elapsedTime += dt

    hasPlayer = !!playerPosition
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

    // Update shared compute uniforms (blade grass: short + tall)
    const computeUniformSets: GrassComputeUniforms[] = []
    if (shortComputeUniforms) computeUniformSets.push(shortComputeUniforms)
    if (tallComputeUniforms) computeUniformSets.push(tallComputeUniforms)

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

    // Update flower uniforms (old system)
    if (flowerUniforms) {
      const fu = flowerUniforms
      fu.uTime.value = elapsedTime
      fu.uWindStrength.value = flowerBaseWindStrength * windStrengthMul
      fu.uWindDir.value.set(cachedWindDirX, cachedWindDirZ)
      fu.uGustStrength.value = windStrengthMul
      for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
        fu.uWaveAngles[wi].value = waveAngles[wi]
        fu.uWaveAmps[wi].value = waveAmplitudes[wi]
        fu.uWaveParams[wi].value.copy(waveParams[wi])
      }
      // Use trail[0] as player current position for flower interaction
      if (playerPosition) {
        fu.uTrail[0].value.set(playerPosition.x, playerPosition.z, 1.0)
      } else {
        fu.uTrail[0].value.set(0, 0, 0)
      }
    }

    // ── Dispatch compute shaders for active blade slots ──
    if (renderer) {
      for (const slot of shortSlots) {
        if (slot && slot.ctx.count > 0) {
          renderer.compute(slot.ctx.computeUpdate)
        }
      }
      for (const slot of tallSlots) {
        if (slot && slot.ctx.count > 0) {
          renderer.compute(slot.ctx.computeUpdate)
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
    matrices: Float32Array
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

  const EMPTY_SUB_CHUNK: SubChunkData = { matrices: new Float32Array(0), worldXZ: new Float32Array(0), worldY: new Float32Array(0), rotations: new Float32Array(0), scales: new Float32Array(0), count: 0 }

  // Non-reactive internal caches — intentionally plain Map/Set for performance
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const subChunkCache = new Map<string, SubChunkBundle>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fetchedTiles = new Set<string>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()
  let needsRebuild = false

  // ── Partition raw instance data into sub-chunks ──────────
  function partitionIntoSubChunks(rawData: Float32Array, needMatrices = false): Map<string, SubChunkData> {
    const count = rawData.length / 5
    if (count === 0) return new Map()

    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const groups = new Map<string, number[]>()
    for (let i = 0; i < count; i++) {
      const x = rawData[i * 5]
      const z = rawData[i * 5 + 2]
      const key = `${Math.floor(x / SUB_CHUNK_SIZE)},${Math.floor(z / SUB_CHUNK_SIZE)}`
      let list = groups.get(key)
      if (!list) {
        list = []
        groups.set(key, list)
      }
      list.push(i)
    }

    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const result = new Map<string, SubChunkData>()
    for (const [key, indices] of groups) {
      const n = indices.length
      const matrices = needMatrices ? new Float32Array(n * 16) : new Float32Array(0)
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

        if (needMatrices) {
          const cos = Math.cos(rot) * scale
          const sin = Math.sin(rot) * scale
          const mi = j * 16
          matrices[mi] = cos
          matrices[mi + 1] = 0
          matrices[mi + 2] = -sin
          matrices[mi + 3] = 0
          matrices[mi + 4] = 0
          matrices[mi + 5] = scale
          matrices[mi + 6] = 0
          matrices[mi + 7] = 0
          matrices[mi + 8] = sin
          matrices[mi + 9] = 0
          matrices[mi + 10] = cos
          matrices[mi + 11] = 0
          matrices[mi + 12] = x
          matrices[mi + 13] = y
          matrices[mi + 14] = z
          matrices[mi + 15] = 1
        }

        worldXZ[j * 2] = x
        worldXZ[j * 2 + 1] = z
        worldY[j] = y
        rotations[j] = rot
        scales[j] = scale
      }

      result.set(key, { matrices, worldXZ, worldY, rotations, scales, count: n })
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

  function rebuildGrassBuffers() {
    needsRebuild = false
    if (!ensureMaterials()) return

    const wantedKeys = new Set(getActiveSubChunkKeys())

    rebuildBladeType(shortSlots, _shortGrassGeometry, shortComputeUniforms!, shortKeyToSlot, wantedKeys, (c) => c?.short)
    rebuildBladeType(tallSlots, _tallGrassGeometry, tallComputeUniforms!, tallKeyToSlot, wantedKeys, (c) => c?.tall, TALL_GRASS_CONFIG)
    rebuildFlowerType(wantedKeys)
  }

  /** Rebuild blade grass slots (short or tall) using compute contexts. */
  function rebuildBladeType(
    slots: (BladeSlot | null)[],
    baseGeometry: THREE.BufferGeometry,
    uniforms: GrassComputeUniforms,
    keyToSlot: Map<string, number>,
    wantedKeys: Set<string>,
    getData: (cached: SubChunkBundle | undefined) => SubChunkData | undefined,
    cfg?: GrassMaterialConfig,
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
      if (keyToSlot.has(key)) continue

      const data = getData(subChunkCache.get(key))
      if (!data || data.count === 0) continue
      if (freeSlots.length === 0) continue
      const slot = freeSlots.pop()!

      const bladeSlot = ensureBladeSlot(slots, slot, baseGeometry, uniforms, cfg)
      writeBladeSlotData(bladeSlot, data, key)
      keyToSlot.set(key, slot)
    }
  }

  /** Rebuild flower slots (still uses old InstancedMesh + attribute pattern). */
  function rebuildFlowerType(wantedKeys: Set<string>) {
    const freeSlots: number[] = []
    for (const [key, slot] of flowerKeyToSlot) {
      if (!wantedKeys.has(key)) {
        if (flowerMeshes[slot]) {
          flowerMeshes[slot].count = 0
          if (flowerMeshes[slot].parent) flowerMeshes[slot].parent.remove(flowerMeshes[slot])
        }
        flowerKeyToSlot.delete(key)
        freeSlots.push(slot)
      }
    }

    const usedSlots = new Set(flowerKeyToSlot.values())
    for (let i = 0; i < GRID_COUNT; i++) {
      if (!usedSlots.has(i)) freeSlots.push(i)
    }

    for (const key of wantedKeys) {
      if (flowerKeyToSlot.has(key)) continue

      const data = subChunkCache.get(key)?.flower
      if (!data || data.count === 0) continue
      if (freeSlots.length === 0) continue
      const slot = freeSlots.pop()!

      const mesh = ensureFlowerSlotMesh(slot)
      writeFlowerMeshData(mesh, data, key)
      flowerKeyToSlot.set(key, slot)
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
    const count = Math.min(data.count, MESH_CAPACITY)

    // Write placement data into compute bladeData + bladeScale buffers
    writeBladeData(slot.ctx, data.worldXZ, data.worldY, data.rotations, data.scales, count)

    // instanceMatrix: not used for position (all in bladeData), but InstancedMesh
    // requires it. Leave as default identity matrices.
    slot.mesh.count = count

    setBoundingSphere(slot.mesh, subChunkKey)

    if (slot.mesh.parent) slot.mesh.parent.remove(slot.mesh)
    grassGroup.add(slot.mesh)
  }

  /** Write flower data (old attribute-based pattern). */
  function writeFlowerMeshData(mesh: THREE.InstancedMesh, data: SubChunkData, subChunkKey: string) {
    if (data.count === 0) {
      mesh.count = 0
      if (mesh.parent) mesh.parent.remove(mesh)
      return
    }

    const count = Math.min(data.count, FLOWER_MESH_CAPACITY)

    const matArr = mesh.instanceMatrix.array as Float32Array
    matArr.set(data.matrices.subarray(0, count * 16))
    mesh.instanceMatrix.needsUpdate = true

    const xzAttr = mesh.geometry.getAttribute(GRASS_INSTANCE_POS_ATTR) as THREE.InstancedBufferAttribute
    ;(xzAttr.array as Float32Array).set(data.worldXZ.subarray(0, count * 2))
    xzAttr.needsUpdate = true

    const rotAttr = mesh.geometry.getAttribute(GRASS_INSTANCE_ROT_ATTR) as THREE.InstancedBufferAttribute
    ;(rotAttr.array as Float32Array).set(data.rotations.subarray(0, count))
    rotAttr.needsUpdate = true

    mesh.count = count
    setBoundingSphere(mesh, subChunkKey)

    if (mesh.parent) mesh.parent.remove(mesh)
    grassGroup.add(mesh)
  }

  // ── Tile data lifecycle ─────────────────────────────────
  $effect(() => {
    if (!hasPlayer || !assetsReady) return

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
            const flowerChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'flower'), true)

            const allKeys = new Set([...shortChunks.keys(), ...tallChunks.keys(), ...flowerChunks.keys()])
            for (const key of allKeys) {
              subChunkCache.set(key, {
                short: shortChunks.get(key) ?? EMPTY_SUB_CHUNK,
                tall: tallChunks.get(key) ?? EMPTY_SUB_CHUNK,
                flower: flowerChunks.get(key) ?? EMPTY_SUB_CHUNK,
              })
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

  function tileSubChunkRange(tileX: number, tileZ: number) {
    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMaxX = tileX * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMaxZ = tileZ * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
    return {
      scMinX: Math.floor(tileMinX / SUB_CHUNK_SIZE),
      scMaxX: Math.floor((tileMaxX - 1) / SUB_CHUNK_SIZE),
      scMinZ: Math.floor(tileMinZ / SUB_CHUNK_SIZE),
      scMaxZ: Math.floor((tileMaxZ - 1) / SUB_CHUNK_SIZE),
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
      const flowerChunks = partitionIntoSubChunks(getThinnedInstanceData(grassData, 'flower'), true)

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
