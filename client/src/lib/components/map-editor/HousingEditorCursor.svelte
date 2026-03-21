<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { get } from 'svelte/store'
  import {
    selectedRoomTemplate,
    placementRotation,
    placementPreview,
    placementFloorLevel,
    placementRoomType,
    wallTextureIndex,
    floorTextureIndex,
    roofTextureIndex,
    housingEditorTool,
    selectedHouseId,
    selectedRoomIndex,
    populateEditStoresFromRoom,
    wallVariants,
    type RoomTemplate,
    type WallVariants,
    type HousingEditorTool,
  } from '../../stores/housingEditorStore'
  import type {
    HouseData,
    RoomData,
    RoomType,
    WallConfig,
    WallVariant,
  } from '../../types/housing'
  import { housingManager } from '../../managers/housingManager'
  import { buildHouseGroup, disposeHouseGroup, DEFAULT_WALL_HEIGHT, floorYBase } from '../../utils/house-geometry'
  import { editorPanOffset } from '../../stores/editorStore'
  import { ORTHOGRAPHIC_FRUSTUM_HEIGHT } from '../game-scene/camera-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'
  import { removeGrassInRect } from '../../utils/grass-data'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    heightManager: TerrainHeightManager | null
    grassDataManager: TerrainGrassDataManager | null
  }

  let { camera, terrainMeshes, heightManager, grassDataManager }: Props =
    $props()

  const { renderer } = useThrelte()
  const canvas = renderer.domElement

  const raycaster = new THREE.Raycaster()
  const mouseNDC = new THREE.Vector2()
  const previewGroup = new THREE.Group()
  previewGroup.name = 'housingPreview'

  // Preview materials: green = valid, red = invalid
  const previewMatValid = new THREE.MeshBasicMaterial({
    color: 0x44cc44,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0.4,
    depthWrite: false,
  })
  const previewMatInvalid = new THREE.MeshBasicMaterial({
    color: 0xcc4444,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0.4,
    depthWrite: false,
  })

  let currentTemplate = $state<RoomTemplate | null>(null)
  let currentRotation = $state(0)
  let currentTool = $state<HousingEditorTool>('place')
  let currentWallVariants = $state<WallVariants>({
    north: 'solid',
    south: 'door',
    east: 'solid',
    west: 'solid',
  })
  let currentFloorLevel = $state(0)
  let currentRoomType = $state<RoomType>('normal')
  let previewPos = $state<{ x: number; z: number } | null>(null)
  let previewMesh: THREE.Group | null = null
  let placementValid = false

  // Highlight outline for selected room (blue) and delete preview (red)
  const highlightEdgeMat = new THREE.LineBasicMaterial({ color: 0x44aaff })
  const deleteEdgeMat = new THREE.LineBasicMaterial({ color: 0xff4444 })
  let highlightEdges: THREE.LineSegments | null = null
  let deleteHighlight: THREE.LineSegments | null = null
  let deleteTarget: { houseId: string; roomIndex: number } | null = null

  const BLEND_RADIUS = 4

  // Middle-button camera panning
  let isPanning = false
  let lastPanX = 0
  let lastPanY = 0
  const _panRight = new THREE.Vector3()
  const _panUp = new THREE.Vector3()
  const _panFwd = new THREE.Vector3()

  function getRotatedSize() {
    if (!currentTemplate) return { sx: 0, sz: 0 }
    const rotated = currentRotation === 90 || currentRotation === 270
    return {
      sx: rotated ? currentTemplate.sizeZ : currentTemplate.sizeX,
      sz: rotated ? currentTemplate.sizeX : currentTemplate.sizeZ,
    }
  }

  let rebuildScheduled = false
  function scheduleRebuildPreview() {
    if (rebuildScheduled) return
    rebuildScheduled = true
    queueMicrotask(() => {
      rebuildScheduled = false
      rebuildPreview()
    })
  }

  let highlightScheduled = false
  function scheduleUpdateHighlight() {
    if (highlightScheduled) return
    highlightScheduled = true
    queueMicrotask(() => {
      highlightScheduled = false
      updateHighlight()
    })
  }

  function clearHighlight() {
    if (highlightEdges) {
      previewGroup.remove(highlightEdges)
      highlightEdges.geometry.dispose()
      highlightEdges = null
    }
  }

  function clearDeleteHighlight() {
    if (deleteHighlight) {
      previewGroup.remove(deleteHighlight)
      deleteHighlight.geometry.dispose()
      deleteHighlight = null
    }
    deleteTarget = null
    deleteResults = []
    deleteResultIdx = 0
  }

  let deleteResults: { house: HouseData; roomIndex: number }[] = []
  let deleteResultIdx = 0

  function updateDeleteHighlight(wx: number, wz: number, groundY: number) {
    clearDeleteHighlight()
    deleteResults = collectRoomsAtXZ(wx, wz, groundY)
    deleteResultIdx = 0
    showDeleteHighlightForResult()
  }

  function showDeleteHighlightForResult() {
    if (deleteHighlight) {
      previewGroup.remove(deleteHighlight)
      deleteHighlight.geometry.dispose()
      deleteHighlight = null
    }
    deleteTarget = null
    if (deleteResults.length === 0) return

    const result = deleteResults[deleteResultIdx % deleteResults.length]
    const room = result.house.rooms[result.roomIndex]
    const geo = new THREE.BoxGeometry(room.sizeX, room.wallHeight, room.sizeZ)
    const edgesGeo = new THREE.EdgesGeometry(geo)
    geo.dispose()
    deleteHighlight = new THREE.LineSegments(edgesGeo, deleteEdgeMat)
    deleteHighlight.position.set(
      result.house.origin.x + room.localX + room.sizeX / 2,
      result.house.origin.y + floorYBase(room.floorLevel, room.wallHeight) + room.wallHeight / 2,
      result.house.origin.z + room.localZ + room.sizeZ / 2
    )
    previewGroup.add(deleteHighlight)
    deleteTarget = { houseId: result.house.id, roomIndex: result.roomIndex }
  }

  function updateHighlight() {
    clearHighlight()

    const houseId = get(selectedHouseId)
    const roomIdx = get(selectedRoomIndex)
    if (houseId == null || roomIdx == null) return

    const house = housingManager.getHouseById(houseId)
    if (!house || roomIdx >= house.rooms.length) return

    const room = house.rooms[roomIdx]
    const geo = new THREE.BoxGeometry(room.sizeX, room.wallHeight, room.sizeZ)
    const edgesGeo = new THREE.EdgesGeometry(geo)
    geo.dispose()
    highlightEdges = new THREE.LineSegments(edgesGeo, highlightEdgeMat)
    highlightEdges.position.set(
      house.origin.x + room.localX + room.sizeX / 2,
      house.origin.y + floorYBase(room.floorLevel, room.wallHeight) + room.wallHeight / 2,
      house.origin.z + room.localZ + room.sizeZ / 2
    )
    previewGroup.add(highlightEdges)
  }

  const unsubs = [
    selectedRoomTemplate.subscribe((v) => {
      currentTemplate = v
      scheduleRebuildPreview()
    }),
    placementRotation.subscribe((v) => {
      currentRotation = v
      scheduleRebuildPreview()
    }),
    housingEditorTool.subscribe((v) => {
      currentTool = v
      canvas.style.cursor = v === 'delete' ? 'crosshair' : v === 'select' ? 'pointer' : ''
      if (v !== 'select') clearHighlight()
      if (v !== 'delete') clearDeleteHighlight()
      isPanning = false
    }),
    selectedHouseId.subscribe(() => scheduleUpdateHighlight()),
    selectedRoomIndex.subscribe(() => scheduleUpdateHighlight()),
    wallVariants.subscribe((v) => {
      currentWallVariants = v
      scheduleRebuildPreview()
    }),
    placementFloorLevel.subscribe((v) => {
      currentFloorLevel = v
      scheduleRebuildPreview()
    }),
    placementRoomType.subscribe((v) => {
      currentRoomType = v
      scheduleRebuildPreview()
    }),
  ]

  function raycastTerrain(event: MouseEvent): THREE.Intersection | null {
    if (!camera) return null
    const meshes = terrainMeshes.filter(
      (m): m is THREE.Mesh => m !== undefined
    )
    if (meshes.length === 0) return null

    const rect = canvas.getBoundingClientRect()
    mouseNDC.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(mouseNDC, camera)
    const intersects = raycaster.intersectObjects(meshes, false)
    return intersects.length > 0 ? intersects[0] : null
  }

  function rebuildPreview() {
    if (previewMesh) {
      previewGroup.remove(previewMesh)
      disposeHouseGroup(previewMesh)
      previewMesh = null
    }

    if (!currentTemplate) return

    const { sx, sz } = getRotatedSize()
    const room = buildRoomData(sx, sz)
    const previewHouse: HouseData = {
      id: 'preview',
      ownerId: '',
      origin: { x: 0, y: 0, z: 0 },
      rooms: [room],
    }
    const result = buildHouseGroup(previewHouse)

    // Apply preview material
    result.houseGroup.traverse((obj) => {
      if (obj instanceof THREE.Mesh) {
        obj.material = previewMatValid
      }
    })

    previewMesh = result.houseGroup
    previewGroup.add(previewMesh)
    updatePreviewTransform()
  }

  function updatePreviewTransform() {
    if (!previewMesh || !previewPos) return
    previewMesh.position.set(previewPos.x, previewMesh.position.y, previewPos.z)
  }

  function checkPlacementValid(): boolean {
    if (!currentTemplate || !previewPos) return false
    const { sx, sz } = getRotatedSize()

    if (currentRoomType === 'stairwell') {
      // Stairwells must be placed inside an existing 1F room
      return housingManager.hasFloorSupport(
        previewPos.x,
        previewPos.z,
        sx,
        sz
      )
    }

    const hasOverlap = housingManager.checkOverlap(
      previewPos.x,
      previewPos.z,
      sx,
      sz,
      currentFloorLevel
    )
    if (hasOverlap) return false
    // 2F rooms need full floor support from 1F rooms
    if (currentFloorLevel >= 1) {
      return housingManager.hasFloorSupport(
        previewPos.x,
        previewPos.z,
        sx,
        sz
      )
    }
    return true
  }

  function setPreviewMaterial(valid: boolean) {
    if (!previewMesh) return
    const mat = valid ? previewMatValid : previewMatInvalid
    previewMesh.traverse((obj) => {
      if (obj instanceof THREE.Mesh) obj.material = mat
    })
  }

  function handleMouseMove(event: MouseEvent) {
    if (isPanning) {
      if (!camera) return
      const dx = event.clientX - lastPanX
      const dy = event.clientY - lastPanY
      lastPanX = event.clientX
      lastPanY = event.clientY

      camera.matrixWorld.extractBasis(_panRight, _panUp, _panFwd)
      _panRight.y = 0
      _panRight.normalize()
      _panFwd.y = 0
      _panFwd.normalize()

      const rect = canvas.getBoundingClientRect()
      const scale = ORTHOGRAPHIC_FRUSTUM_HEIGHT / (camera.zoom * rect.height)

      const current = get(editorPanOffset)
      editorPanOffset.set({
        x: current.x - (_panRight.x * dx + _panFwd.x * dy) * scale,
        z: current.z - (_panRight.z * dx + _panFwd.z * dy) * scale,
      })
      return
    }

    const hit = raycastTerrain(event)
    if (!hit || (!currentTemplate && currentTool === 'place')) {
      placementPreview.set(null)
      previewPos = null
      if (previewMesh) previewMesh.visible = false
      if (currentTool === 'delete') clearDeleteHighlight()
      return
    }

    const x = Math.floor(hit.point.x)
    const z = Math.floor(hit.point.z)
    const posChanged = !previewPos || previewPos.x !== x || previewPos.z !== z
    previewPos = { x, z }
    if (posChanged) placementPreview.set({ x, z })

    if (currentTool === 'place' && previewMesh) {
      previewMesh.visible = true
      previewMesh.position.set(x, hit.point.y, z)
      if (posChanged) {
        const wasValid = placementValid
        placementValid = checkPlacementValid()
        if (placementValid !== wasValid) setPreviewMaterial(placementValid)
      }
    } else if (previewMesh) {
      previewMesh.visible = false
    }

    if (currentTool === 'delete' && posChanged) {
      updateDeleteHighlight(hit.point.x, hit.point.z, hit.point.y)
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

    if (currentTool === 'delete') {
      deleteRoomAtCursor()
      return
    }

    if (currentTool === 'select') {
      selectRoomAtCursor(event)
      return
    }

    if (!currentTemplate || !previewPos || !placementValid) return
    placeHouse()
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'r' || event.key === 'R') {
      placementRotation.set((currentRotation + 90) % 360)
    }
  }

  /** Collect all unique rooms at world XZ across both floor levels. */
  function collectRoomsAtXZ(
    wx: number,
    wz: number,
    groundY: number
  ): { house: HouseData; roomIndex: number }[] {
    const results: { house: HouseData; roomIndex: number }[] = []
    const seen = new SvelteSet<string>()
    for (let fl = 1; fl >= 0; fl--) {
      const testY = groundY + floorYBase(fl, DEFAULT_WALL_HEIGHT) + 1
      for (const r of housingManager.findAllRoomsAtPoint(wx, testY, wz)) {
        const key = `${r.house.id}:${r.roomIndex}`
        if (!seen.has(key)) {
          seen.add(key)
          results.push(r)
        }
      }
    }
    return results
  }

  /** Find all rooms at cursor XZ via raycast. */
  function findAllAtCursorXZ(event: MouseEvent) {
    const hit = raycastTerrain(event)
    if (!hit) return []
    return collectRoomsAtXZ(hit.point.x, hit.point.z, hit.point.y)
  }

  let lastSelectKey = ''

  async function deleteRoomAtCursor() {
    if (!deleteTarget) {
      // No target but overlapping rooms exist — cycle to next
      if (deleteResults.length > 1) {
        deleteResultIdx = (deleteResultIdx + 1) % deleteResults.length
        showDeleteHighlightForResult()
      }
      return
    }
    const house = housingManager.getHouseById(deleteTarget.houseId)
    if (!house) return

    const deletedRoom = house.rooms[deleteTarget.roomIndex]

    if (house.rooms.length <= 1) {
      await housingManager.deleteHouse(house.id)
    } else {
      const updatedRooms = house.rooms.filter(
        (_, i) => i !== deleteTarget!.roomIndex
      )
      const updatedHouse: HouseData = { ...house, rooms: updatedRooms }
      await housingManager.updateHouse(updatedHouse)
    }
    clearDeleteHighlight()
    deleteResults = []

    // Restore terrain and grass for 1F non-stairwell rooms
    if (
      deletedRoom.floorLevel === 0 &&
      deletedRoom.roomType !== 'stairwell' &&
      heightManager
    ) {
      const roomWorldX = house.origin.x + deletedRoom.localX
      const roomWorldZ = house.origin.z + deletedRoom.localZ
      const roomMaxX = roomWorldX + deletedRoom.sizeX
      const roomMaxZ = roomWorldZ + deletedRoom.sizeZ

      // 1. Restore heightmap from original (footprint + blend radius)
      const restoreMinX = roomWorldX - BLEND_RADIUS
      const restoreMinZ = roomWorldZ - BLEND_RADIUS
      const restoreMaxX = roomMaxX + BLEND_RADIUS
      const restoreMaxZ = roomMaxZ + BLEND_RADIUS
      heightManager.restoreFromOriginal(
        restoreMinX,
        restoreMinZ,
        restoreMaxX,
        restoreMaxZ
      )

      // 2. Re-flatten for all remaining nearby 1F rooms
      const allHouses = housingManager.getAllHouses()
      for (const h of allHouses) {
        for (const room of h.rooms) {
          if (room.floorLevel !== 0 || room.roomType === 'stairwell') continue
          const rx = h.origin.x + room.localX
          const rz = h.origin.z + room.localZ
          const rmx = rx + room.sizeX
          const rmz = rz + room.sizeZ
          // Check if this room's flatten zone overlaps the restored area
          if (
            rx - BLEND_RADIUS > restoreMaxX ||
            rmx + BLEND_RADIUS < restoreMinX ||
            rz - BLEND_RADIUS > restoreMaxZ ||
            rmz + BLEND_RADIUS < restoreMinZ
          )
            continue
          // Build protected rects from other 1F rooms
          const protectedRects: {
            minX: number
            minZ: number
            maxX: number
            maxZ: number
          }[] = []
          for (const h2 of allHouses) {
            for (const r2 of h2.rooms) {
              if (r2 === room) continue
              if (r2.floorLevel !== 0 || r2.roomType === 'stairwell') continue
              const r2x = h2.origin.x + r2.localX
              const r2z = h2.origin.z + r2.localZ
              protectedRects.push({
                minX: r2x,
                minZ: r2z,
                maxX: r2x + r2.sizeX,
                maxZ: r2z + r2.sizeZ,
              })
            }
          }
          const targetHeight = h.origin.y
          heightManager.flattenArea(rx, rz, rmx, rmz, targetHeight, BLEND_RADIUS, (wx, wz) =>
            protectedRects.some(
              (r) => wx >= r.minX && wx <= r.maxX && wz >= r.minZ && wz <= r.maxZ
            )
          )
        }
      }
      heightManager.saveAllDirty()

      // 3. Restore grass from original, then re-remove for remaining houses
      if (grassDataManager) {
        const GRASS_MARGIN = 1
        const grassMinX = roomWorldX - GRASS_MARGIN
        const grassMinZ = roomWorldZ - GRASS_MARGIN
        const grassMaxX = roomMaxX + GRASS_MARGIN
        const grassMaxZ = roomMaxZ + GRASS_MARGIN

        const tileMinX = Math.floor(
          (grassMinX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMaxX = Math.floor(
          (grassMaxX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMinZ = Math.floor(
          (grassMinZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMaxZ = Math.floor(
          (grassMaxZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )

        // Restore original grass for affected tiles
        const restorePromises: Promise<boolean>[] = []
        for (let tz = tileMinZ; tz <= tileMaxZ; tz++) {
          for (let tx = tileMinX; tx <= tileMaxX; tx++) {
            restorePromises.push(grassDataManager.restoreFromOriginal(tx, tz))
          }
        }
        await Promise.all(restorePromises)

        // Re-remove grass under remaining 1F rooms
        for (const h of allHouses) {
          for (const room of h.rooms) {
            if (room.floorLevel !== 0 || room.roomType === 'stairwell') continue
            const rx = h.origin.x + room.localX
            const rz = h.origin.z + room.localZ
            const rMinX = rx - GRASS_MARGIN
            const rMinZ = rz - GRASS_MARGIN
            const rMaxX = rx + room.sizeX + GRASS_MARGIN
            const rMaxZ = rz + room.sizeZ + GRASS_MARGIN
            // Only process if overlapping the restored grass area
            if (
              rMinX > grassMaxX ||
              rMaxX < grassMinX ||
              rMinZ > grassMaxZ ||
              rMaxZ < grassMinZ
            )
              continue
            for (let tz = tileMinZ; tz <= tileMaxZ; tz++) {
              for (let tx = tileMinX; tx <= tileMaxX; tx++) {
                const cached = grassDataManager.getCachedGrassData(tx, tz)
                if (!cached) continue
                const filtered = removeGrassInRect(
                  cached,
                  rMinX,
                  rMinZ,
                  rMaxX,
                  rMaxZ
                )
                if (filtered) grassDataManager.saveGrassData(tx, tz, filtered)
              }
            }
          }
        }
      }
    }
  }

  function selectRoomAtCursor(event: MouseEvent) {
    const results = findAllAtCursorXZ(event)
    if (results.length === 0) {
      selectedHouseId.set(null)
      selectedRoomIndex.set(null)
      lastSelectKey = ''
      return
    }

    // Cycle through overlapping rooms on repeated clicks
    let idx = 0
    if (results.length > 1) {
      const currentIdx = results.findIndex(
        (r) => `${r.house.id}:${r.roomIndex}` === lastSelectKey
      )
      if (currentIdx >= 0) {
        idx = (currentIdx + 1) % results.length
      }
    }

    const result = results[idx]
    lastSelectKey = `${result.house.id}:${result.roomIndex}`
    selectedHouseId.set(result.house.id)
    selectedRoomIndex.set(result.roomIndex)
    populateEditStoresFromRoom(result.house.rooms[result.roomIndex])
  }

  async function placeHouse() {
    if (!currentTemplate || !previewPos || !heightManager) return

    const pos = { ...previewPos }
    const { sx, sz } = getRotatedSize()
    const centerX = pos.x + sx / 2
    const centerZ = pos.z + sz / 2
    const targetHeight = heightManager.getHeightAtWorldPosition(centerX, centerZ)

    const newRoom = buildRoomData(sx, sz)
    const shouldFlattenTerrain = currentFloorLevel === 0 && currentRoomType !== 'stairwell'

    // Build protected rects BEFORE saving (so the new room isn't included)
    const protectedRects: { minX: number; minZ: number; maxX: number; maxZ: number }[] = []
    if (shouldFlattenTerrain) {
      for (const house of housingManager.getAllHouses()) {
        for (const room of house.rooms) {
          if (room.floorLevel === 0 && room.roomType !== 'stairwell') {
            const rx = house.origin.x + room.localX
            const rz = house.origin.z + room.localZ
            protectedRects.push({
              minX: rx,
              minZ: rz,
              maxX: rx + room.sizeX,
              maxZ: rz + room.sizeZ,
            })
          }
        }
      }
    }

    // Stairwells and 2F rooms attach to the house with supporting 1F rooms
    // 1F rooms check edge adjacency
    let targetHouse: HouseData | null
    if (currentRoomType === 'stairwell' || currentFloorLevel >= 1) {
      targetHouse = housingManager.findSupportingHouse(pos.x, pos.z, sx, sz)
    } else {
      targetHouse = housingManager.findAdjacentHouse(pos.x, pos.z, sx, sz)
    }

    let saved: HouseData | null
    if (targetHouse) {
      // Add room to existing house — localX/Z relative to house origin
      newRoom.localX = pos.x - targetHouse.origin.x
      newRoom.localZ = pos.z - targetHouse.origin.z

      const updatedRooms = [...targetHouse.rooms, newRoom]
      // Only set shared walls open for same-floor rooms
      setSharedWallsOpen(updatedRooms)

      const updatedHouse: HouseData = {
        ...targetHouse,
        rooms: updatedRooms,
      }
      saved = await housingManager.updateHouse(updatedHouse)
    } else {
      // 2F rooms and stairwells must attach to an existing house
      if (currentRoomType === 'stairwell' || currentFloorLevel >= 1) return

      const houseData: HouseData = {
        id: '',
        ownerId: 'local',
        origin: { x: pos.x, y: targetHeight, z: pos.z },
        rooms: [newRoom],
      }
      saved = await housingManager.saveHouse(houseData)
    }

    if (!saved) return

    // Skip terrain flatten and grass removal for 2F rooms and stairwells
    if (shouldFlattenTerrain) {
      heightManager.flattenArea(
        pos.x,
        pos.z,
        pos.x + sx,
        pos.z + sz,
        targetHeight,
        BLEND_RADIUS,
        (wx, wz) =>
          protectedRects.some(
            (r) => wx >= r.minX && wx <= r.maxX && wz >= r.minZ && wz <= r.maxZ
          )
      )
      heightManager.saveAllDirty()

      // Remove grass under the house footprint (+ 1m margin)
      if (grassDataManager) {
        const GRASS_MARGIN = 1
        const rectMinX = pos.x - GRASS_MARGIN
        const rectMinZ = pos.z - GRASS_MARGIN
        const rectMaxX = pos.x + sx + GRASS_MARGIN
        const rectMaxZ = pos.z + sz + GRASS_MARGIN

        const tileMinX = Math.floor(
          (rectMinX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMaxX = Math.floor(
          (rectMaxX + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMinZ = Math.floor(
          (rectMinZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )
        const tileMaxZ = Math.floor(
          (rectMaxZ + TERRAIN_TILE_SIZE / 2) / TERRAIN_TILE_SIZE
        )

        for (let tz = tileMinZ; tz <= tileMaxZ; tz++) {
          for (let tx = tileMinX; tx <= tileMaxX; tx++) {
            grassDataManager.ensureOriginalGrass(tx, tz)
            const cached = grassDataManager.getCachedGrassData(tx, tz)
            if (!cached) continue
            const filtered = removeGrassInRect(
              cached,
              rectMinX,
              rectMinZ,
              rectMaxX,
              rectMaxZ
            )
            if (filtered) grassDataManager.saveGrassData(tx, tz, filtered)
          }
        }
      }
    }
  }

  function fillWall(count: number, variant: WallVariant, texture: number): WallConfig[] {
    const base: WallVariant = variant === 'door' || variant === 'window' ? 'solid' : variant
    const segs: WallConfig[] = Array.from({ length: count }, () => ({ variant: base, texture }))
    if (variant === 'door' || variant === 'window') {
      segs[Math.floor(count / 2)] = { variant, texture }
    }
    return segs
  }

  function buildRoomData(sizeX: number, sizeZ: number): RoomData {
    const wallTex = get(wallTextureIndex)
    const floorTex = get(floorTextureIndex)
    const roofTex = get(roofTextureIndex)
    const wv = currentWallVariants

    return {
      roomType: currentRoomType,
      localX: 0,
      localZ: 0,
      sizeX,
      sizeZ,
      floorLevel: currentRoomType === 'stairwell' ? 0 : currentFloorLevel,
      floorTexture: floorTex,
      roofTexture: roofTex,
      wallHeight: DEFAULT_WALL_HEIGHT,
      wallNorth: fillWall(sizeX, wv.north, wallTex),
      wallSouth: fillWall(sizeX, wv.south, wallTex),
      wallEast: fillWall(sizeZ, wv.east, wallTex),
      wallWest: fillWall(sizeZ, wv.west, wallTex),
    }
  }

  /**
   * Auto-set overlapping 1m wall segments to 'open' where two rooms touch.
   * e.g. 6x4 + 3x3 on its south wall: 3 of the 6 south segments → open,
   *      and all 3 of the 3x3's north segments → open.
   */
  function setSharedWallsOpen(rooms: RoomData[]) {
    for (let i = 0; i < rooms.length; i++) {
      const a = rooms[i]
      for (let j = i + 1; j < rooms.length; j++) {
        const b = rooms[j]
        // Only open walls between rooms on the same floor
        if (a.floorLevel !== b.floorLevel) continue

        // N/S: a's south touches b's north
        if (a.localZ + a.sizeZ === b.localZ) {
          openOverlappingSegments(a, 'wallSouth', b, 'wallNorth', 'x')
        }
        // N/S: b's south touches a's north
        if (b.localZ + b.sizeZ === a.localZ) {
          openOverlappingSegments(b, 'wallSouth', a, 'wallNorth', 'x')
        }
        // E/W: a's east touches b's west
        if (a.localX + a.sizeX === b.localX) {
          openOverlappingSegments(a, 'wallEast', b, 'wallWest', 'z')
        }
        // E/W: b's east touches a's west
        if (b.localX + b.sizeX === a.localX) {
          openOverlappingSegments(b, 'wallEast', a, 'wallWest', 'z')
        }
      }
    }
  }

  type WallKey = 'wallNorth' | 'wallSouth' | 'wallEast' | 'wallWest'

  /** Set overlapping 1m segments to open on both rooms' touching walls. */
  function openOverlappingSegments(
    a: RoomData,
    aWall: WallKey,
    b: RoomData,
    bWall: WallKey,
    axis: 'x' | 'z'
  ) {
    const aStart = axis === 'x' ? a.localX : a.localZ
    const aLen = axis === 'x' ? a.sizeX : a.sizeZ
    const bStart = axis === 'x' ? b.localX : b.localZ
    const bLen = axis === 'x' ? b.sizeX : b.sizeZ

    const overlapStart = Math.max(aStart, bStart)
    const overlapEnd = Math.min(aStart + aLen, bStart + bLen)

    for (let pos = overlapStart; pos < overlapEnd; pos++) {
      const aIdx = pos - aStart
      const bIdx = pos - bStart
      a[aWall][aIdx] = { variant: 'open', texture: a[aWall][aIdx].texture }
      b[bWall][bIdx] = { variant: 'open', texture: b[bWall][bIdx].texture }
    }
  }

  function handleMouseUp(event: MouseEvent) {
    if (event.button === 1) {
      isPanning = false
    }
  }

  function handleWheel(event: WheelEvent) {
    if (!camera) return
    event.preventDefault()
    const factor = event.deltaY > 0 ? 0.95 : 1 / 0.95
    camera.zoom = Math.max(0.15, Math.min(2, camera.zoom * factor))
    camera.updateProjectionMatrix()
  }

  canvas.addEventListener('mousemove', handleMouseMove)
  canvas.addEventListener('mousedown', handleMouseDown)
  canvas.addEventListener('mouseup', handleMouseUp)
  canvas.addEventListener('wheel', handleWheel, { passive: false })
  window.addEventListener('keydown', handleKeyDown)

  onDestroy(() => {
    unsubs.forEach((u) => u())
    canvas.removeEventListener('mousemove', handleMouseMove)
    canvas.removeEventListener('mousedown', handleMouseDown)
    canvas.removeEventListener('mouseup', handleMouseUp)
    canvas.removeEventListener('wheel', handleWheel)
    window.removeEventListener('keydown', handleKeyDown)
    canvas.style.cursor = ''
    placementPreview.set(null)
    previewMatValid.dispose()
    previewMatInvalid.dispose()
    highlightEdgeMat.dispose()
    deleteEdgeMat.dispose()
    clearHighlight()
    clearDeleteHighlight()

    if (previewMesh) {
      previewGroup.remove(previewMesh)
      disposeHouseGroup(previewMesh)
      previewMesh = null
    }
  })
</script>

<T is={previewGroup} />
