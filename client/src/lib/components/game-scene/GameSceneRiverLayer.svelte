<script lang="ts">
  import { T } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import * as THREE from 'three'

  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE, parseTileId } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { RiverDataManager } from '../../managers/riverDataManager'
  import {
    buildChains,
    buildRiverGeometry,
    endpointKey,
    RIVER_DEPTH_OFFSET_M,
    type SeamGhostExtension,
  } from '../../utils/river-geometry'
  import type { RiverSegment } from '../../utils/river-data'
  import {
    createRiverMaterial,
    type RiverMaterialResult,
  } from '../../shaders/river-material'
  import { riverWireframeVisible } from '../../stores/debugStore'
  import {
    WaterfallSprayParticles,
    type WaterfallSprayEmitter,
  } from '../../effects/waterfall-spray-particles'

  /** Depth of the cross-tile ghost extension provided per shared seam.
   *  Should be ≥ BEND_CAP_SMOOTH_RADIUS in river-geometry (=10) so the
   *  cap-smoothing window reaches a fully-anchored neighbor cap on the
   *  far side. A small margin handles future tweaks of that constant. */
  const SEAM_GHOST_EXTENSION_DEPTH = 12

  interface Props {
    terrainTiles: TerrainTile[]
    heightManager: TerrainHeightManager | null
    riverDataManager: RiverDataManager | null
    normalMap?: THREE.Texture | null
    reflectionMap?: THREE.Texture | null
    refractionMap?: THREE.Texture | null
    time?: number
    sunDirection?: THREE.Vector3 | null
    sunColor?: THREE.Color | null
    cameraDirection?: THREE.Vector3 | null
    moonBrightness?: number
    torchLight?: THREE.PointLight | null
    playerPosition?: { x: number; y: number; z: number } | null
  }

  let {
    terrainTiles,
    heightManager,
    riverDataManager,
    normalMap = null,
    reflectionMap = null,
    refractionMap = null,
    time = 0,
    sunDirection = null,
    sunColor = null,
    cameraDirection = null,
    moonBrightness = 0,
    torchLight = null,
    playerPosition = null,
  }: Props = $props()

  const riverGroup = new THREE.Group()
  riverGroup.name = 'rivers'
  const waterfallSpray = new WaterfallSprayParticles()
  riverGroup.add(waterfallSpray.group)

  const WATERFALL_MIN_BEND_DOT = Math.cos(THREE.MathUtils.degToRad(8))
  const WATERFALL_MIN_DROP_M = 0.05
  const WATERFALL_MIN_SLOPE = 0.015
  const WATERFALL_PLAYER_FOCUS_RADIUS = 96

  const wireframeMaterial = new THREE.LineBasicMaterial({
    color: 0xff3366,
    transparent: true,
    opacity: 0.9,
    depthTest: false,
    depthWrite: false,
  })

  export function getGroup(): THREE.Group {
    return riverGroup
  }

  export function update(deltaTime: number, camera: THREE.Camera | undefined) {
    const sunY = sunDirection?.y ?? 0.8
    const dayFactor = THREE.MathUtils.smoothstep(sunY, -0.08, 0.35)
    const lightFactor = THREE.MathUtils.clamp(
      0.34 + dayFactor * 1.35 + moonBrightness * 0.12,
      0.28,
      1.75
    )
    waterfallSpray.setLightFactor(lightFactor)
    waterfallSpray.setDayWhiteness(THREE.MathUtils.clamp(dayFactor * 0.38, 0, 0.38))
    waterfallSpray.setCloudReflectionMix(
      THREE.MathUtils.lerp(0.3, 0.7, dayFactor)
    )
    waterfallSpray.update(deltaTime / 1000, camera)
  }

  // Plain (non-reactive): async load callbacks mutate this, and a reactive
  // dep would retrigger the $effect below and churn frames. Only the
  // `terrainTiles` prop drives the effect. `null` value = processed but
  // no mesh (empty-segment tile).
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileMeshes = new Map<string, THREE.Mesh | null>()
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const wireframeMeshes = new Map<string, THREE.LineSegments>()
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const inflightTiles = new Set<string>()
  // Per-tile build queue. `buildTileMesh` is async and can be invoked
  // concurrently for the same id (placeholder-promotion effect ⨯
  // neighbor-rebuild loop ⨯ initial load). Without serialization two
  // overlapping builds race on `riverGroup.add` / `tileMeshes.set` and
  // can leak a mesh into the scene. Each call awaits the prior in-flight
  // build for that id so disposal-then-add stays atomic.
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const buildChain = new Map<string, Promise<void>>()
  // Per-tile segment cache so we can compute "endpoints present in other
  // tiles" when deciding whether a chain tip is a real mouth (extend into
  // sea) or a tile-seam continuation (skip the extension to avoid two
  // overlapping deltas rendered from both sides of the seam).
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileSegments = new Map<string, RiverSegment[]>()
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileSprayEmitters = new Map<string, WaterfallSprayEmitter[]>()

  // Cross-tile cumulative-length values at shared seam endpoints; see
  // `RiverGeometryResult.publishedOffsets` for the full rationale. The
  // rebuild cascade below propagates values downstream over O(chain
  // depth) rounds.
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const chainOffsets = new Map<string, number>()

  /** Map each shared seam endpoint to a list of K positions stepping
   *  inward along the neighbor tile's chain. Position 0 is the
   *  immediate ghost (used by river-geometry for tangent averaging at
   *  the seam tip); the rest extend the bend-cap smoothing window
   *  across the seam so a tight bend within K vertices of the seam in
   *  one tile propagates to the other side and both tiles compute
   *  identical inside-bank caps. Reuses `buildChains` to share the
   *  degree-1-tip-walking logic with the geometry builder; only
   *  chain ends that are degree-1 (true seam tips, not junctions)
   *  qualify as extension keys. */
  function collectExternalContinuations(
    excludeId: string
  ): Map<string, SeamGhostExtension> {
    /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
    const map = new Map<string, SeamGhostExtension>()
    for (const [id, segs] of tileSegments) {
      if (id === excludeId) continue

      // Endpoint degree: chain tips that aren't degree-1 are junctions
      // (degree ≥ 3) which can't be ghost-extended unambiguously.
      /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
      const degree = new Map<string, number>()
      for (const s of segs) {
        const ka = endpointKey(s.ax, s.az)
        const kb = endpointKey(s.bx, s.bz)
        degree.set(ka, (degree.get(ka) ?? 0) + 1)
        degree.set(kb, (degree.get(kb) ?? 0) + 1)
      }

      for (const chain of buildChains(segs)) {
        if (chain.length === 0) continue
        // Reconstruct vertex positions along the oriented chain
        // (n+1 positions for n links). positions[0] = chain head
        // endpoint, positions[chain.length] = chain tail endpoint.
        const positions: Array<readonly [number, number]> = new Array(
          chain.length + 1
        )
        const first = segs[chain[0].seg]
        positions[0] = chain[0].forward
          ? [first.ax, first.az]
          : [first.bx, first.bz]
        for (let i = 0; i < chain.length; i++) {
          const link = chain[i]
          const s = segs[link.seg]
          positions[i + 1] = link.forward ? [s.bx, s.bz] : [s.ax, s.az]
        }

        const headKey = endpointKey(positions[0][0], positions[0][1])
        if (degree.get(headKey) === 1) {
          const walk = positions.slice(1, 1 + SEAM_GHOST_EXTENSION_DEPTH)
          if (walk.length > 0) map.set(headKey, walk)
        }
        const tailIdx = chain.length
        const tailKey = endpointKey(positions[tailIdx][0], positions[tailIdx][1])
        if (degree.get(tailKey) === 1) {
          const walk: Array<readonly [number, number]> = []
          for (
            let i = tailIdx - 1;
            i >= 0 && walk.length < SEAM_GHOST_EXTENSION_DEPTH;
            i--
          ) {
            walk.push(positions[i])
          }
          if (walk.length > 0) map.set(tailKey, walk)
        }
      }
    }
    return map
  }

  // Per-tile river material — each instance binds to its own tile heightmap
  // so the depth-based edge fade samples the same data the sea shader does
  // and the two boundaries land on the same shoreline contour. Tiles built
  // before normalMap is available carry a transient basic material and are
  // upgraded in the `$effect` below when the shared textures come online.
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileMaterials = new Map<string, RiverMaterialResult>()
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileHeightTextures = new Map<string, THREE.DataTexture>()
  const placeholderMaterial = new THREE.MeshBasicMaterial({
    color: 0x33ccff,
    transparent: true,
    opacity: 0.6,
    depthWrite: false,
    side: THREE.DoubleSide,
  })

  /** Called from the game loop each frame to sync uniforms across all tile
   *  materials. Reflection/refraction textures are captured once at material
   *  creation (they're render targets set up at scene init and never swapped);
   *  WebGPU bind groups lock to the initial reference anyway (see
   *  `webgpu_precompile_bind_group_staleness`), so reassigning them per frame
   *  is a no-op — skip the extra write. */
  export function updateUniforms() {
    for (const result of tileMaterials.values()) {
      const u = result.uniforms
      u.uTime.value = time
      if (sunDirection) u.uSunDirection.value.copy(sunDirection)
      if (sunColor) u.uSunColor.value.copy(sunColor)
      if (cameraDirection) u.uCameraDirection.value.copy(cameraDirection)
      u.uMoonBrightness.value = moonBrightness
      if (torchLight) {
        u.uTorchPos.value.copy(torchLight.position)
        u.uTorchColor.value.copy(torchLight.color)
        u.uTorchIntensity.value = torchLight.intensity
        u.uTorchDistance.value = torchLight.distance
      } else {
        u.uTorchIntensity.value = 0
      }
    }
  }

  function addWireframeForTile(id: string, geometry: THREE.BufferGeometry) {
    if (wireframeMeshes.has(id)) return
    const wf = new THREE.LineSegments(
      new THREE.WireframeGeometry(geometry),
      wireframeMaterial
    )
    wf.renderOrder = 10
    wf.castShadow = false
    wf.receiveShadow = false
    riverGroup.add(wf)
    wireframeMeshes.set(id, wf)
  }

  function removeWireframeForTile(id: string) {
    const wf = wireframeMeshes.get(id)
    if (!wf) return
    riverGroup.remove(wf)
    wf.geometry.dispose()
    wireframeMeshes.delete(id)
  }

  $effect(() => {
    if ($riverWireframeVisible) {
      for (const [id, mesh] of tileMeshes) {
        if (mesh) addWireframeForTile(id, mesh.geometry)
      }
    } else {
      for (const id of [...wireframeMeshes.keys()]) {
        removeWireframeForTile(id)
      }
    }
  })

  function disposeTile(id: string) {
    const mesh = tileMeshes.get(id)
    if (mesh) {
      riverGroup.remove(mesh)
      mesh.geometry.dispose()
      const wf = wireframeMeshes.get(id)
      if (wf) {
        riverGroup.remove(wf)
        wf.geometry.dispose()
        wireframeMeshes.delete(id)
      }
    }
    tileMeshes.delete(id)
    tileSegments.delete(id)
    tileSprayEmitters.delete(id)
    syncSprayEmitters()
    // Drop the per-tile material; pipeline recompile cost is paid on next
    // load. Don't dispose the heightmap texture — Three.js Sampler binding
    // listens for 'dispose' and nullifies .texture, but _init doesn't sync
    // sampler bindings, so a re-pooled material would crash. GC handles it.
    tileMaterials.delete(id)
    tileHeightTextures.delete(id)
    // Drop chainOffsets entries whose endpoints no surviving loaded tile
    // still references — without this the map grows unboundedly across
    // a roaming session as tiles are loaded and disposed.
    /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
    const live = new Set<string>()
    for (const segs of tileSegments.values()) {
      for (const s of segs) {
        live.add(endpointKey(s.ax, s.az))
        live.add(endpointKey(s.bx, s.bz))
      }
    }
    for (const key of [...chainOffsets.keys()]) {
      if (!live.has(key)) chainOffsets.delete(key)
    }
  }

  function syncSprayEmitters() {
    waterfallSpray.setEmitters([...tileSprayEmitters.values()].flat())
  }

  function normalized2(
    x1: number,
    z1: number,
    x2: number,
    z2: number
  ): [number, number, number] {
    const dx = x2 - x1
    const dz = z2 - z1
    const len = Math.hypot(dx, dz)
    if (len < 1e-6) return [0, 1, 0]
    return [dx / len, dz / len, len]
  }

  function buildBendCandidate(
    candidateId: string,
    prev: { x: number; z: number },
    current: { x: number; z: number },
    next: { x: number; z: number },
    width: number
  ): WaterfallSprayEmitter | null {
    if (!heightManager) return null

    const [prevDirX, prevDirZ, prevLen] = normalized2(
      prev.x,
      prev.z,
      current.x,
      current.z
    )
    const [nextDirX, nextDirZ, nextLen] = normalized2(
      current.x,
      current.z,
      next.x,
      next.z
    )
    if (prevLen < 0.5 || nextLen < 0.5) return null

    const bendDot = prevDirX * nextDirX + prevDirZ * nextDirZ
    if (bendDot > WATERFALL_MIN_BEND_DOT) return null

    if (playerPosition) {
      const playerDist = Math.hypot(
        current.x - playerPosition.x,
        current.z - playerPosition.z
      )
      if (playerDist > WATERFALL_PLAYER_FOCUS_RADIUS) return null
    }

    if (
      !heightManager.hasHeightData(prev.x, prev.z) ||
      !heightManager.hasHeightData(current.x, current.z) ||
      !heightManager.hasHeightData(next.x, next.z)
    ) {
      return null
    }

    const yPrev = heightManager.getHeightAtWorldPosition(prev.x, prev.z)
    const yCurrent = heightManager.getHeightAtWorldPosition(
      current.x,
      current.z
    )
    const yNext = heightManager.getHeightAtWorldPosition(next.x, next.z)
    const drop = Math.max(yPrev - yCurrent, yCurrent - yNext, yPrev - yNext, 0)
    const slope = drop / (prevLen + nextLen)
    if (drop < WATERFALL_MIN_DROP_M || slope < WATERFALL_MIN_SLOPE) return null

    const bendStrength = 1 - THREE.MathUtils.clamp(bendDot, -1, 1)
    const terrainSlope = (yNext - yCurrent) / nextLen
    const launchSlope = THREE.MathUtils.clamp(terrainSlope + 0.16, -0.55, 0.28)
    const intensity = THREE.MathUtils.clamp(
      5.5 + drop * 4 + bendStrength * 9 + slope * 12,
      6,
      14
    )
    const radius = THREE.MathUtils.clamp(width * 0.75 + 1, 2.2, 5.8)

    return {
      id: candidateId,
      x: current.x - nextDirX * Math.min(0.85, nextLen * 0.32),
      y: yCurrent + RIVER_DEPTH_OFFSET_M + 0.05,
      z: current.z - nextDirZ * Math.min(0.85, nextLen * 0.32),
      dirX: nextDirX,
      dirZ: nextDirZ,
      radius,
      intensity,
      launchSlope,
    }
  }

  function collectSprayEmitters(
    id: string,
    segments: RiverSegment[]
  ): WaterfallSprayEmitter[] {
    if (!heightManager) return []

    const externalContinuations = collectExternalContinuations(id)
    let best: WaterfallSprayEmitter | null = null
    const consider = (candidate: WaterfallSprayEmitter | null) => {
      if (!candidate) return
      if (!best || candidate.intensity > best.intensity) best = candidate
    }

    for (const chain of buildChains(segments)) {
      if (chain.length < 2) continue

      const n = chain.length
      const px: number[] = new Array(n + 1)
      const pz: number[] = new Array(n + 1)
      const widths: number[] = new Array(n + 1)

      let firstFlow = 0
      let lastFlow = 0
      for (let i = 0; i < n; i++) {
        const link = chain[i]
        const segment = segments[link.seg]
        const ax = link.forward ? segment.ax : segment.bx
        const az = link.forward ? segment.az : segment.bz
        const bx = link.forward ? segment.bx : segment.ax
        const bz = link.forward ? segment.bz : segment.az
        const wa = link.forward ? segment.widthA : segment.widthB
        const wb = link.forward ? segment.widthB : segment.widthA
        const fa = link.forward ? segment.flowNormA : segment.flowNormB
        const fb = link.forward ? segment.flowNormB : segment.flowNormA

        if (i === 0) {
          px[0] = ax
          pz[0] = az
          widths[0] = wa
          firstFlow = fa
        }
        px[i + 1] = bx
        pz[i + 1] = bz
        widths[i + 1] = wb
        if (i === n - 1) lastFlow = fb
      }

      if (lastFlow < firstFlow) {
        px.reverse()
        pz.reverse()
        widths.reverse()
      }

      const point = (i: number) => ({ x: px[i], z: pz[i] })

      for (let i = 1; i < n; i++) {
        consider(
          buildBendCandidate(
            `${id}:bend:${i}`,
            point(i - 1),
            point(i),
            point(i + 1),
            widths[i]
          )
        )
      }

      const headKey = endpointKey(px[0], pz[0])
      const headExtension = externalContinuations.get(headKey)
      if (headExtension && headExtension.length > 0) {
        consider(
          buildBendCandidate(
            `${id}:bend:head`,
            { x: headExtension[0][0], z: headExtension[0][1] },
            point(0),
            point(1),
            widths[0]
          )
        )
      }

      const tailKey = endpointKey(px[n], pz[n])
      const tailExtension = externalContinuations.get(tailKey)
      if (tailExtension && tailExtension.length > 0) {
        consider(
          buildBendCandidate(
            `${id}:bend:tail`,
            point(n - 1),
            point(n),
            { x: tailExtension[0][0], z: tailExtension[0][1] },
            widths[n]
          )
        )
      }
    }

    return best ? [best] : []
  }

  function refreshSprayEmitters(id: string, segments: RiverSegment[]) {
    const emitters = collectSprayEmitters(id, segments)
    if (emitters.length > 0) tileSprayEmitters.set(id, emitters)
    else tileSprayEmitters.delete(id)
    syncSprayEmitters()
  }

  interface SpillBindings {
    xTex: THREE.DataTexture | null
    zTex: THREE.DataTexture | null
    xzTex: THREE.DataTexture | null
    // Worldspace tile-min on each axis the ribbon spills into. Defaults
    // to the owner's own min when that axis has no spill — pre-baked by
    // the caller so `ensureTileMaterial` doesn't repeat the math.
    xMinX: number
    zMinZ: number
  }

  function tileMinFromCoords(tileX: number, tileZ: number): [number, number] {
    return [
      tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2,
      tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2,
    ]
  }

  /** Create-on-demand the per-tile river material. All spill bindings
   *  must be resolved up-front — WebGPU bind groups lock to the initial
   *  texture references at compile time, so swapping samplers
   *  post-creation is a no-op (see memory:
   *  webgpu_precompile_bind_group_staleness). When the cached material
   *  was built with different bindings, drop and recreate. Returns null
   *  when normalMap / coords aren't ready yet (caller falls back to
   *  `placeholderMaterial`). */
  function ensureTileMaterial(
    id: string,
    heightTex: THREE.DataTexture,
    spill: SpillBindings
  ): RiverMaterialResult | null {
    const xTex = spill.xTex ?? heightTex
    const zTex = spill.zTex ?? heightTex
    const xzTex = spill.xzTex ?? heightTex
    const cached = tileMaterials.get(id)
    if (cached) {
      if (
        cached.uniforms.uHeightmapXTexture.value === xTex &&
        cached.uniforms.uHeightmapZTexture.value === zTex &&
        cached.uniforms.uHeightmapXZTexture.value === xzTex
      ) {
        return cached
      }
      tileMaterials.delete(id)
    }
    if (!normalMap) return null
    const coords = parseTileId(id)
    if (!coords) return null
    const result = createRiverMaterial({
      normalMap,
      heightmapTexture: heightTex,
      heightmapXTexture: xTex,
      heightmapZTexture: zTex,
      heightmapXZTexture: xzTex,
      reflectionMap,
      refractionMap,
    })
    const [tileMinX, tileMinZ] = tileMinFromCoords(coords.tileX, coords.tileZ)
    result.uniforms.uTileMin.value.set(tileMinX, tileMinZ)
    result.uniforms.uTileMinX.value.set(spill.xMinX, tileMinZ)
    result.uniforms.uTileMinZ.value.set(tileMinX, spill.zMinZ)
    tileMaterials.set(id, result)
    return result
  }

  /** Pick the dominant-spill neighbor on each axis. Both axes spilling
   *  produces a diagonal-corner overshoot the caller must also load —
   *  if the corner is left unbound, alpha freezes in a small rectangular
   *  patch where the ribbon enters the corner quadrant. */
  function determineSpillNeighbors(
    id: string,
    geometry: THREE.BufferGeometry
  ): { xTileX: number | null; zTileZ: number | null } | null {
    const coords = parseTileId(id)
    if (!coords) return null
    const bbox = geometry.boundingBox
    if (!bbox) return null
    const [tileMinX, tileMinZ] = tileMinFromCoords(coords.tileX, coords.tileZ)
    const tileMaxX = tileMinX + TERRAIN_TILE_SIZE
    const tileMaxZ = tileMinZ + TERRAIN_TILE_SIZE
    const overMinusX = tileMinX - bbox.min.x
    const overPlusX = bbox.max.x - tileMaxX
    const overMinusZ = tileMinZ - bbox.min.z
    const overPlusZ = bbox.max.z - tileMaxZ
    let xTileX: number | null = null
    if (overPlusX > 0 && overPlusX >= overMinusX) xTileX = coords.tileX + 1
    else if (overMinusX > 0) xTileX = coords.tileX - 1
    let zTileZ: number | null = null
    if (overPlusZ > 0 && overPlusZ >= overMinusZ) zTileZ = coords.tileZ + 1
    else if (overMinusZ > 0) zTileZ = coords.tileZ - 1
    if (xTileX === null && zTileZ === null) return null
    return { xTileX, zTileZ }
  }

  async function loadNeighborTex(
    tileX: number,
    tileZ: number
  ): Promise<THREE.DataTexture | null> {
    if (!heightManager) return null
    await heightManager.loadHeightmap(tileX, tileZ).catch(() => null)
    return heightManager.getHeightmapTexture(tileX, tileZ)
  }

  /** Acquire-or-refresh the per-tile heightmap texture. Same create-once,
   *  in-place update pattern the water layer uses so the WebGPU bind group
   *  keeps a stable reference (per `webgpu_precompile_bind_group_staleness`). */
  function ensureTileHeightTexture(id: string): THREE.DataTexture | null {
    if (!heightManager) return null
    const coords = parseTileId(id)
    if (!coords) return null
    const cached = tileHeightTextures.get(id)
    if (cached) {
      heightManager.updateHeightmapTexture(coords.tileX, coords.tileZ, cached)
      return cached
    }
    const tex = heightManager.getHeightmapTexture(coords.tileX, coords.tileZ)
    if (!tex) return null
    tileHeightTextures.set(id, tex)
    return tex
  }

  function disposePriorMesh(id: string) {
    const prev = tileMeshes.get(id)
    if (prev) {
      riverGroup.remove(prev)
      prev.geometry.dispose()
    }
    const prevWf = wireframeMeshes.get(id)
    if (prevWf) {
      riverGroup.remove(prevWf)
      prevWf.geometry.dispose()
      wireframeMeshes.delete(id)
    }
  }

  /** Public entry point. Serializes per-id so concurrent invocations
   *  from the placeholder-promotion `$effect`, the neighbor-rebuild
   *  loop, and the initial load can't race on `riverGroup.add` /
   *  `tileMeshes.set` and leak a mesh into the scene. */
  function buildTileMesh(id: string, segments: RiverSegment[]): Promise<void> {
    const prior = buildChain.get(id) ?? Promise.resolve()
    const next = prior
      .catch(() => undefined)
      .then(() => buildTileMeshInner(id, segments))
    buildChain.set(id, next)
    return next.finally(() => {
      if (buildChain.get(id) === next) buildChain.delete(id)
    })
  }

  async function buildTileMeshInner(id: string, segments: RiverSegment[]) {
    const externalContinuations = collectExternalContinuations(id)
    const { geometry, vertexCount, publishedOffsets } = buildRiverGeometry(
      segments,
      heightManager,
      externalContinuations,
      chainOffsets
    )
    // Publish into the shared map; downstream tile reads on next rebuild.
    for (const [seamKey, value] of publishedOffsets) {
      chainOffsets.set(seamKey, value)
    }
    if (vertexCount === 0) {
      geometry.dispose()
      disposePriorMesh(id)
      tileMeshes.set(id, null)
      tileSprayEmitters.delete(id)
      syncSprayEmitters()
      return
    }

    const ownerCoords = parseTileId(id)
    const spill = determineSpillNeighbors(id, geometry)
    const [ownerMinX, ownerMinZ] = ownerCoords
      ? tileMinFromCoords(ownerCoords.tileX, ownerCoords.tileZ)
      : [0, 0]
    const spillBindings: SpillBindings = {
      xTex: null,
      zTex: null,
      xzTex: null,
      xMinX: ownerMinX,
      zMinZ: ownerMinZ,
    }
    if (spill && heightManager && ownerCoords) {
      const xT = spill.xTileX
      const zT = spill.zTileZ
      const [xTexLoad, zTexLoad, xzTexLoad] = await Promise.all([
        xT !== null ? loadNeighborTex(xT, ownerCoords.tileZ) : null,
        zT !== null ? loadNeighborTex(ownerCoords.tileX, zT) : null,
        xT !== null && zT !== null ? loadNeighborTex(xT, zT) : null,
      ])
      if (xT !== null) {
        spillBindings.xTex = xTexLoad
        spillBindings.xMinX = tileMinFromCoords(xT, ownerCoords.tileZ)[0]
      }
      if (zT !== null) {
        spillBindings.zTex = zTexLoad
        spillBindings.zMinZ = tileMinFromCoords(ownerCoords.tileX, zT)[1]
      }
      // Corner: if only one axis spills, the corner sample is unreachable
      // (the corresponding half-plane test stays 0 in valid fragment
      // ranges) — folding it to that single axis neighbor keeps the
      // sampler bound to a real texture without affecting output.
      spillBindings.xzTex =
        xzTexLoad ?? spillBindings.xTex ?? spillBindings.zTex
    }

    disposePriorMesh(id)

    const heightTex = ensureTileHeightTexture(id)
    const matResult = heightTex
      ? ensureTileMaterial(id, heightTex, spillBindings)
      : null
    const meshMaterial: THREE.Material = matResult?.material ?? placeholderMaterial
    const mesh = new THREE.Mesh(geometry, meshMaterial)
    mesh.receiveShadow = false
    mesh.castShadow = false
    // River ribbon and sea quad both use alpha blending with depthWrite
    // off, so three.js sorts them by distance — and for overlapping
    // flat surfaces that sort flips across the camera's frustum, showing
    // the river above the sea in one tile and below it in the next
    // (visible as a diagonal seam at the mouth). Force river strictly
    // after sea with a higher renderOrder so estuary blending is stable.
    mesh.renderOrder = 1
    riverGroup.add(mesh)
    tileMeshes.set(id, mesh)
    refreshSprayEmitters(id, segments)

    if ($riverWireframeVisible) {
      addWireframeForTile(id, geometry)
    }
  }

  async function loadRiverTile(
    id: string,
    tileX: number,
    tileZ: number
  ): Promise<void> {
    if (inflightTiles.has(id) || tileMeshes.has(id)) return
    if (!riverDataManager || !heightManager) return
    inflightTiles.add(id)
    try {
      const [, data] = await Promise.all([
        heightManager.loadHeightmap(tileX, tileZ).catch(() => null),
        riverDataManager.loadRiverData(tileX, tileZ),
      ])
      if (!data || data.segments.length === 0) {
        tileMeshes.set(id, null)
        return
      }
      tileSegments.set(id, data.segments)
      await buildTileMesh(id, data.segments)

      // Rebuild every other tile with segments — seam-shared status
      // only becomes known now that our segments landed. Don't gate on
      // `tileMeshes.get(otherId)`: an in-progress first build started
      // before our `tileSegments.set` and saw an empty ghost set, so
      // its smoothing moved a vertex we treat as a ghost reference.
      // `buildTileMesh` serializes per-id; this rebuild queues behind.
      const rebuilds: Promise<void>[] = []
      for (const [otherId, segs] of tileSegments) {
        if (otherId === id) continue
        rebuilds.push(buildTileMesh(otherId, segs))
      }
      await Promise.all(rebuilds)
    } finally {
      inflightTiles.delete(id)
    }
  }

  // Promote tile meshes from placeholder to per-tile river materials once
  // normalMap arrives. Tiles built before that point still hold the
  // placeholder; rebuild via `buildTileMesh` (rather than a hot material
  // swap) so the spill neighbor binding lands at material-compile time
  // along with the primary heightmap.
  $effect(() => {
    if (!normalMap) return
    for (const [id, mesh] of tileMeshes) {
      if (!mesh) continue
      if (mesh.material !== placeholderMaterial) continue
      const segs = tileSegments.get(id)
      if (!segs) continue
      void buildTileMesh(id, segs)
    }
  })

  $effect(() => {
    if (!riverDataManager || !heightManager) return

    const currentIds = new Set(terrainTiles.map((t) => t.id))
    for (const id of [...tileMeshes.keys()]) {
      if (!currentIds.has(id)) disposeTile(id)
    }
    for (const tile of terrainTiles) {
      const tileX = Math.round(tile.position[0] / TERRAIN_TILE_SIZE)
      const tileZ = Math.round(tile.position[2] / TERRAIN_TILE_SIZE)
      void loadRiverTile(tile.id, tileX, tileZ)
    }
  })

  onDestroy(() => {
    waterfallSpray.dispose()
  })
</script>

<T is={riverGroup} />
