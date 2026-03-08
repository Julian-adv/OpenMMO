<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { MeshStandardNodeMaterial } from 'three/webgpu'
  import { SvelteMap } from 'svelte/reactivity'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import SplatTerrain from '../SplatTerrain.svelte'
  import {
    makeSplatStandardMaterial,
    createSplatBrushUniforms,
    type SplatBrushUniforms,
  } from '../makeSplatStandardMaterial'
  import type { SplatLayer } from '../makeSplatStandardMaterial'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import { loadSplatLayers, buildSplatAtlas } from '../../utils/splatLayerLoader'
  import type { SplatAtlasSet } from '../../utils/splatLayerLoader'
  import { mapEditorMode, gridVisible } from '../../stores/debugStore'
  import {
    brushWorldPos,
    brushSize,
    brushMode,
    editorTool,
    regionMetaVersion,
    currentEditorRegion,
  } from '../../stores/editorStore'
  import type { BrushMode, EditorTool } from '../../stores/editorStore'
  import { enqueueTileWork } from '../../utils/tileWorkQueue'

  interface Props {
    terrainGeometry: THREE.BufferGeometry | null
    terrainTiles: TerrainTile[]
    terrainMeshes?: (THREE.Mesh | undefined)[]
    terrainGroup?: THREE.Group | undefined
    heightManager?: TerrainHeightManager | null
    splatManager?: TerrainSplatManager | null
    metaManager?: TerrainMetaManager | null
    syncTileMeshes?: () => void
  }

  let {
    terrainGeometry,
    terrainTiles,
    terrainMeshes = $bindable<(THREE.Mesh | undefined)[]>([]),
    terrainGroup = $bindable<THREE.Group | undefined>(undefined),
    heightManager = null,
    splatManager = null,
    metaManager = null,
    syncTileMeshes = $bindable<() => void>(() => {}),
  }: Props = $props()

  // ── Default resources (created once) ──────────────────
  let _defaultLayers: [SplatLayer, SplatLayer, SplatLayer, SplatLayer] | null =
    null
  let defaultAtlas: SplatAtlasSet | null = null
  let sharedMaterial = $state<MeshStandardNodeMaterial | null>(null)
  let brushUnsubs: (() => void)[] = []

  // Default 1x1 all-grass splatmap for tiles whose splatmap hasn't loaded yet
  const defaultSplat = new THREE.DataTexture(
    new Uint8Array([255, 0, 0, 0]),
    1,
    1,
    THREE.RGBAFormat,
    THREE.UnsignedByteType,
  )
  defaultSplat.wrapS = defaultSplat.wrapT = THREE.ClampToEdgeWrapping
  defaultSplat.minFilter = THREE.LinearFilter
  defaultSplat.magFilter = THREE.LinearFilter
  defaultSplat.needsUpdate = true

  // Shared brush/grid uniforms
  const brushUniforms: SplatBrushUniforms = createSplatBrushUniforms()

  loadSplatLayers().then((layers) => {
    _defaultLayers = layers
    defaultAtlas = buildSplatAtlas(layers)
    sharedMaterial = makeSplatStandardMaterial({
      atlas: defaultAtlas,
      tileScales: [layers[0].tile, layers[1].tile, layers[2].tile, layers[3].tile],
      splatMap: defaultSplat,
      splatScale: 1.0,
      sharedBrushUniforms: brushUniforms,
    })
    setupBrushSync()
  })

  // ── Brush sync (updates shared uniform nodes → affects the material) ──
  function setupBrushSync() {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []

    let editorActive = false
    let gridOn = false
    let pos: { x: number; z: number } | null = null
    let size = 3
    let mode: BrushMode = 'raise'
    let tool: EditorTool = 'height'

    const modeToShaderValue: Record<BrushMode, number> = {
      lower: 0.0,
      raise: 1.0,
      flatten: 2.0,
    }

    function sync() {
      brushUniforms.gridVisible.value =
        editorActive || gridOn ? 1.0 : 0.0
      if (editorActive && pos) {
        brushUniforms.brushActive.value = 1.0
        brushUniforms.brushCenter.value.set(pos.x, pos.z)
        brushUniforms.brushRadius.value = size
        brushUniforms.brushRaise.value = modeToShaderValue[mode]
        brushUniforms.brushToolMode.value = tool === 'splat' ? 1.0 : 0.0
      } else {
        brushUniforms.brushActive.value = 0.0
      }
    }

    brushUnsubs.push(
      mapEditorMode.subscribe((v) => {
        editorActive = v
        sync()
      }),
      gridVisible.subscribe((v) => {
        gridOn = v
        sync()
      }),
      brushWorldPos.subscribe((v) => {
        pos = v
        sync()
      }),
      brushSize.subscribe((v) => {
        size = v
        sync()
      }),
      brushMode.subscribe((v) => {
        mode = v
        sync()
      }),
      editorTool.subscribe((v) => {
        tool = v
        sync()
      }),
    )
  }

  onDestroy(() => {
    brushUnsubs.forEach((u) => u())
    brushUnsubs = []
  })

  // ── Geometry management (SvelteMap, needed for template) ──────
  const geoMap = new SvelteMap<string, THREE.BufferGeometry>()

  // ── Per-tile texture data (plain Map, no reactivity needed) ──
  interface TileTexData {
    splatTex: THREE.Texture
    atlas: SplatAtlasSet | null
    tileScales: [number, number, number, number] | null
  }
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const tileTexMap = new Map<string, TileTexData>()

  function getTileCoords(tile: TerrainTile): {
    tileX: number
    tileZ: number
  } {
    return {
      tileX: Math.round(tile.position[0] / TERRAIN_TILE_SIZE),
      tileZ: Math.round(tile.position[2] / TERRAIN_TILE_SIZE),
    }
  }

  // ── syncTileMeshes: swap shared material uniforms per-tile before each render ──
  syncTileMeshes = () => {
    const mat = sharedMaterial
    if (!mat) return
    const u = mat.userData?.uniforms
    if (!u) return

    for (let i = 0; i < terrainMeshes.length; i++) {
      const mesh = terrainMeshes[i]
      if (!mesh) continue
      // Use tileId stored on the mesh itself to avoid index mismatch
      // between terrainMeshes[] (old order) and terrainTiles[] (new order)
      // during the frame when tiles are added/removed.
      const tileId = mesh.userData.tileId as string | undefined
      if (!tileId) continue
      const texData = tileTexMap.get(tileId)

      mesh.onBeforeRender = () => {
        u.splatMap.value = texData?.splatTex ?? defaultSplat
        const tileAtlas = texData?.atlas ?? defaultAtlas
        const scales = texData?.tileScales
        if (tileAtlas) {
          u.diffuseAtlas.value = tileAtlas.diffuseAtlas
          if (u.normalAtlas && tileAtlas.normalAtlas) {
            u.normalAtlas.value = tileAtlas.normalAtlas
          }
          if (u.ormAtlas && tileAtlas.ormAtlas) {
            u.ormAtlas.value = tileAtlas.ormAtlas
          }
        }
        if (scales) {
          u.uTile0.value = scales[0]
          u.uTile1.value = scales[1]
          u.uTile2.value = scales[2]
          u.uTile3.value = scales[3]
        }
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

  // ── Tile lifecycle (geometry + async data loading) ──
  $effect(() => {
    if (!terrainGeometry || !heightManager || !sharedMaterial) return

    const currentTileIds = new Set(terrainTiles.map((t) => t.id))

    // Remove data for tiles no longer in the list
    for (const [id, geo] of geoMap) {
      if (!currentTileIds.has(id)) {
        geo.dispose()
        geoMap.delete(id)
        tileTexMap.delete(id)
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
      tileTexMap.set(tile.id, { splatTex: defaultSplat, atlas: null, tileScales: null })

      const { tileX, tileZ } = getTileCoords(tile)
      mgr.registerGeometry(tileX, tileZ, geo)

      mgr
        .loadHeightmap(tileX, tileZ)
        .then(() => {
          mgr.applyHeightToGeometry(tileX, tileZ, geo)
          scheduleEdgeRefresh(tileX, tileZ)
        })
        .catch(() => {})

      const tileId = tile.id
      if (sMgr) {
        sMgr.loadSplatmap(tileX, tileZ).then((tex) => {
          const td = tileTexMap.get(tileId)
          if (td) td.splatTex = tex
        })
      }

      if (mMgr) {
        mMgr
          .getLayersForTile(tileX, tileZ)
          .then((resolved) => {
            const td = tileTexMap.get(tileId)
            if (td) {
              td.atlas = buildSplatAtlas(resolved.layers)
              td.tileScales = [
                resolved.layers[0].tile, resolved.layers[1].tile,
                resolved.layers[2].tile, resolved.layers[3].tile,
              ]
            }
          })
          .catch(() => {})
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
        mMgr.getLayersForTile(tileX, tileZ).then((resolved) => {
          const td = tileTexMap.get(tile.id)
          if (td) {
            td.atlas = buildSplatAtlas(resolved.layers)
            td.tileScales = [
              resolved.layers[0].tile, resolved.layers[1].tile,
              resolved.layers[2].tile, resolved.layers[3].tile,
            ]
          }
        })
      }
    }
  })
</script>

{#if terrainGeometry && sharedMaterial}
  <T.Group bind:ref={terrainGroup}>
    {#each terrainTiles as tile, index (tile.id)}
      {@const geo = geoMap.get(tile.id) ?? null}
      {#if geo}
        <SplatTerrain
          geometry={geo}
          material={sharedMaterial}
          tileId={tile.id}
          position={tile.position}
          bind:mesh={terrainMeshes[index]}
        />
      {/if}
    {/each}
  </T.Group>
{/if}
