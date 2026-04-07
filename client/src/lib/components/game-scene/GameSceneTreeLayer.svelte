<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainTreeDataManager } from '../../managers/terrainTreeDataManager'
  import { getTreeInstanceData, type TreePlacementData } from '../../utils/tree-data'
  import { loadGLB } from '../../utils/gltfCache'

  interface Props {
    terrainTiles: TerrainTile[]
    treeDataManager: TerrainTreeDataManager | null
  }

  let {
    terrainTiles,
    treeDataManager = null,
  }: Props = $props()

  const treeGroup = new THREE.Group()

  export function getGroup(): THREE.Group {
    return treeGroup
  }

  // ── Model templates (lazy loaded) ──────────────────────
  let tree1Scene: THREE.Object3D | null = null
  let tree2Scene: THREE.Object3D | null = null
  let modelsReady = false
  let modelsLoading = false

  async function ensureModelsLoaded(): Promise<boolean> {
    if (modelsReady) return true
    if (modelsLoading) return false
    modelsLoading = true
    try {
      const [gltf1, gltf2] = await Promise.all([
        loadGLB('/models/tree.glb'),
        loadGLB('/models/tree2.glb'),
      ])
      tree1Scene = gltf1.scene
      tree2Scene = gltf2.scene
      modelsReady = true
      return true
    } catch (e) {
      console.error('Failed to load tree models:', e)
      modelsLoading = false
      return false
    }
  }

  // ── Simple per-tile approach: clone scenes per instance ──
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const tileObjects = new Map<string, THREE.Object3D[]>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fetchedTiles = new Set<string>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()

  function placeTreesForTile(
    tileId: string,
    treeData: TreePlacementData
  ) {
    removeTile(tileId)
    const objects: THREE.Object3D[] = []

    const types: ('tree1' | 'tree2')[] = ['tree1', 'tree2']
    const scenes = [tree1Scene, tree2Scene]

    for (let t = 0; t < 2; t++) {
      const scene = scenes[t]
      if (!scene) continue
      const data = getTreeInstanceData(treeData, types[t])
      const count = data.length / 5

      for (let i = 0; i < count; i++) {
        const base = i * 5
        const clone = scene.clone()
        clone.position.set(data[base], data[base + 1], data[base + 2])
        clone.rotation.y = data[base + 3]
        const s = data[base + 4]
        clone.scale.set(s, s, s)
        treeGroup.add(clone)
        objects.push(clone)
      }
    }

    tileObjects.set(tileId, objects)
  }

  function removeTile(tileId: string) {
    const objs = tileObjects.get(tileId)
    if (objs) {
      for (const obj of objs) {
        treeGroup.remove(obj)
      }
      // Do NOT dispose geometry here — clones share geometry with the template
      // scene, so disposing would destroy GPU buffers still used by other clones
      // and future instances.
      tileObjects.delete(tileId)
    }
  }

  export function update() {}

  // ── Invalidation listener ─────────────────────────────
  $effect(() => {
    const tMgr = treeDataManager
    if (!tMgr) return

    return tMgr.onInvalidateAll(() => {
      for (const tileId of tileObjects.keys()) removeTile(tileId)
      fetchedTiles.clear()
      pendingTiles.clear()
    })
  })

  // ── Tile data lifecycle ─────────────────────────────────
  $effect(() => {
    const tMgr = treeDataManager
    if (!tMgr) return

    for (const tile of terrainTiles) {
      const tk = tile.id
      if (fetchedTiles.has(tk) || pendingTiles.has(tk)) continue

      const tileX = Math.round(tile.position[0] / TERRAIN_TILE_SIZE)
      const tileZ = Math.round(tile.position[2] / TERRAIN_TILE_SIZE)

      pendingTiles.add(tk)

      tMgr
        .loadTreeData(tileX, tileZ)
        .then(async (treeData: TreePlacementData | null) => {
          if (!pendingTiles.has(tk)) return
          pendingTiles.delete(tk)

          if (treeData && (treeData.tree1Count > 0 || treeData.tree2Count > 0)) {
            if (!modelsReady) {
              const ok = await ensureModelsLoaded()
              if (!ok) return
            }
            placeTreesForTile(tk, treeData)
          }

          fetchedTiles.add(tk)
        })
        .catch(() => {
          pendingTiles.delete(tk)
        })
    }

    const tileIds = new Set(terrainTiles.map((t) => t.id))
    for (const tk of fetchedTiles) {
      if (!tileIds.has(tk)) {
        fetchedTiles.delete(tk)
        removeTile(tk)
      }
    }
  })
</script>

<T is={treeGroup} />
