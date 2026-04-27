<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'

  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE, parseTileId } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { RiverDataManager } from '../../managers/riverDataManager'
  import { buildRiverGeometry, endpointKey } from '../../utils/river-geometry'
  import type { RiverSegment } from '../../utils/river-data'
  import {
    createRiverMaterial,
    type RiverMaterialResult,
  } from '../../shaders/river-material'

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
  }: Props = $props()

  const riverGroup = new THREE.Group()
  riverGroup.name = 'rivers'

  // Debug: overlay the ribbon's triangle edges so the tessellation is
  // visible. Flip to false (or wire to a UI toggle) to disable.
  const SHOW_WIREFRAME = true
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

  /** Map each shared seam endpoint to the other endpoint of the neighbor
   *  tile's segment that touches it. The river-geometry ribbon loop uses
   *  this as a "ghost point" so the tangent at a tile-seam chain tip is
   *  averaged across the split — both tiles then bevel the ribbon
   *  identically at the shared centerline point. */
  function collectExternalContinuations(
    excludeId: string
     
  ): Map<string, [number, number]> {
    /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
    const map = new Map<string, [number, number]>()
    for (const [id, segs] of tileSegments) {
      if (id === excludeId) continue
      for (const s of segs) {
        map.set(endpointKey(s.ax, s.az), [s.bx, s.bz])
        map.set(endpointKey(s.bx, s.bz), [s.ax, s.az])
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
    }
  }

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
    // Drop the per-tile material; pipeline recompile cost is paid on next
    // load. Don't dispose the heightmap texture — Three.js Sampler binding
    // listens for 'dispose' and nullifies .texture, but _init doesn't sync
    // sampler bindings, so a re-pooled material would crash. GC handles it.
    tileMaterials.delete(id)
    tileHeightTextures.delete(id)
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
    const { geometry, vertexCount } = buildRiverGeometry(
      segments,
      heightManager,
      externalContinuations
    )
    if (vertexCount === 0) {
      geometry.dispose()
      disposePriorMesh(id)
      tileMeshes.set(id, null)
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

    if (SHOW_WIREFRAME) {
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
</script>

<T is={riverGroup} />
