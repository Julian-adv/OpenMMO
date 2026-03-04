<script lang="ts">
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import { onDestroy } from 'svelte'
  import SplatTerrain from '../SplatTerrain.svelte'
  import {
    makeSplatStandardMaterial,
  } from '../makeSplatStandardMaterial'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager, ResolvedRegionLayers } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import { loadSplatLayers } from '../../utils/splatLayerLoader'
  import { mapEditorMode, gridVisible } from '../../stores/debugStore'
  import { brushWorldPos, brushSize, brushMode, editorTool, regionMetaVersion, currentEditorRegion } from '../../stores/editorStore'
  import type { BrushMode, EditorTool } from '../../stores/editorStore'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    heightManager?: TerrainHeightManager | null
    splatManager?: TerrainSplatManager | null
    metaManager?: TerrainMetaManager | null
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    heightManager = null,
    splatManager = null,
    metaManager = null,
  }: Props = $props()

  // ── Shared material (created once) ──────────────────────
  let sharedMaterial = $state<MeshStandardNodeMaterial | null>(null)
  let brushUnsubs: (() => void)[] = []

  // Default 1x1 all-grass splatmap for initial material creation
  const defaultSplat = new THREE.DataTexture(
    new Uint8Array([255, 0, 0, 0]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  defaultSplat.wrapS = defaultSplat.wrapT = THREE.ClampToEdgeWrapping
  defaultSplat.minFilter = THREE.LinearFilter
  defaultSplat.magFilter = THREE.LinearFilter
  defaultSplat.needsUpdate = true

  loadSplatLayers().then((layers) => {
    sharedMaterial = makeSplatStandardMaterial({
      layers,
      splatMap: defaultSplat,
      splatScale: 1.0,
    })
    setupBrushSync(sharedMaterial)
  })

  function setupBrushSync(mat: MeshStandardNodeMaterial) {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []

    let editorActive = false
    let gridOn = false
    let pos: { x: number; z: number } | null = null
    let size = 3
    let mode: BrushMode = 'raise'
    let tool: EditorTool = 'height'

    const modeToShaderValue: Record<BrushMode, number> = { lower: 0.0, raise: 1.0, flatten: 2.0 }

    function sync() {
      const u = mat.userData?.uniforms
      if (!u) return
      u.gridVisible.value = (editorActive || gridOn) ? 1.0 : 0.0
      if (editorActive && pos) {
        u.brushActive.value = 1.0
        u.brushCenter.value.set(pos.x, pos.z)
        u.brushRadius.value = size
        u.brushRaise.value = modeToShaderValue[mode]
        u.brushToolMode.value = tool === 'splat' ? 1.0 : 0.0
      } else {
        u.brushActive.value = 0.0
      }
    }

    brushUnsubs.push(
      mapEditorMode.subscribe((v) => { editorActive = v; sync() }),
      gridVisible.subscribe((v) => { gridOn = v; sync() }),
      brushWorldPos.subscribe((v) => { pos = v; sync() }),
      brushSize.subscribe((v) => { size = v; sync() }),
      brushMode.subscribe((v) => { mode = v; sync() }),
      editorTool.subscribe((v) => { tool = v; sync() }),
    )
  }

  onDestroy(() => {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []
  })

  // ── Geometry, splatmap & region layer management ──────────
  // Use SvelteMaps directly in the template (keyed by tile id) to avoid
  // index mismatch when terrainTiles reorders during tile transitions.
  const geoMap = new SvelteMap<string, THREE.BufferGeometry>()
  const splatTexMap = new SvelteMap<string, THREE.Texture>()
  const regionLayerMap = new SvelteMap<string, ResolvedRegionLayers>()

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  // ── Batched edge refresh ──────────────────────────────
  // When multiple tiles load in the same microtask (common on localhost),
  // batch their edge refreshes to avoid redundant applyHeightToGeometry calls.
  let pendingEdgeTiles: { tileX: number; tileZ: number }[] = []
  let edgeBatchScheduled = false

  function scheduleEdgeRefresh(tileX: number, tileZ: number) {
    pendingEdgeTiles.push({ tileX, tileZ })
    if (!edgeBatchScheduled) {
      edgeBatchScheduled = true
      requestAnimationFrame(flushEdgeRefreshes)
    }
  }

  function flushEdgeRefreshes() {
    edgeBatchScheduled = false
    if (!heightManager) return
    const tiles = pendingEdgeTiles
    pendingEdgeTiles = []

    // Collect unique neighbor tiles that need re-application (dedup)
    const newlyLoaded = new Set(tiles.map((t) => `${t.tileX},${t.tileZ}`))
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const toRefresh = new Set<string>()
    for (const { tileX, tileZ } of tiles) {
      for (const { dx, dz } of [{ dx: -1, dz: 0 }, { dx: 0, dz: -1 }, { dx: -1, dz: -1 }]) {
        const nk = `${tileX + dx},${tileZ + dz}`
        if (!newlyLoaded.has(nk)) toRefresh.add(nk)
      }
    }

    // Also refresh newly loaded tiles that are neighbors of OTHER newly loaded tiles
    for (const { tileX, tileZ } of tiles) {
      const key = `${tileX},${tileZ}`
      for (const other of tiles) {
        for (const { dx, dz } of [{ dx: -1, dz: 0 }, { dx: 0, dz: -1 }, { dx: -1, dz: -1 }]) {
          if (other.tileX + dx === tileX && other.tileZ + dz === tileZ) {
            toRefresh.add(key)
          }
        }
      }
    }

    const mgr = heightManager
    for (const key of toRefresh) {
      const [tx, tz] = key.split(',').map(Number)
      const geoKey = `${tx}_${tz}`
      const geo = geoMap.get(geoKey)
      if (geo && mgr.getHeightmap(tx, tz)) {
        mgr.applyHeightToGeometry(tx, tz, geo)
      }
    }
  }

  $effect(() => {
    if (!terrainGeometry || !heightManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove geometries for tiles no longer in the list
    for (const [id, geo] of geoMap) {
      if (!currentTileIds.has(id)) {
        geo.dispose()
        geoMap.delete(id)
        splatTexMap.delete(id)
        regionLayerMap.delete(id)
      }
    }

    // Create geometries for new tiles
    const mgr = heightManager
    const sMgr = splatManager
    const mMgr = metaManager
    for (const tile of terrainTiles) {
      if (geoMap.has(tile.id)) continue

      const geo = terrainGeometry.clone()
      geoMap.set(tile.id, geo)

      const { tileX, tileZ } = getTileCoords(tile)
      mgr.registerGeometry(tileX, tileZ, geo)

      mgr.loadHeightmap(tileX, tileZ).then(() => {
        mgr.applyHeightToGeometry(tileX, tileZ, geo)
        scheduleEdgeRefresh(tileX, tileZ)
      })

      if (sMgr) {
        sMgr.loadSplatmap(tileX, tileZ).then((tex) => {
          splatTexMap.set(tile.id, tex)
        })
      }

      if (mMgr) {
        mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
          regionLayerMap.set(tile.id, resolved)
        })
      }
    }
  })

  // Re-resolve region layers when meta changes (texture swap in SplatBrushPanel)
  let metaVer = $state(0)
  regionMetaVersion.subscribe((v) => (metaVer = v))

  let changedRegion = $state<{ rx: number; rz: number } | null>(null)
  currentEditorRegion.subscribe((v) => (changedRegion = v))

  $effect(() => {
    if (metaVer === 0 || !changedRegion || !metaManager) return
    const mMgr = metaManager
    const { rx, rz } = changedRegion

    for (const tile of terrainTiles) {
      const { tileX, tileZ } = getTileCoords(tile)
      if (tileToRegion(tileX) === rx && tileToRegion(tileZ) === rz) {
        mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
          regionLayerMap.set(tile.id, resolved)
        })
      }
    }
  })
</script>

{#if terrainGeometry && sharedMaterial}
  {#each terrainTiles as tile, index (tile.id)}
    {@const geo = geoMap.get(tile.id) ?? null}
    {#if geo}
      <SplatTerrain
        geometry={geo}
        material={sharedMaterial}
        position={tile.position}
        splatTexture={splatTexMap.get(tile.id) ?? null}
        regionLayers={regionLayerMap.get(tile.id) ?? null}
        bind:mesh={terrainMeshes[index]}
      />
    {/if}
  {/each}
{/if}
