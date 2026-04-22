<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'

  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { RiverDataManager } from '../../managers/riverDataManager'
  import { buildRiverGeometry } from '../../utils/river-geometry'
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
  const inflightTiles = new Set<string>()

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
    }
    tileMeshes.delete(id)
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
      const { geometry, vertexCount } = buildRiverGeometry(
        data.segments,
        heightManager
      )
      if (vertexCount === 0) {
        geometry.dispose()
        tileMeshes.set(id, null)
        return
      }
      const mesh = new THREE.Mesh(geometry, currentMaterial())
      mesh.receiveShadow = false
      mesh.castShadow = false
      riverGroup.add(mesh)
      tileMeshes.set(id, mesh)
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
