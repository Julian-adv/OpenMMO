<script lang="ts">
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import SplatTerrain from '../SplatTerrain.svelte'
  import {
    makeSplatStandardMaterial,
  } from '../makeSplatStandardMaterial'
  import type { ResolvedRegionLayers } from '../../managers/terrainMetaManager'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import { loadSplatLayers } from '../../utils/splatLayerLoader'
  import { mapEditorMode, gridVisible } from '../../stores/debugStore'
  import { brushWorldPos, brushSize, brushMode, editorTool, regionMetaVersion, currentEditorRegion } from '../../stores/editorStore'
  import type { BrushMode, EditorTool } from '../../stores/editorStore'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    heightManager?: TerrainHeightManager | null
    splatManager?: TerrainSplatManager | null
    metaManager?: TerrainMetaManager | null
    syncTileMeshes?: () => void
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    heightManager = null,
    splatManager = null,
    metaManager = null,
    syncTileMeshes = $bindable<() => void>(() => {}),
  }: Props = $props()

  // ── Shared material (created once) ──────────────────────
  let sharedMaterial = $state<MeshStandardNodeMaterial | null>(null)
  let defaultRegionLayers: ResolvedRegionLayers | null = null
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

  // Placeholder textures for missing normal/ORM maps
  const placeholderNorm = new THREE.DataTexture(
    new Uint8Array([128, 128, 255, 255]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  placeholderNorm.needsUpdate = true

  const placeholderORM = new THREE.DataTexture(
    new Uint8Array([255, 255, 0, 255]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  placeholderORM.needsUpdate = true

  loadSplatLayers().then((layers) => {
    defaultRegionLayers = { layers }
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

  // ── Geometry management (SvelteMap, needed for template) ──────
  const geoMap = new SvelteMap<string, THREE.BufferGeometry>()

  // ── Per-tile render data (plain Map, read by onBeforeRender) ──
  interface TileRenderState {
    splatTexture: THREE.Texture | null
    regionLayers: ResolvedRegionLayers | null
  }
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- intentionally non-reactive; read in onBeforeRender hot path, not in template
  const tileRenderState = new Map<string, TileRenderState>()

  function getTileCoords(tile: TerrainTile): { tileX: number; tileZ: number } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  // ── onBeforeRender setup (called from game loop) ────────────
  // Sets up onBeforeRender on any mesh that doesn't have it yet.
  // The callback reads per-tile data from the plain tileRenderState Map,
  // so it always gets the latest values without Svelte reactivity.
  syncTileMeshes = () => {
    const mat = sharedMaterial
    if (!mat) return
    for (let i = 0; i < terrainTiles.length; i++) {
      const mesh = terrainMeshes[i]
      if (!mesh || mesh.userData.__tileOBR) continue
      const tile = terrainTiles[i]
      if (!tile) continue

      mesh.userData.__tileOBR = true
      const tileId = tile.id
      mesh.onBeforeRender = () => {
        const u = mat.userData?.uniforms
        if (!u) return

        const rs = tileRenderState.get(tileId)
        const splat = rs?.splatTexture ?? defaultSplat
        const rl = rs?.regionLayers ?? defaultRegionLayers

        u.splatMap.value = splat

        if (rl) {
          u.diffTex0.value = rl.layers[0].map
          u.diffTex1.value = rl.layers[1].map
          u.diffTex2.value = rl.layers[2].map
          u.diffTex3.value = rl.layers[3].map

          if (u.normTex0) {
            u.normTex0.value = rl.layers[0].normalMap ?? placeholderNorm
            u.normTex1.value = rl.layers[1].normalMap ?? placeholderNorm
            u.normTex2.value = rl.layers[2].normalMap ?? placeholderNorm
            u.normTex3.value = rl.layers[3].normalMap ?? placeholderNorm
          }

          if (u.ormTex0) {
            u.ormTex0.value = rl.layers[0].orm ?? placeholderORM
            u.ormTex1.value = rl.layers[1].orm ?? placeholderORM
            u.ormTex2.value = rl.layers[2].orm ?? placeholderORM
            u.ormTex3.value = rl.layers[3].orm ?? placeholderORM
          }

          u.uTile0.value = rl.layers[0].tile
          u.uTile1.value = rl.layers[1].tile
          u.uTile2.value = rl.layers[2].tile
          u.uTile3.value = rl.layers[3].tile
        }

        splat.needsUpdate = true
      }
    }
  }

  // ── Edge refresh queue ──────────────────────────────────
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const edgeRefreshQueued = new Set<string>()

  function scheduleEdgeRefresh(tileX: number, tileZ: number) {
    if (!heightManager) return
    for (let dz = -1; dz <= 1; dz++) {
      for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dz === 0) continue
        const nx = tileX + dx
        const nz = tileZ + dz
        const key = `${nx},${nz}`
        if (edgeRefreshQueued.has(key)) continue
        const geo = geoMap.get(`${nx}_${nz}`)
        if (geo && heightManager.getHeightmap(nx, nz)) {
          edgeRefreshQueued.add(key)
          enqueueTileWork(() => {
            edgeRefreshQueued.delete(key)
            heightManager?.applyHeightToGeometry(nx, nz, geo)
          })
        }
      }
    }
  }

  // ── Tile lifecycle (geometry + async data loading) ──────────
  $effect(() => {
    if (!terrainGeometry || !heightManager) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove data for tiles no longer in the list
    for (const [id, geo] of geoMap) {
      if (!currentTileIds.has(id)) {
        geo.dispose()
        geoMap.delete(id)
        tileRenderState.delete(id)
      }
    }

    // Create geometries + kick off async loads for new tiles
    const mgr = heightManager
    const sMgr = splatManager
    const mMgr = metaManager
    for (const tile of terrainTiles) {
      if (geoMap.has(tile.id)) continue

      const geo = terrainGeometry.clone()
      geoMap.set(tile.id, geo)

      // Create mutable render state (read by onBeforeRender each frame)
      const rs: TileRenderState = { splatTexture: null, regionLayers: null }
      tileRenderState.set(tile.id, rs)

      const { tileX, tileZ } = getTileCoords(tile)
      mgr.registerGeometry(tileX, tileZ, geo)

      mgr.loadHeightmap(tileX, tileZ).then(() => {
        mgr.applyHeightToGeometry(tileX, tileZ, geo)
        scheduleEdgeRefresh(tileX, tileZ)
      })

      if (sMgr) {
        sMgr.loadSplatmap(tileX, tileZ).then((tex) => {
          rs.splatTexture = tex
        })
      }

      if (mMgr) {
        mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
          rs.regionLayers = resolved
        })
      }
    }
  })

  // Re-resolve region layers when meta changes (texture swap in SplatBrushPanel)
  regionMetaVersion.subscribe((ver) => {
    if (ver === 0 || !metaManager) return
    const region = get(currentEditorRegion)
    if (!region) return
    const { rx, rz } = region
    const mMgr = metaManager

    for (const tile of terrainTiles) {
      const { tileX, tileZ } = getTileCoords(tile)
      if (tileToRegion(tileX) === rx && tileToRegion(tileZ) === rz) {
        const rs = tileRenderState.get(tile.id)
        if (rs) {
          mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
            rs.regionLayers = resolved
          })
        }
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
        bind:mesh={terrainMeshes[index]}
      />
    {/if}
  {/each}
{/if}
