<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { SvelteMap } from 'svelte/reactivity'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'
  import { windDebugVisible } from '../../stores/debugStore'
  import { getInstanceData } from '../../utils/grass-data'
  import {
    createGrassBladeGeometry,
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

  const GRASS_RADIUS = 30 // grass render distance from player (meters)
  const _dummy = new THREE.Object3D()

  // ── Shared geometries (created once) ─────────────────
  const shortBladeGeometry = createGrassBladeGeometry(0.03, 0.4, 0.4, 0.5)
  const tallBladeGeometry = createGrassBladeGeometry(0.05, 0.8, 0.35, 0.4)

  // ── Shared TSL materials (created once) ──────────────
  const { material: shortGrassMaterial, uniforms: shortGrassUniforms } =
    createGrassMaterial()
  const { material: tallGrassMaterial, uniforms: tallGrassUniforms } =
    createGrassMaterial(TALL_GRASS_CONFIG)

  const allUniforms: GrassMaterialUniforms[] = [
    shortGrassUniforms,
    tallGrassUniforms,
  ]
  const baseWindStrengths = allUniforms.map((u) => u.uWindStrength.value)

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
  const TRAIL_MIN_DIST = 0.5 // min distance between trail points
  const TRAIL_RISE = 8.0 // strength gained per second (ramp up over ~0.15s)
  const TRAIL_DECAY = 1.5 // strength lost per second
  const trail: { x: number; z: number; strength: number; decaying: boolean }[] = []
  let lastTrailX = 0
  let lastTrailZ = 0
  let elapsedTime = 0

  // ── Wind parameters ──────────────────────────────────────
  const WIND_STR_MIN = 0.3
  const WIND_STR_MAX = 1.0
  let windAngle = Math.random() * Math.PI * 2
  let windStrengthMul = 0.5

  // Rest-start snapshots for smoothstep interpolation
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
  const GUST_BAND_STAGGER_MIN = 2.0 // seconds between band starts
  const GUST_BAND_STAGGER_MAX = 5.0
  const GUST_REST_MAX = 10.0

  // ── Gust cycle state machine ───────────────────────────
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

  export function update(deltaTime: number) {
    const dt = Math.min(deltaTime / 1000, 0.1)
    elapsedTime += dt

    // Update snapped player position (THREE.Vector3 is mutated in-place,
    // so $derived cannot track changes — we do it here instead)
    hasPlayer = !!playerPosition
    if (playerPosition) {
      snappedX = Math.round(playerPosition.x / SNAP_SIZE) * SNAP_SIZE
      snappedZ = Math.round(playerPosition.z / SNAP_SIZE) * SNAP_SIZE
    }

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

    // Add new trail point if player moved enough
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

      // Smoothstep interpolation: wind changes during rest
      if (cycleRestDuration > 0) {
        const t = smoothstep(Math.min(1, 1 - cycleRestTimer / cycleRestDuration))
        windStrengthMul = windStrengthStart + (windStrengthTarget - windStrengthStart) * t
        let angleDelta = windAngleTarget - windAngleStart
        angleDelta = ((angleDelta + Math.PI) % (Math.PI * 2)) - Math.PI
        windAngle = windAngleStart + angleDelta * t
      }

      if (cycleRestTimer <= 0) {
        // Snap to targets
        windAngle = windAngleTarget
        windStrengthMul = windStrengthTarget

        // Decide band count: strong → 3, weak → 1, with randomness
        const raw = windStrengthMul * 2 + (Math.random() - 0.3) * 2
        const bandCount = Math.min(GRASS_GUST_COUNT, Math.max(1, Math.round(raw)))

        // Gust speed based on current strength
        gustSpeed = GUST_SPEED_MIN + (GUST_SPEED_MAX - GUST_SPEED_MIN) * windStrengthMul

        // Create bands with staggered starts and evenly spaced phases
        const PHASE_PERIOD = 60
        const phaseBase = Math.random() * PHASE_PERIOD
        const phaseSlice = PHASE_PERIOD / bandCount
        const phaseJitter = phaseSlice * 0.25 // ±25% jitter within each slice
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
        // Enter rest: snapshot current values, pick new targets
        cycleState = 'resting'
        windAngleStart = windAngle
        windStrengthStart = windStrengthMul
        windStrengthTarget = WIND_STR_MIN + Math.random() * (WIND_STR_MAX - WIND_STR_MIN)

        // 90% small turn (±45°), 10% large turn (±45°~±108°)
        const bigTurn = Math.random() < 0.1
        const sign = Math.random() < 0.5 ? -1 : 1
        if (bigTurn) {
          const angle = Math.PI / 4 + Math.random() * (Math.PI * 0.35) // 45°~108°
          windAngleTarget = windAngle + sign * angle
          cycleRestDuration = GUST_REST_MAX - 3 + Math.random() * 3 // 7~10s
        } else {
          const angle = Math.random() * (Math.PI / 4) // 0°~45°
          windAngleTarget = windAngle + sign * angle
          cycleRestDuration = Math.random() * 3 // 0~3s
        }
        cycleRestTimer = cycleRestDuration
      }
    }

    // Write uniforms
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

    // ── Update wind debug arrow ──
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

  // ── Per-tile InstancedMesh maps ──────────────────────
  const shortGrassMap = new SvelteMap<string, THREE.InstancedMesh>()
  const tallGrassMap = new SvelteMap<string, THREE.InstancedMesh>()
  const allMaps = [shortGrassMap, tallGrassMap]

  // Track in-flight generation to avoid duplicates
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  interface VegetationConfig {
    keyPrefix: string
    geometry: THREE.BufferGeometry
    material: THREE.Material
    outputMap: SvelteMap<string, THREE.InstancedMesh>
    grassType: 'short' | 'tall'
  }

  const SHORT_GRASS_CFG: VegetationConfig = {
    keyPrefix: 's',
    geometry: shortBladeGeometry,
    material: shortGrassMaterial,
    outputMap: shortGrassMap,
    grassType: 'short',
  }

  const TALL_GRASS_CFG: VegetationConfig = {
    keyPrefix: 't',
    geometry: tallBladeGeometry,
    material: tallGrassMaterial,
    outputMap: tallGrassMap,
    grassType: 'tall',
  }

  const allConfigs = [SHORT_GRASS_CFG, TALL_GRASS_CFG]

  // ── Create InstancedMesh from pre-computed binary data ──
  function createMeshFromPrecomputed(
    cfg: VegetationConfig,
    instanceData: Float32Array,
    tileId: string,
  ): boolean {
    const count = instanceData.length / 5
    if (count === 0) return true // nothing to render, still "success"

    const tileGeometry = cfg.geometry.clone()
    const instancedMesh = new THREE.InstancedMesh(tileGeometry, cfg.material, count)
    instancedMesh.castShadow = false
    instancedMesh.receiveShadow = true
    instancedMesh.frustumCulled = true

    const worldXZArray = new Float32Array(count * 2)
    const rotationArray = new Float32Array(count)

    for (let i = 0; i < count; i++) {
      const base = i * 5
      const x = instanceData[base]
      const y = instanceData[base + 1]
      const z = instanceData[base + 2]
      const rotation = instanceData[base + 3]
      const scale = instanceData[base + 4]

      _dummy.position.set(x, y, z)
      _dummy.rotation.set(0, rotation, 0)
      _dummy.scale.setScalar(scale)
      _dummy.updateMatrix()
      instancedMesh.setMatrixAt(i, _dummy.matrix)
      worldXZArray[i * 2] = x
      worldXZArray[i * 2 + 1] = z
      rotationArray[i] = rotation
    }

    instancedMesh.instanceMatrix.needsUpdate = true
    instancedMesh.geometry.setAttribute(
      GRASS_INSTANCE_POS_ATTR,
      new THREE.InstancedBufferAttribute(worldXZArray, 2),
    )
    instancedMesh.geometry.setAttribute(
      GRASS_INSTANCE_ROT_ATTR,
      new THREE.InstancedBufferAttribute(rotationArray, 1),
    )
    instancedMesh.computeBoundingBox()
    instancedMesh.computeBoundingSphere()

    cfg.outputMap.set(tileId, instancedMesh)
    return true
  }

  // Quantize player position to avoid re-running $effect on every frame.
  // Only changes when player moves across a SNAP_SIZE boundary.
  // NOTE: playerPosition is a THREE.Vector3 mutated in-place, so $derived
  // cannot track .x/.z changes. We update these in update() instead.
  const SNAP_SIZE = 8
  let snappedX = $state(0)
  let snappedZ = $state(0)
  let hasPlayer = $state(false)

  function isTileInGrassRange(tile: TerrainTile, px: number, pz: number): boolean {
    const half = TERRAIN_TILE_SIZE / 2
    const tileMinX = tile.position[0] - half
    const tileMaxX = tile.position[0] + half
    const tileMinZ = tile.position[2] - half
    const tileMaxZ = tile.position[2] + half
    const dx = Math.max(tileMinX - px, 0, px - tileMaxX)
    const dz = Math.max(tileMinZ - pz, 0, pz - tileMaxZ)
    return dx * dx + dz * dz < GRASS_RADIUS * GRASS_RADIUS
  }

  function disposeMeshFromMap(map: SvelteMap<string, THREE.InstancedMesh>, id: string) {
    const mesh = map.get(id)
    if (mesh) {
      mesh.geometry.dispose()
      mesh.dispose()
      map.delete(id)
    }
  }

  // ── Tile lifecycle ─────────────────────────────────────
  $effect(() => {
    if (!hasPlayer) return
    const px = snappedX
    const pz = snappedZ
    const tileById = new Map(terrainTiles.map((t) => [t.id, t]))

    // Remove grass for tiles no longer in range or no longer visible
    for (const map of allMaps) {
      for (const [id] of map) {
        const tile = tileById.get(id)
        if (!tile || !isTileInGrassRange(tile, px, pz)) {
          disposeMeshFromMap(map, id)
          for (const cfg of allConfigs) pendingTiles.delete(`${cfg.keyPrefix}:${id}`)
        }
      }
    }

    // Load pre-computed grass for new tiles in range
    const gMgr = grassDataManager
    for (const tile of terrainTiles) {
      if (!isTileInGrassRange(tile, px, pz)) continue

      // Skip if all types already exist or are pending
      const allReady = allConfigs.every(
        (cfg) => cfg.outputMap.has(tile.id) || pendingTiles.has(`${cfg.keyPrefix}:${tile.id}`),
      )
      if (allReady) continue

      if (!gMgr) continue

      const { tileX, tileZ } = getTileCoords(tile)
      const tileId = tile.id

      // Mark all configs as pending
      for (const cfg of allConfigs) {
        const key = `${cfg.keyPrefix}:${tileId}`
        if (!cfg.outputMap.has(tileId) && !pendingTiles.has(key)) {
          pendingTiles.add(key)
        }
      }

      gMgr
        .loadGrassData(tileX, tileZ)
        .then((grassData) => {
          if (grassData) {
            for (const cfg of allConfigs) {
              const key = `${cfg.keyPrefix}:${tileId}`
              if (cfg.outputMap.has(tileId) || !pendingTiles.has(key)) continue
              const instanceData = getInstanceData(grassData, cfg.grassType)
              createMeshFromPrecomputed(cfg, instanceData, tileId)
              pendingTiles.delete(key)
            }
          } else {
            // No pre-computed data — don't render grass for this tile
            for (const cfg of allConfigs) pendingTiles.delete(`${cfg.keyPrefix}:${tileId}`)
          }
        })
        .catch(() => {
          for (const cfg of allConfigs) pendingTiles.delete(`${cfg.keyPrefix}:${tileId}`)
        })
    }
  })
</script>

{#each [...shortGrassMap] as [tileId, mesh] (tileId)}
  <T is={mesh} />
{/each}
{#each [...tallGrassMap] as [tileId, mesh] (`tall_${tileId}`)}
  <T is={mesh} />
{/each}
<T is={windArrow} />
