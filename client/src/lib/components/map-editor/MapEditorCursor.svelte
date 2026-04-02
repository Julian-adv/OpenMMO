<script lang="ts">
  import * as THREE from 'three'
  import { onMount } from 'svelte'
  import { hoveredCell, brushSize, brushStrength, brushRaiseMode, brushMode, brushWorldPos, cursorHeight, editorTool, splatLayer, editorPanOffset, currentRegionLayers, textureNameToLabel, currentEditorRegion, currentRegionConfigs, editorMetaManager, editorHeightManager, editorSplatManager, zoneDrawStart, zoneSubTool, editorZoneManager, currentZoneData, spawnFormMonsterType, spawnFormMaxPerPlayer, spawnFormMaxTotal, spawnFormIntervalSecs, noSpawnFormLabel, npcNames, selectedNpc, selectedNpcSchedule, selectedScheduleIndex, draggingWaypointIndex, selectedFurnitureType, furnitureRotation, currentFurnitureData, selectedFurniturePlacementId, furniturePreviewPos, furnitureSubTool } from '../../stores/editorStore'
  import type { EditorTool, ZoneSubTool, FurnitureSubTool, FurnitureRegionData } from '../../stores/editorStore'
  import { NpcScheduleManager } from '../../managers/npcScheduleManager'
  import type { NpcScheduleData } from '../../managers/npcScheduleManager'
  import { furnitureManager } from '../../managers/furnitureManager'
  import { playerFloorLevel } from '../../stores/housingStore'
  import { floorYBase, DEFAULT_WALL_HEIGHT } from '../../utils/house-geo-utils'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { ORTHOGRAPHIC_FRUSTUM_HEIGHT } from '../game-scene/camera-utils'
  import { get } from 'svelte/store'
  import type { TerrainTile } from '../game-scene/terrain-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainSplatManager } from '../../managers/terrainSplatManager'
  import type { TerrainMetaManager } from '../../managers/terrainMetaManager'
  import { tileToRegion } from '../../managers/terrainMetaManager'
  import { gameStore } from '../../stores/gameStore'
  import { remotePlayerManager } from '../../managers/remotePlayerManager'

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
  let currentZoneSubTool = $state<ZoneSubTool>('noSpawn')
  let currentZoneDrawStart = $state<{ x: number; z: number } | null>(null)

  // NPC editor state
  let currentNpcNames = $state<string[]>([])
  let currentNpcSchedule = $state<NpcScheduleData | null>(null)
  let currentSchedIdx = $state(0)
  let currentDragWaypoint = $state<number | null>(null)
  let npcManager: NpcScheduleManager | null = null

  // Furniture editor state
  let currentFurnitureType = $state<string | null>(null)
  let currentFurnitureRot = $state(0)
  let currentFurnitureSubTool = $state<FurnitureSubTool>('place')
  let currentPlayerFloor = $state(-1)

  brushSize.subscribe((v) => (currentBrushSize = v))
  brushStrength.subscribe((v) => (currentBrushStrength = v))
  brushRaiseMode.subscribe((v) => {
    currentBrushRaise = v
    syncBrushMode()
  })
  editorTool.subscribe((v) => {
    currentTool = v
    // Clear zone draw state when switching away from zone tool
    if (v !== 'zone') {
      zoneDrawStart.set(null)
    }
  })
  splatLayer.subscribe((v) => (currentSplatLayer = v))
  zoneSubTool.subscribe((v) => (currentZoneSubTool = v))
  zoneDrawStart.subscribe((v) => (currentZoneDrawStart = v))
  npcNames.subscribe((v) => (currentNpcNames = v))
  selectedNpcSchedule.subscribe((v) => (currentNpcSchedule = v))
  selectedScheduleIndex.subscribe((v) => (currentSchedIdx = v))
  draggingWaypointIndex.subscribe((v) => (currentDragWaypoint = v))
  selectedFurnitureType.subscribe((v) => (currentFurnitureType = v))
  furnitureRotation.subscribe((v) => (currentFurnitureRot = v))
  furnitureSubTool.subscribe((v) => (currentFurnitureSubTool = v))
  playerFloorLevel.subscribe((v) => (currentPlayerFloor = v))

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
    // Only show brush overlay for height/splat tools
    if (currentTool === 'height' || currentTool === 'splat') {
      brushWorldPos.set({ x: hit.point.x, z: hit.point.z })
    } else {
      brushWorldPos.set(null)
    }

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
        // Fetch furniture data for the new region
        loadFurnitureForRegion(rx, rz)
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

    if (currentTool === 'furniture' && currentFurnitureSubTool === 'place' && currentFurnitureType) {
      const terrainY = heightManager
        ? heightManager.getHeightAtWorldPosition(hit.point.x, hit.point.z)
        : 0
      const floor = Math.max(0, currentPlayerFloor)
      const y = terrainY + floorYBase(floor, DEFAULT_WALL_HEIGHT)
      furniturePreviewPos.set({ x: hit.point.x, y, z: hit.point.z })
    } else if (currentTool !== 'furniture') {
      furniturePreviewPos.set(null)
    }

    if (currentTool === 'npc' && currentDragWaypoint !== null) {
      handleNpcDrag(hit.point.x, hit.point.z)
      return
    }

    if (isPainting) {
      applyBrushAtCursor()
    }
  }

  // --- NPC waypoint interaction ---

  function findClosestWaypoint(worldX: number, worldZ: number): number | null {
    if (!currentNpcSchedule) return null
    const entry = currentNpcSchedule.schedule[currentSchedIdx]
    if (!entry) return null

    const threshold = 4
    let bestDist = threshold * threshold
    let bestIdx: number | null = null

    // Check home position (index -1)
    const hdx = worldX - entry.pos[0]
    const hdz = worldZ - entry.pos[2]
    const homeDist = hdx * hdx + hdz * hdz
    if (homeDist < bestDist) {
      bestDist = homeDist
      bestIdx = -1
    }

    // Check waypoints
    const waypoints = entry.waypoints
    for (let i = 0; i < waypoints.length; i++) {
      const wp = waypoints[i]
      const dx = worldX - wp[0]
      const dz = worldZ - wp[2]
      const dist = dx * dx + dz * dz
      if (dist < bestDist) {
        bestDist = dist
        bestIdx = i
      }
    }

    return bestIdx
  }

  function findClosestNpc(worldX: number, worldZ: number, threshold: number): string | null {
    const state = get(gameStore)
    const names = currentNpcNames
    if (names.length === 0) return null

    let bestDist = threshold * threshold
    let bestName: string | null = null

    for (const [id, player] of state.otherPlayers) {
      const nameLower = player.name.toLowerCase()
      if (!names.includes(nameLower)) continue
      const rp = remotePlayerManager.players.get(id)
      if (!rp) continue
      const dx = worldX - rp.position.x
      const dz = worldZ - rp.position.z
      const dist = dx * dx + dz * dz
      if (dist < bestDist) {
        bestDist = dist
        bestName = nameLower
      }
    }
    return bestName
  }

  async function loadNpcSchedule(name: string) {
    selectedNpcSchedule.set(null)
    selectedScheduleIndex.set(0)
    selectedNpc.set(name)
    try {
      if (!npcManager) npcManager = new NpcScheduleManager()
      const data = await npcManager.fetchSchedule(name)
      if (get(selectedNpc) === name) {
        selectedNpcSchedule.set(data)
      }
    } catch (e) {
      console.error(`[NPC] Failed to fetch schedule for '${name}':`, e)
    }
  }

  function handleNpcMouseDown(worldX: number, worldZ: number) {
    // NPC selection takes priority over waypoint dragging (tight 3-unit radius)
    const clickedNpc = findClosestNpc(worldX, worldZ, 3)
    if (clickedNpc) {
      loadNpcSchedule(clickedNpc)
      return
    }
    // Then try to grab a waypoint
    const wpIdx = findClosestWaypoint(worldX, worldZ)
    if (wpIdx !== null) {
      draggingWaypointIndex.set(wpIdx)
      return
    }
    // Fall back to wider NPC search (5-unit radius)
    const nearbyNpc = findClosestNpc(worldX, worldZ, 5)
    if (nearbyNpc) loadNpcSchedule(nearbyNpc)
  }

  function handleNpcDrag(worldX: number, worldZ: number) {
    if (currentDragWaypoint === null || !currentNpcSchedule) return
    const entry = currentNpcSchedule.schedule[currentSchedIdx]
    if (!entry) return

    const y = heightManager
      ? heightManager.getHeightAtWorldPosition(worldX, worldZ)
      : 0

    // Clone schedule to trigger Svelte reactivity (new object reference)
    const updated: NpcScheduleData = {
      schedule: currentNpcSchedule.schedule.map((s, i) => {
        if (i !== currentSchedIdx) return s
        const newEntry = { ...s }
        if (currentDragWaypoint === -1) {
          newEntry.pos = [worldX, y, worldZ]
        } else {
          newEntry.waypoints = [...(s.waypoints)]
          newEntry.waypoints[currentDragWaypoint!] = [worldX, y, worldZ]
        }
        return newEntry
      }),
    }
    selectedNpcSchedule.set(updated)
  }

  // --- Furniture interaction ---

  async function loadFurnitureForRegion(rx: number, rz: number) {

    const data = await furnitureManager.fetchFurniture(rx, rz)
    currentFurnitureData.set(data)
    selectedFurniturePlacementId.set(null)
  }

  async function handleFurnitureMouseDown(worldX: number, worldZ: number) {
    if (currentFurnitureSubTool === 'place') {
      if (!currentFurnitureType) return
      const terrainY = heightManager
        ? heightManager.getHeightAtWorldPosition(worldX, worldZ)
        : 0
      const floor = Math.max(0, currentPlayerFloor)
      const y = terrainY + floorYBase(floor, DEFAULT_WALL_HEIGHT)
      const data = get(currentFurnitureData)
      const maxId = data.placements.reduce((max, p) => Math.max(max, p.id), 0)
      const placement = {
        id: maxId + 1,
        type: currentFurnitureType,
        x: worldX,
        y,
        z: worldZ,
        rotation: currentFurnitureRot,
        floorLevel: floor,
      }
      const updated: FurnitureRegionData = {
        placements: [...data.placements, placement],
      }
      currentFurnitureData.set(updated)

      // Auto-save
      const region = get(currentEditorRegion)
      if (region) {
    
        await furnitureManager.saveFurniture(region.rx, region.rz, updated)
      }
    } else {
      // Select mode: find closest placement
      const data = get(currentFurnitureData)
      const threshold = 4
      let bestDist = threshold * threshold
      let bestId: number | null = null
      for (const p of data.placements) {
        const dx = worldX - p.x
        const dz = worldZ - p.z
        const dist = dx * dx + dz * dz
        if (dist < bestDist) {
          bestDist = dist
          bestId = p.id
        }
      }
      selectedFurniturePlacementId.set(bestId)
    }
  }

  async function handleZoneClick(worldX: number, worldZ: number) {
    if (currentZoneDrawStart) {
      // Second click: finish the rectangle
      const minX = Math.min(currentZoneDrawStart.x, worldX)
      const minZ = Math.min(currentZoneDrawStart.z, worldZ)
      const maxX = Math.max(currentZoneDrawStart.x, worldX)
      const maxZ = Math.max(currentZoneDrawStart.z, worldZ)

      const mgr = get(editorZoneManager)
      const region = get(currentEditorRegion)
      if (mgr && region) {
        const zoneData = get(currentZoneData)
        let updated
        if (currentZoneSubTool === 'noSpawn') {
          const label = get(noSpawnFormLabel).trim()
          const zone = { minX, minZ, maxX, maxZ, ...(label ? { label } : {}) }
          const zones = [...(zoneData.noSpawnZones ?? []), zone]
          updated = { ...zoneData, noSpawnZones: zones }
        } else {
          const spawns = [...(zoneData.monsterSpawns ?? []), {
            monsterType: get(spawnFormMonsterType),
            maxPerPlayer: get(spawnFormMaxPerPlayer),
            maxTotal: get(spawnFormMaxTotal),
            spawnIntervalSecs: get(spawnFormIntervalSecs),
            minX, minZ, maxX, maxZ,
          }]
          updated = { ...zoneData, monsterSpawns: spawns }
        }
        await mgr.saveZone(region.rx, region.rz, updated)
        currentZoneData.set(updated)
        if (currentZoneSubTool === 'noSpawn') noSpawnFormLabel.set('')
      }

      zoneDrawStart.set(null)
    } else {
      // First click: store the start corner
      zoneDrawStart.set({ x: worldX, z: worldZ })
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

    if (currentTool === 'zone') {
      updateCursorFromHit(hit)
      handleZoneClick(hit.point.x, hit.point.z)
      return
    }

    if (currentTool === 'npc') {
      updateCursorFromHit(hit)
      handleNpcMouseDown(hit.point.x, hit.point.z)
      return
    }

    if (currentTool === 'furniture') {
      updateCursorFromHit(hit)
      handleFurnitureMouseDown(hit.point.x, hit.point.z)
      return
    }

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
    if (currentDragWaypoint !== null) {
      draggingWaypointIndex.set(null)
    }
    isPainting = false
    lastPaintTime = 0
  }

  async function handleFurnitureDelete() {
    const placementId = get(selectedFurniturePlacementId)
    if (placementId === null) return
    const data = get(currentFurnitureData)
    const updated: FurnitureRegionData = {
      placements: data.placements.filter((p) => p.id !== placementId),
    }
    currentFurnitureData.set(updated)
    selectedFurniturePlacementId.set(null)

    const region = get(currentEditorRegion)
    if (region) {
      await furnitureManager.saveFurniture(region.rx, region.rz, updated)
    }
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
    if (currentTool === 'furniture') {
      if (event.key === 'r' || event.key === 'R') {
        furnitureRotation.update((r) => (r + 90) % 360)
      }
      if (event.key === 'Delete' || event.key === 'Backspace') {
        handleFurnitureDelete()
      }
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
    if (heightManager) editorHeightManager.set(heightManager)
    if (splatManager) editorSplatManager.set(splatManager)

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
