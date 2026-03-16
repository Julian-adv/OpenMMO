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
    loadGrassAlphaTexture,
    createGrassMaterial,
    GRASS_INSTANCE_POS_ATTR,
    GRASS_INSTANCE_ROT_ATTR,
    GRASS_TRAIL_COUNT,
    GRASS_GUST_COUNT,
    TALL_GRASS_CONFIG,
    type GrassMaterialUniforms,
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
  const SUB_CHUNK_SIZE = 16
  const SUB_CHUNK_GRID_RADIUS = 2 // 2 = 5×5 grid (80m coverage)
  const GRID_COUNT = (SUB_CHUNK_GRID_RADIUS * 2 + 1) ** 2 // 25
  const MESH_CAPACITY = 1024

  // ── Async-loaded geometry & materials ─────────────────
  // Stored for reference/disposal only; meshes are created imperatively below
  let _grassGeometry: THREE.BufferGeometry | null = null
  let _shortGrassMaterial: THREE.Material | null = null
  let _tallGrassMaterial: THREE.Material | null = null
  let allUniforms: GrassMaterialUniforms[] = []
  let baseWindStrengths: number[] = []
  let assetsReady = $state(false)

  // ── Mesh management via THREE.Group (no Svelte proxy) ──
  const grassGroup = new THREE.Group()

  /** Expose grassGroup for visibility toggling during render passes */
  export function getGroup(): THREE.Group {
    return grassGroup
  }
  let shortMeshes: THREE.InstancedMesh[] = []
  let tallMeshes: THREE.InstancedMesh[] = []

  Promise.all([loadGrassBillboardGeometry(), loadGrassAlphaTexture()]).then(
    ([geometry, alphaMap]) => {
      _grassGeometry = geometry

      const shortResult = createGrassMaterial({ alphaMap })
      const tallResult = createGrassMaterial({ ...TALL_GRASS_CONFIG, alphaMap })
      _shortGrassMaterial = shortResult.material
      _tallGrassMaterial = tallResult.material

      allUniforms = [shortResult.uniforms, tallResult.uniforms]
      baseWindStrengths = allUniforms.map((u) => u.uWindStrength.value)

      shortMeshes = Array.from({ length: GRID_COUNT }, () =>
        createSlotMesh(geometry, shortResult.material),
      )
      tallMeshes = Array.from({ length: GRID_COUNT }, () =>
        createSlotMesh(geometry, tallResult.material),
      )

      // Add all meshes to the group imperatively — bypasses Svelte entirely
      for (const m of shortMeshes) grassGroup.add(m)
      for (const m of tallMeshes) grassGroup.add(m)

      assetsReady = true
    },
  )

  function createSlotMesh(
    baseGeometry: THREE.BufferGeometry,
    material: THREE.Material,
  ): THREE.InstancedMesh {
    const geom = baseGeometry.clone()
    geom.setAttribute(
      GRASS_INSTANCE_POS_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(MESH_CAPACITY * 2), 2),
    )
    geom.setAttribute(
      GRASS_INSTANCE_ROT_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(MESH_CAPACITY), 1),
    )
    const mesh = new THREE.InstancedMesh(geom, material, MESH_CAPACITY)
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
  const GUST_ARROW_COLOR = 0xff4444
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
  let windAngle = Math.random() * Math.PI * 2
  let windStrengthMul = 0.5

  let windAngleStart = windAngle
  let windAngleTarget = windAngle
  let windStrengthStart = windStrengthMul
  let windStrengthTarget = windStrengthMul

  // ── Gust cycle constants ───────────────────────────────
  const GUST_SPEED_MIN = 0.8
  const GUST_SPEED_MAX = 5.0
  const GUST_FADE_IN = 2.0
  const GUST_ACTIVE_MIN = 10
  const GUST_ACTIVE_MAX = 25
  const GUST_FADE_OUT = 3.0
  const GUST_BAND_STAGGER_MIN = 2.0
  const GUST_BAND_STAGGER_MAX = 5.0
  const GUST_REST_MAX = 10.0

  interface GustBand {
    phase: number
    intensity: number
    state: 'waiting' | 'fade-in' | 'active' | 'fade-out' | 'done'
    timer: number
    activeTime: number
  }

  function smoothstep(t: number): number {
    return t * t * (3 - 2 * t)
  }

  let cycleState: 'resting' | 'gusting' = 'resting'
  let cycleRestTimer = 0
  let cycleRestDuration = 0
  let activeBands: GustBand[] = []
  let gustSpeed = 0

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

    // ── Gust cycle state machine ──
    if (cycleState === 'resting') {
      cycleRestTimer -= dt
      if (cycleRestDuration > 0) {
        const t = smoothstep(Math.min(1, 1 - cycleRestTimer / cycleRestDuration))
        windStrengthMul = windStrengthStart + (windStrengthTarget - windStrengthStart) * t
        let angleDelta = windAngleTarget - windAngleStart
        angleDelta = ((angleDelta + Math.PI) % (Math.PI * 2)) - Math.PI
        windAngle = windAngleStart + angleDelta * t
      }
      if (cycleRestTimer <= 0) {
        windAngle = windAngleTarget
        windStrengthMul = windStrengthTarget
        const raw = windStrengthMul * 2 + (Math.random() - 0.3) * 2
        const bandCount = Math.min(GRASS_GUST_COUNT, Math.max(1, Math.round(raw)))
        gustSpeed = GUST_SPEED_MIN + (GUST_SPEED_MAX - GUST_SPEED_MIN) * windStrengthMul
        const PHASE_PERIOD = 60
        const phaseBase = Math.random() * PHASE_PERIOD
        const phaseSlice = PHASE_PERIOD / bandCount
        const phaseJitter = phaseSlice * 0.25
        activeBands = []
        for (let i = 0; i < bandCount; i++) {
          const stagger = i * (GUST_BAND_STAGGER_MIN + Math.random() * (GUST_BAND_STAGGER_MAX - GUST_BAND_STAGGER_MIN))
          const phase = phaseBase + i * phaseSlice + (Math.random() - 0.5) * 2 * phaseJitter
          activeBands.push({
            phase,
            intensity: 0,
            state: 'waiting',
            timer: stagger,
            activeTime: GUST_ACTIVE_MIN + Math.random() * (GUST_ACTIVE_MAX - GUST_ACTIVE_MIN),
          })
        }
        cycleState = 'gusting'
      }
    }

    const windDirX = Math.cos(windAngle)
    const windDirZ = Math.sin(windAngle)

    if (cycleState === 'gusting') {
      const intensityScale = 0.3 + windStrengthMul * 0.7
      let allDone = true
      for (const b of activeBands) {
        if (b.state === 'done') continue
        allDone = false
        b.timer -= dt
        while (b.timer <= 0 && b.state !== 'done') {
          switch (b.state) {
            case 'waiting': {
              b.state = 'fade-in'
              b.timer += GUST_FADE_IN
              break
            }
            case 'fade-in': {
              b.state = 'active'
              b.timer += b.activeTime
              break
            }
            case 'active': {
              b.state = 'fade-out'
              b.timer += GUST_FADE_OUT
              break
            }
            case 'fade-out': {
              b.state = 'done'
              break
            }
          }
        }
        switch (b.state) {
          case 'waiting':
          case 'done': {
            b.intensity = 0
            break
          }
          case 'fade-in': {
            b.intensity = (1 - b.timer / GUST_FADE_IN) * intensityScale
            break
          }
          case 'active': {
            b.intensity = intensityScale
            break
          }
          case 'fade-out': {
            b.intensity = (b.timer / GUST_FADE_OUT) * intensityScale
            break
          }
        }
        b.phase += gustSpeed * dt
      }
      if (allDone) {
        cycleState = 'resting'
        windAngleStart = windAngle
        windStrengthStart = windStrengthMul
        windStrengthTarget = WIND_STR_MIN + Math.random() * (WIND_STR_MAX - WIND_STR_MIN)
        const bigTurn = Math.random() < 0.1
        const sign = Math.random() < 0.5 ? -1 : 1
        if (bigTurn) {
          const angle = Math.PI / 4 + Math.random() * (Math.PI * 0.35)
          windAngleTarget = windAngle + sign * angle
          cycleRestDuration = GUST_REST_MAX - 3 + Math.random() * 3
        } else {
          const angle = Math.random() * (Math.PI / 4)
          windAngleTarget = windAngle + sign * angle
          cycleRestDuration = Math.random() * 3
        }
        cycleRestTimer = cycleRestDuration
      }
    }

    for (let ui = 0; ui < allUniforms.length; ui++) {
      const u = allUniforms[ui]
      u.uTime.value = elapsedTime
      u.uWindStrength.value = baseWindStrengths[ui] * windStrengthMul
      u.uWindDir.value.set(windDirX, windDirZ)
      for (let gi = 0; gi < GRASS_GUST_COUNT; gi++) {
        if (gi < activeBands.length) {
          u.uGustPhase[gi].value = activeBands[gi].phase
          u.uGustIntensity[gi].value = activeBands[gi].intensity
        } else {
          u.uGustIntensity[gi].value = 0
        }
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
      windArrowDir.set(windDirX, 0, windDirZ)
      windArrow.position.set(playerPosition.x, playerPosition.y + 3, playerPosition.z)
      windArrow.setDirection(windArrowDir)
      windArrow.setLength(arrowLen, arrowLen * 0.2, arrowLen * 0.1)
      const anyGustActive = activeBands.some((b) => b.intensity > 0.1)
      const arrowColor = anyGustActive ? GUST_ARROW_COLOR : WIND_ARROW_COLOR
      windArrow.setColor(arrowColor)
    }
  }

  // ── Sub-chunk data cache ──────────────────────────────
  interface SubChunkData {
    matrices: Float32Array
    worldXZ: Float32Array
    rotations: Float32Array
    count: number
  }

  const EMPTY_SUB_CHUNK: SubChunkData = { matrices: new Float32Array(0), worldXZ: new Float32Array(0), rotations: new Float32Array(0), count: 0 }

  // Non-reactive internal caches — intentionally plain Map/Set for performance
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const subChunkCache = new Map<string, { short: SubChunkData; tall: SubChunkData }>()
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

  // ── Collect active sub-chunk keys ──
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

  function rebuildGrassBuffers() {
    if (shortMeshes.length === 0) return
    needsRebuild = false

    const wantedKeys = new Set(getActiveSubChunkKeys())

    rebuildType(shortMeshes, shortKeyToSlot, wantedKeys, (c) => c?.short)
    rebuildType(tallMeshes, tallKeyToSlot, wantedKeys, (c) => c?.tall)
  }

  function rebuildType(
    meshes: THREE.InstancedMesh[],
    keyToSlot: Map<string, number>,
    wantedKeys: Set<string>,
    getData: (cached: { short: SubChunkData; tall: SubChunkData } | undefined) => SubChunkData | undefined,
  ) {
    // Free slots whose key is no longer in the grid
    const freeSlots: number[] = []
    for (const [key, slot] of keyToSlot) {
      if (!wantedKeys.has(key)) {
        meshes[slot].count = 0
        keyToSlot.delete(key)
        freeSlots.push(slot)
      }
    }

    // Collect unassigned slots
    const usedSlots = new Set(keyToSlot.values())
    for (let i = 0; i < meshes.length; i++) {
      if (!usedSlots.has(i)) freeSlots.push(i)
    }

    // Assign new keys to free slots
    for (const key of wantedKeys) {
      if (keyToSlot.has(key)) continue // already showing correct data

      const data = getData(subChunkCache.get(key))
      if (!data || data.count === 0) continue

      if (freeSlots.length === 0) continue
      const slot = freeSlots.pop()!

      writeMeshData(meshes[slot], data)
      keyToSlot.set(key, slot)
    }
  }

  function writeMeshData(mesh: THREE.InstancedMesh, data?: SubChunkData) {
    if (!data || data.count === 0) {
      if (mesh.count > 0) mesh.count = 0
      return
    }

    const count = Math.min(data.count, MESH_CAPACITY)

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

    // Force WebGPU to re-create GPU bindings by re-adding to scene graph
    const parent = mesh.parent
    if (parent) {
      parent.remove(mesh)
      parent.add(mesh)
    }
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

            const allKeys = new Set([...shortChunks.keys(), ...tallChunks.keys()])
            for (const key of allKeys) {
              subChunkCache.set(key, {
                short: shortChunks.get(key) ?? EMPTY_SUB_CHUNK,
                tall: tallChunks.get(key) ?? EMPTY_SUB_CHUNK,
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
