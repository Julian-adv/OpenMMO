<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'
  import { windDebugVisible } from '../../stores/debugStore'
  import { getInstanceData } from '../../utils/grass-data'
  import {
    loadGrassBillboardGeometry,
    loadFlowerBillboardGeometry,
    loadGrassAlphaTexture,
    loadFlowerColorTexture,
    createGrassMaterial,
    GRASS_INSTANCE_POS_ATTR,
    GRASS_INSTANCE_ROT_ATTR,
    GRASS_TRAIL_COUNT,
    GUST_WAVE_COUNT,
    TALL_GRASS_CONFIG,
    FLOWER_CONFIG,
    type GrassMaterialUniforms,
    type WindState,
  } from '../../shaders/grass-material'

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
  const MESH_CAPACITY = 10240
  const FLOWER_MESH_CAPACITY = 2048

  // ── Async-loaded geometry & materials ─────────────────
  // Stored for reference/disposal only; meshes are created imperatively below
  let _grassGeometry: THREE.BufferGeometry | null = null
  let _flowerGeometry: THREE.BufferGeometry | null = null
  let _shortGrassMaterial: THREE.Material | null = null
  let _tallGrassMaterial: THREE.Material | null = null
  let _flowerMaterial: THREE.Material | null = null
  let allUniforms: GrassMaterialUniforms[] = []
  let baseWindStrengths: number[] = []
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
    if (!ensureMaterials() || !_grassGeometry || !_flowerGeometry) return false
    // Create at least one mesh per type so the pipeline exists in the scene
    ensureSlotMesh(shortMeshes, 0, _grassGeometry, _shortGrassMaterial!, MESH_CAPACITY)
    ensureSlotMesh(tallMeshes, 0, _grassGeometry, _tallGrassMaterial!, MESH_CAPACITY)
    ensureSlotMesh(flowerMeshes, 0, _flowerGeometry, _flowerMaterial!, FLOWER_MESH_CAPACITY)
    return true
  }
  let shortMeshes: THREE.InstancedMesh[] = []
  let tallMeshes: THREE.InstancedMesh[] = []
  let flowerMeshes: THREE.InstancedMesh[] = []

  // Load grass assets in parallel; defer material + mesh creation.
  // Meshes are created lazily (not all 50 upfront) to spread the cost.
  let _grassAlphaMap: THREE.Texture | null = null
  let _flowerColorMap: THREE.Texture | null = null

  Promise.all([
    loadGrassBillboardGeometry(),
    loadFlowerBillboardGeometry(),
    loadGrassAlphaTexture(),
    loadFlowerColorTexture(),
  ]).then(([geometry, flowerGeometry, alphaMap, flowerColorMap]) => {
    _grassGeometry = geometry
    _flowerGeometry = flowerGeometry
    _grassAlphaMap = alphaMap
    _flowerColorMap = flowerColorMap
    assetsReady = true
  })

  /** Create grass materials on first use. */
  function ensureMaterials(): boolean {
    if (_shortGrassMaterial && _tallGrassMaterial && _flowerMaterial) return true
    if (!_grassGeometry || !_flowerGeometry || !_grassAlphaMap || !_flowerColorMap) return false

    const shortResult = createGrassMaterial({ alphaMap: _grassAlphaMap })
    const tallResult = createGrassMaterial({ ...TALL_GRASS_CONFIG, alphaMap: _grassAlphaMap })
    const flowerResult = createGrassMaterial({ ...FLOWER_CONFIG, colorMap: _flowerColorMap })
    _shortGrassMaterial = shortResult.material
    _tallGrassMaterial = tallResult.material
    _flowerMaterial = flowerResult.material

    allUniforms = [shortResult.uniforms, tallResult.uniforms, flowerResult.uniforms]
    baseWindStrengths = allUniforms.map((u) => u.uWindStrength.value)
    return true
  }

  /** Get or create the slot mesh at the given index. */
  function ensureSlotMesh(
    pool: THREE.InstancedMesh[],
    index: number,
    geometry: THREE.BufferGeometry,
    material: THREE.Material,
    capacity = MESH_CAPACITY,
  ): THREE.InstancedMesh {
    if (!pool[index]) {
      pool[index] = createSlotMesh(geometry, material, capacity)
    }
    return pool[index]
  }

  function createSlotMesh(
    baseGeometry: THREE.BufferGeometry,
    material: THREE.Material,
    capacity = MESH_CAPACITY,
  ): THREE.InstancedMesh {
    const geom = baseGeometry.clone()
    geom.setAttribute(
      GRASS_INSTANCE_POS_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(capacity * 2), 2),
    )
    geom.setAttribute(
      GRASS_INSTANCE_ROT_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(capacity), 1),
    )
    const mesh = new THREE.InstancedMesh(geom, material, capacity)
    // Do NOT set mesh.count = 0 here! WebGPU allocates GPU buffers based on
    // mesh.count at first render. If 0, the buffer can never grow later.
    // MESH_CAPACITY instances with zero matrices are invisible (zero scale).
    mesh.castShadow = false
    mesh.receiveShadow = true
    mesh.frustumCulled = false
    return mesh
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

  // ── Player interaction trail with decay ────────────────────
  const TRAIL_MIN_DIST = 0.5
  const TRAIL_RISE = 8.0
  const TRAIL_DECAY = 1.5
  const trail: { x: number; z: number; strength: number; decaying: boolean }[] = []
  let lastTrailX = 0
  let lastTrailZ = 0
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

  export function update(deltaTime: number) {
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

    // Rise until peak, then decay. Prune dead points.
    for (let i = trail.length - 1; i >= 0; i--) {
      if (trail[i].strength < 1.0 && !trail[i].decaying) {
        trail[i].strength = Math.min(1.0, trail[i].strength + TRAIL_RISE * dt)
        if (trail[i].strength >= 1.0) trail[i].decaying = true
      } else {
        trail[i].decaying = true
        trail[i].strength -= TRAIL_DECAY * dt
      }
      if (trail[i].strength <= 0) trail.splice(i, 1)
    }

    if (playerPosition) {
      const dx = playerPosition.x - lastTrailX
      const dz = playerPosition.z - lastTrailZ
      if (dx * dx + dz * dz > TRAIL_MIN_DIST * TRAIL_MIN_DIST) {
        if (trail.length >= GRASS_TRAIL_COUNT) trail.shift()
        trail.push({ x: playerPosition.x, z: playerPosition.z, strength: 0, decaying: false })
        lastTrailX = playerPosition.x
        lastTrailZ = playerPosition.z
      }
    }

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

    for (let ui = 0; ui < allUniforms.length; ui++) {
      const u = allUniforms[ui]
      u.uTime.value = elapsedTime
      u.uWindStrength.value = baseWindStrengths[ui] * windStrengthMul
      u.uWindDir.value.set(cachedWindDirX, cachedWindDirZ)
      u.uGustStrength.value = windStrengthMul
      for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
        u.uWaveAngles[wi].value = waveAngles[wi]
        u.uWaveAmps[wi].value = waveAmplitudes[wi]
        u.uWaveParams[wi].value.copy(waveParams[wi])
      }
      for (let i = 0; i < GRASS_TRAIL_COUNT; i++) {
        if (i < trail.length) {
          u.uTrail[i].value.set(trail[i].x, trail[i].z, trail[i].strength)
        } else {
          u.uTrail[i].value.set(0, 0, 0)
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
    rotations: Float32Array
    count: number
  }

  interface SubChunkBundle {
    short: SubChunkData
    tall: SubChunkData
    flower: SubChunkData
  }

  const EMPTY_SUB_CHUNK: SubChunkData = { matrices: new Float32Array(0), worldXZ: new Float32Array(0), rotations: new Float32Array(0), count: 0 }

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
      const matrices = new Float32Array(n * 16)
      const worldXZ = new Float32Array(n * 2)
      const rotations = new Float32Array(n)

      for (let j = 0; j < n; j++) {
        const base = indices[j] * 5
        const x = rawData[base]
        const y = rawData[base + 1]
        const z = rawData[base + 2]
        const rot = rawData[base + 3]
        const scale = rawData[base + 4]

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

        worldXZ[j * 2] = x
        worldXZ[j * 2 + 1] = z
        rotations[j] = rot
      }

      result.set(key, { matrices, worldXZ, rotations, count: n })
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

  // Key-based slot assignment: track which sub-chunk key each mesh displays.
  // When the grid shifts, meshes already showing a still-active key keep their
  // GPU data untouched — only meshes that need NEW data get rewritten.
  // Non-reactive by design — managed imperatively in rebuildType().
  const shortKeyToSlot = new Map<string, number>()
  const tallKeyToSlot = new Map<string, number>()
  const flowerKeyToSlot = new Map<string, number>()

  function rebuildGrassBuffers() {
    needsRebuild = false
    // Lazily create materials on first rebuild with actual data
    if (!ensureMaterials()) return

    const wantedKeys = new Set(getActiveSubChunkKeys())

    rebuildType(shortMeshes, _grassGeometry!, _shortGrassMaterial!, shortKeyToSlot, wantedKeys, (c) => c?.short)
    rebuildType(tallMeshes, _grassGeometry!, _tallGrassMaterial!, tallKeyToSlot, wantedKeys, (c) => c?.tall)
    rebuildType(flowerMeshes, _flowerGeometry!, _flowerMaterial!, flowerKeyToSlot, wantedKeys, (c) => c?.flower, FLOWER_MESH_CAPACITY)
  }

  function rebuildType(
    meshes: THREE.InstancedMesh[],
    geometry: THREE.BufferGeometry,
    material: THREE.Material,
    keyToSlot: Map<string, number>,
    wantedKeys: Set<string>,
    getData: (cached: SubChunkBundle | undefined) => SubChunkData | undefined,
    capacity = MESH_CAPACITY,
  ) {
    // Free slots whose key is no longer in the grid
    const freeSlots: number[] = []
    for (const [key, slot] of keyToSlot) {
      if (!wantedKeys.has(key)) {
        if (meshes[slot]) {
          meshes[slot].count = 0
          // Remove from scene so empty meshes don't waste GPU cycles
          if (meshes[slot].parent) meshes[slot].parent.remove(meshes[slot])
        }
        keyToSlot.delete(key)
        freeSlots.push(slot)
      }
    }

    // Collect unassigned slots
    const usedSlots = new Set(keyToSlot.values())
    for (let i = 0; i < GRID_COUNT; i++) {
      if (!usedSlots.has(i)) freeSlots.push(i)
    }

    // Assign new keys to free slots — mesh is created lazily if needed
    for (const key of wantedKeys) {
      if (keyToSlot.has(key)) continue // already showing correct data

      const data = getData(subChunkCache.get(key))
      if (!data || data.count === 0) continue

      if (freeSlots.length === 0) continue
      const slot = freeSlots.pop()!

      const mesh = ensureSlotMesh(meshes, slot, geometry, material, capacity)
      writeMeshData(mesh, data)
      keyToSlot.set(key, slot)
    }
  }

  function writeMeshData(mesh: THREE.InstancedMesh, data?: SubChunkData) {
    if (!data || data.count === 0) {
      if (mesh.count > 0) mesh.count = 0
      if (mesh.parent) mesh.parent.remove(mesh)
      return
    }

    const count = Math.min(data.count, mesh.instanceMatrix.count)

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

    // Force WebGPU to re-create GPU bindings by re-adding to scene graph.
    // Also handles the initial case where mesh hasn't been added yet.
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
            const shortChunks = partitionIntoSubChunks(getInstanceData(grassData, 'short'))
            const tallChunks = partitionIntoSubChunks(getInstanceData(grassData, 'tall'))
            const flowerChunks = partitionIntoSubChunks(getInstanceData(grassData, 'flower'))

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
        const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
        const tileMaxX = tileX * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
        const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
        const tileMaxZ = tileZ * TERRAIN_TILE_SIZE + TERRAIN_TILE_SIZE / 2
        const scMinX = Math.floor(tileMinX / SUB_CHUNK_SIZE)
        const scMaxX = Math.floor((tileMaxX - 1) / SUB_CHUNK_SIZE)
        const scMinZ = Math.floor(tileMinZ / SUB_CHUNK_SIZE)
        const scMaxZ = Math.floor((tileMaxZ - 1) / SUB_CHUNK_SIZE)
        for (let sz = scMinZ; sz <= scMaxZ; sz++) {
          for (let sx = scMinX; sx <= scMaxX; sx++) {
            subChunkCache.delete(`${sx},${sz}`)
          }
        }
        needsRebuild = true
      }
    }
  })
</script>

<T is={grassGroup} />
<T is={windArrow} />
