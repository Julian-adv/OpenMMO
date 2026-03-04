<script lang="ts">
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import { hoveredCell, brushSize, brushStrength, brushRaiseMode, brushMode, brushWorldPos, cursorHeight, editorTool, splatLayer, editorPanOffset, currentRegionLayers, textureNameToLabel, currentEditorRegion, currentRegionConfigs, editorMetaManager } from '../../stores/editorStore'
  import type { EditorTool } from '../../stores/editorStore'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { ORTHOGRAPHIC_FRUSTUM_HEIGHT } from '../game-scene/camera-utils'
  import { get } from 'svelte/store'
  import type { TerrainTile } from '../game-scene/terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'

  const LAYER_COLORS = ['#66cc66', '#999999', '#bb7744', '#ddeeff']

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    terrainTiles: TerrainTile[]
    heightManager: TerrainHeightManager | null
    splatManager: TerrainSplatManager | null
    metaManager: TerrainMetaManager | null
  }

  let { camera, terrainMeshes, terrainTiles: _terrainTiles, heightManager, splatManager, metaManager = null }: Props = $props()

  let isPainting = $state(false)
  let isPanning = $state(false)
  let lastPanX = $state(0)
  let lastPanY = $state(0)
  let shiftHeld = $state(false)
  let ctrlHeld = $state(false)
  let lastPaintTime = $state(0)

  let currentBrushSize = $state(3)
  let currentBrushStrength = $state(5)
  let currentBrushRaise = $state(true)
  let currentTool = $state<EditorTool>('height')
  let currentSplatLayer = $state(0)

  brushSize.subscribe((v) => (currentBrushSize = v))
  brushStrength.subscribe((v) => (currentBrushStrength = v))
  brushRaiseMode.subscribe((v) => {
    currentBrushRaise = v
    syncBrushMode()
  })
  editorTool.subscribe((v) => (currentTool = v))
  splatLayer.subscribe((v) => (currentSplatLayer = v))

  function syncBrushMode() {
    if (ctrlHeld) {
      brushMode.set('flatten')
    } else {
      const raise = shiftHeld ? !currentBrushRaise : currentBrushRaise
      brushMode.set(raise ? 'raise' : 'lower')
    }
  }

  const raycaster = new THREE.Raycaster()
  const mouseNDC = new THREE.Vector2()
  const _panRight = new THREE.Vector3()
  const _panUp = new THREE.Vector3()
  const _panFwd = new THREE.Vector3()

  let lastWorldPos = { x: 0, z: 0 }
  let lastRegionX = NaN
  let lastRegionZ = NaN

  function raycastTerrain(event: MouseEvent): THREE.Intersection | null {
    if (!camera) return null

    const meshes = terrainMeshes.filter((m): m is THREE.Mesh => m !== undefined)
    if (meshes.length === 0) return null

    const rect = (event.target as HTMLElement).getBoundingClientRect()
    mouseNDC.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )

    raycaster.setFromCamera(mouseNDC, camera)
    const intersects = raycaster.intersectObjects(meshes, false)
    return intersects.length > 0 ? intersects[0] : null
  }

  function updateCursorFromHit(hit: THREE.Intersection) {
    const mesh = hit.object as THREE.Mesh

    const localX = hit.point.x - mesh.position.x
    const localZ = hit.point.z - mesh.position.z

    const cellX = Math.max(0, Math.min(63, Math.floor(localX + TERRAIN_TILE_SIZE / 2)))
    const cellZ = Math.max(0, Math.min(63, Math.floor(localZ + TERRAIN_TILE_SIZE / 2)))

    const tileX = Math.round(mesh.position.x / TERRAIN_TILE_SIZE)
    const tileZ = Math.round(mesh.position.z / TERRAIN_TILE_SIZE)

    const worldX = mesh.position.x - TERRAIN_TILE_SIZE / 2 + cellX + 0.5
    const worldZ = mesh.position.z - TERRAIN_TILE_SIZE / 2 + cellZ + 0.5

    hoveredCell.set({ tileX, tileZ, cellX, cellZ, worldX, worldZ })
    lastWorldPos = { x: hit.point.x, z: hit.point.z }
    brushWorldPos.set({ x: hit.point.x, z: hit.point.z })

    if (heightManager) {
      cursorHeight.set(heightManager.getHeightAtCell(tileX, tileZ, cellX, cellZ))
    }

    // Update splat layer labels when region changes
    if (metaManager) {
      const rx = tileToRegion(tileX)
      const rz = tileToRegion(tileZ)
      if (rx !== lastRegionX || rz !== lastRegionZ) {
        lastRegionX = rx
        lastRegionZ = rz
        currentEditorRegion.set({ rx, rz })
        const meta = metaManager.getMetaForTile(tileX, tileZ)
        if (meta) {
          currentRegionConfigs.set([...meta.layers])
          currentRegionLayers.set(
            meta.layers.map((l, i) => ({
              label: textureNameToLabel(l.texture),
              color: LAYER_COLORS[i] ?? '#ffffff',
            }))
          )
        }
      }
    }
  }

  function getPaintIntervalMs(): number {
    return (11 - currentBrushStrength) * 100
  }

  function applyBrushAtCursor() {
    const now = performance.now()
    if (lastPaintTime === 0) {
      lastPaintTime = now
      return
    }
    const elapsed = now - lastPaintTime
    if (elapsed < getPaintIntervalMs()) return
    lastPaintTime = now

    if (currentTool === 'splat') {
      if (!splatManager) return
      splatManager.applySplatBrush(
        lastWorldPos.x,
        lastWorldPos.z,
        currentBrushSize,
        currentSplatLayer,
        currentBrushStrength / 50
      )
    } else {
      if (!heightManager) return
      if (ctrlHeld) {
        heightManager.applyFlatten(
          lastWorldPos.x,
          lastWorldPos.z,
          currentBrushSize
        )
      } else {
        const raise = shiftHeld ? !currentBrushRaise : currentBrushRaise
        heightManager.applyBrush(
          lastWorldPos.x,
          lastWorldPos.z,
          currentBrushSize,
          0.1,
          raise,
          1
        )
      }
    }
  }

  function handleMouseMove(event: MouseEvent) {
    if (isPanning) {
      if (!camera) return
      const dx = event.clientX - lastPanX
      const dy = event.clientY - lastPanY
      lastPanX = event.clientX
      lastPanY = event.clientY

      // Get camera basis vectors projected onto XZ plane
      camera.matrixWorld.extractBasis(_panRight, _panUp, _panFwd)
      _panRight.y = 0
      _panRight.normalize()
      _panFwd.y = 0
      _panFwd.normalize()

      // Convert screen pixels to world units for orthographic camera
      const rect = (event.target as HTMLElement).getBoundingClientRect()
      const scale = ORTHOGRAPHIC_FRUSTUM_HEIGHT / (camera.zoom * rect.height)

      const current = get(editorPanOffset)
      editorPanOffset.set({
        x: current.x - (_panRight.x * dx + _panFwd.x * dy) * scale,
        z: current.z - (_panRight.z * dx + _panFwd.z * dy) * scale,
      })
      return
    }

    const hit = raycastTerrain(event)

    if (!hit) {
      hoveredCell.set(null)
      brushWorldPos.set(null)
      return
    }

    updateCursorFromHit(hit)

    if (isPainting) {
      applyBrushAtCursor()
    }
  }

  function handleMouseDown(event: MouseEvent) {
    if (event.button === 1) {
      event.preventDefault()
      isPanning = true
      lastPanX = event.clientX
      lastPanY = event.clientY
      return
    }
    if (event.button !== 0) return
    event.preventDefault()
    const hit = raycastTerrain(event)
    if (!hit) return

    isPainting = true
    lastPaintTime = 0
    updateCursorFromHit(hit)
  }

  function handleMouseUp(event: MouseEvent) {
    if (event.button === 1) {
      isPanning = false
      return
    }
    if (event.button !== 0) return
    isPainting = false
    lastPaintTime = 0
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      shiftHeld = true
      syncBrushMode()
    }
    if (event.key === 'Control') {
      ctrlHeld = true
      syncBrushMode()
    }
  }

  function handleKeyUp(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      shiftHeld = false
      syncBrushMode()
    }
    if (event.key === 'Control') {
      ctrlHeld = false
      syncBrushMode()
    }
  }

  function handleWheel(event: WheelEvent) {
    if (event.ctrlKey) {
      event.preventDefault()
      const delta = event.deltaY > 0 ? -1 : 1
      const newSize = Math.max(1, Math.min(10, currentBrushSize + delta))
      brushSize.set(newSize)
    } else {
      if (!camera) return
      event.preventDefault()
      const factor = event.deltaY > 0 ? 0.95 : 1 / 0.95
      camera.zoom = Math.max(0.15, Math.min(2, camera.zoom * factor))
      camera.updateProjectionMatrix()
    }
  }

  function handleMouseOut() {
    hoveredCell.set(null)
    cursorHeight.set(null)
    brushWorldPos.set(null)
    isPainting = false
    isPanning = false
    lastPaintTime = 0
  }

  onMount(() => {
    if (metaManager) editorMetaManager.set(metaManager)

    const canvas = document.querySelector('canvas')
    if (!canvas) return

    canvas.addEventListener('mousemove', handleMouseMove, true)
    canvas.addEventListener('mousedown', handleMouseDown, true)
    canvas.addEventListener('mouseup', handleMouseUp, true)
    canvas.addEventListener('mouseleave', handleMouseOut)
    canvas.addEventListener('wheel', handleWheel, { passive: false })
    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('keyup', handleKeyUp)

    return () => {
      canvas.removeEventListener('mousemove', handleMouseMove, true)
      canvas.removeEventListener('mousedown', handleMouseDown, true)
      canvas.removeEventListener('mouseup', handleMouseUp, true)
      canvas.removeEventListener('mouseleave', handleMouseOut)
      canvas.removeEventListener('wheel', handleWheel)
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('keyup', handleKeyUp)
      hoveredCell.set(null)
      brushWorldPos.set(null)
    }
  })
</script>
