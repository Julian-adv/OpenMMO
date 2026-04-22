<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'

  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { RiverDataManager } from '../../managers/riverDataManager'
  import { buildRiverGeometry } from '../../utils/river-geometry'

  interface Props {
    terrainTiles: TerrainTile[]
    heightManager: TerrainHeightManager | null
    riverDataManager: RiverDataManager | null
  }

  let { terrainTiles, heightManager, riverDataManager }: Props = $props()

  const riverGroup = new THREE.Group()
  riverGroup.name = 'rivers'

  // Plain (non-reactive): async load callbacks mutate this, and a reactive
  // dep would retrigger the $effect below and churn frames. Only the
  // `terrainTiles` prop drives the effect. `null` value = processed but
  // no mesh (empty-segment tile).
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const tileMeshes = new Map<string, THREE.Mesh | null>()
  /* eslint-disable-next-line svelte/prefer-svelte-reactivity */
  const inflightTiles = new Set<string>()

  const debugMaterial = new THREE.MeshBasicMaterial({
    color: 0x33ccff,
    transparent: true,
    opacity: 0.7,
    depthWrite: false,
    side: THREE.DoubleSide,
  })

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
      const mesh = new THREE.Mesh(geometry, debugMaterial)
      mesh.receiveShadow = false
      mesh.castShadow = false
      riverGroup.add(mesh)
      tileMeshes.set(id, mesh)
    } finally {
      inflightTiles.delete(id)
    }
  }

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
