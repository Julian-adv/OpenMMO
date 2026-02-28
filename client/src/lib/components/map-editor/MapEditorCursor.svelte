<script lang="ts">
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import { hoveredCell, brushSize, brushStrength, brushRaiseMode, brushEffectiveRaise, brushWorldPos, cursorHeight } from '../../stores/editorStore'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import type { TerrainTile } from '../game-scene/terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    terrainTiles: TerrainTile[]
    heightManager: TerrainHeightManager | null
  }

  let { camera, terrainMeshes, terrainTiles: _terrainTiles, heightManager }: Props = $props()

  let isPainting = $state(false)
  let shiftHeld = $state(false)
  let lastPaintTime = $state(0)

  let currentBrushSize = $state(3)
  let currentBrushStrength = $state(5)
  let currentBrushRaise = $state(true)

  brushSize.subscribe((v) => (currentBrushSize = v))
  brushStrength.subscribe((v) => (currentBrushStrength = v))
  brushRaiseMode.subscribe((v) => {
    currentBrushRaise = v
    syncEffectiveRaise()
  })

  function syncEffectiveRaise() {
    brushEffectiveRaise.set(shiftHeld ? !currentBrushRaise : currentBrushRaise)
  }

  const raycaster = new THREE.Raycaster()
  const mouseNDC = new THREE.Vector2()

  let lastWorldPos = { x: 0, z: 0 }

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
  }

  function applyBrushAtCursor() {
    if (!heightManager) return

    const now = performance.now()
    const deltaTimeSec = lastPaintTime > 0 ? Math.min((now - lastPaintTime) / 1000, 0.1) : 1 / 60
    lastPaintTime = now

    const raise = shiftHeld ? !currentBrushRaise : currentBrushRaise
    heightManager.applyBrush(
      lastWorldPos.x,
      lastWorldPos.z,
      currentBrushSize,
      currentBrushStrength,
      raise,
      deltaTimeSec
    )
  }

  function handleMouseMove(event: MouseEvent) {
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
    if (event.button !== 0) return
    const hit = raycastTerrain(event)
    if (!hit) return

    isPainting = true
    lastPaintTime = 0
    updateCursorFromHit(hit)
    applyBrushAtCursor()
  }

  function handleMouseUp(event: MouseEvent) {
    if (event.button !== 0) return
    isPainting = false
    lastPaintTime = 0
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      shiftHeld = true
      syncEffectiveRaise()
    }
  }

  function handleKeyUp(event: KeyboardEvent) {
    if (event.key === 'Shift') {
      shiftHeld = false
      syncEffectiveRaise()
    }
  }

  function handleMouseOut() {
    hoveredCell.set(null)
    cursorHeight.set(null)
    brushWorldPos.set(null)
    isPainting = false
    lastPaintTime = 0
  }

  onMount(() => {
    const canvas = document.querySelector('canvas')
    if (!canvas) return

    canvas.addEventListener('mousemove', handleMouseMove)
    canvas.addEventListener('mousedown', handleMouseDown)
    canvas.addEventListener('mouseup', handleMouseUp)
    canvas.addEventListener('mouseleave', handleMouseOut)
    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('keyup', handleKeyUp)

    return () => {
      canvas.removeEventListener('mousemove', handleMouseMove)
      canvas.removeEventListener('mousedown', handleMouseDown)
      canvas.removeEventListener('mouseup', handleMouseUp)
      canvas.removeEventListener('mouseleave', handleMouseOut)
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('keyup', handleKeyUp)
      hoveredCell.set(null)
      brushWorldPos.set(null)
    }
  })
</script>
