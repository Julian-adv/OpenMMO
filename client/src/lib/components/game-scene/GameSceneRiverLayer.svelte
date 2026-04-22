<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'

  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
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
  const SHOW_WIREFRAME = false
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
  // Per-tile segment cache so we can compute "endpoints present in other
  // tiles" when deciding whether a chain tip is a real mouth (extend into
  // sea) or a tile-seam continuation (skip the extension to avoid two
  // overlapping deltas rendered from both sides of the seam).
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileSegments = new Map<string, RiverSegment[]>()

  function collectExternalEndpoints(excludeId: string): Set<string> {
    /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
    const set = new Set<string>()
    for (const [id, segs] of tileSegments) {
      if (id === excludeId) continue
      for (const s of segs) {
        set.add(endpointKey(s.ax, s.az))
        set.add(endpointKey(s.bx, s.bz))
      }
    }
    return set
  }

  // One material shared across tiles — all ribbons use the same uniforms.
  // Created lazily once both textures are available; any tile meshes built
  // before creation carry a transient basic material and are upgraded in the
  // $effect below when the shared material comes online.
  let riverMaterialResult: RiverMaterialResult | null = null
  const placeholderMaterial = new THREE.MeshBasicMaterial({
    color: 0x33ccff,
    transparent: true,
    opacity: 0.6,
    depthWrite: false,
    side: THREE.DoubleSide,
  })

  function currentMaterial(): THREE.Material {
    return riverMaterialResult?.material ?? placeholderMaterial
  }

  /** Called from the game loop each frame to sync uniforms. */
  export function updateUniforms() {
    if (!riverMaterialResult) return
    const u = riverMaterialResult.uniforms
    u.uTime.value = time
    if (sunDirection) u.uSunDirection.value.copy(sunDirection)
    if (sunColor) u.uSunColor.value.copy(sunColor)
    if (cameraDirection) u.uCameraDirection.value.copy(cameraDirection)
    u.uMoonBrightness.value = moonBrightness
    if (reflectionMap) u.uReflectionMap.value = reflectionMap
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
  }

  function buildTileMesh(id: string, segments: RiverSegment[]) {
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

    const externalEndpoints = collectExternalEndpoints(id)
    const { geometry, vertexCount } = buildRiverGeometry(
      segments,
      heightManager,
      externalEndpoints
    )
    if (vertexCount === 0) {
      geometry.dispose()
      tileMeshes.set(id, null)
      return
    }
    const mesh = new THREE.Mesh(geometry, currentMaterial())
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
      await heightManager.loadHeightmap(tileX, tileZ).catch(() => null)
      const data = await riverDataManager.loadRiverData(tileX, tileZ)
      if (!data || data.segments.length === 0) {
        tileMeshes.set(id, null)
        return
      }
      tileSegments.set(id, data.segments)
      buildTileMesh(id, data.segments)

      // Rebuild any already-built neighbor tiles — their chains may have
      // terminated at a tile-seam point whose "shared with neighbor" status
      // only becomes known now that this tile's segments are loaded.
      for (const [otherId, segs] of tileSegments) {
        if (otherId === id) continue
        if (!tileMeshes.get(otherId)) continue
        buildTileMesh(otherId, segs)
      }
    } finally {
      inflightTiles.delete(id)
    }
  }

  // Promote tile meshes from placeholder to the shared river material once
  // the required textures are available.
  $effect(() => {
    if (riverMaterialResult || !normalMap) return
    riverMaterialResult = createRiverMaterial({
      normalMap,
      reflectionMap,
    })
    const mat = riverMaterialResult.material
    for (const mesh of tileMeshes.values()) {
      if (mesh) mesh.material = mat
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
