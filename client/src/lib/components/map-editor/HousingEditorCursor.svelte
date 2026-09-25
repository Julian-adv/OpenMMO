<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import { cameraRotationEnabled } from '../../stores/debugStore'
  import {
    selectedRoomTemplate,
    placementRotation,
    placementPreview,
    placementFloorLevel,
    placementRoomType,
    wallTextureIndex,
    floorTextureIndex,
    roofTextureIndex,
    placementRoofType,
    housingEditorTool,
    selectedHouseId,
    selectedRoomIndex,
    setDeleteSelectedRoom,
    setFlattenSelectedRoomTerrain,
    setReinstallSelectedHouse,
    setMoveSelectedHouse,
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
  import { objectManager } from '../../managers/objectManager'
  import {
    buildHouseGroup,
    disposeHouseGroup,
    DEFAULT_WALL_HEIGHT,
    FLOOR_THICKNESS,
    floorOverhang,
    floorYBase,
  } from '../../utils/house-geometry'
  import { editorPanOffset } from '../../stores/editorStore'
  import { ORTHOGRAPHIC_FRUSTUM_HEIGHT } from '../game-scene/camera-utils'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { TerrainGrassDataManager } from '../../managers/terrainGrassDataManager'
  import { worldRectToTileBounds } from '../game-scene/terrain-utils'
  import { isTypingTarget } from '../../utils/dom'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    heightManager: TerrainHeightManager | null
    grassDataManager: TerrainGrassDataManager | null
    housingGroup: THREE.Group | null
  }

  let {
    camera,
    terrainMeshes,
    heightManager,
    grassDataManager,
    housingGroup,
  }: Props = $props()

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

  // depthTest off + high renderOrder so the outline isn't occluded by corner pillars
  const highlightEdgeMat = new THREE.LineBasicMaterial({
    color: 0x44aaff,
    depthTest: false,
  })
  let highlightEdges: THREE.LineSegments | null = null

  const BLEND_RADIUS = 4
  const GRASS_MARGIN = 1

  type Rect = { minX: number; minZ: number; maxX: number; maxZ: number }

  /** Collect world-space footprint rects for all ground-floor (1F, non-stairwell) rooms,
   *  optionally excluding any (house, room) pairs matched by `exclude`. */
  function buildGroundFloorRects(
    exclude?: (house: HouseData, room: RoomData) => boolean
  ): Rect[] {
    const rects: Rect[] = []
    for (const h of housingManager.getAllHouses()) {
      for (const r of groundFloorRooms(h)) {
        if (exclude?.(h, r)) continue
        const rx = h.origin.x + r.localX
        const rz = h.origin.z + r.localZ
        rects.push({
          minX: rx,
          minZ: rz,
          maxX: rx + r.sizeX,
          maxZ: rz + r.sizeZ,
        })
      }
    }
    return rects
  }

  function isInAnyRect(rects: Rect[], wx: number, wz: number): boolean {
    return rects.some((rect) => isInRect(rect, wx, wz))
  }

  function isInRect(rect: Rect, wx: number, wz: number): boolean {
    return (
      wx >= rect.minX && wx <= rect.maxX && wz >= rect.minZ && wz <= rect.maxZ
    )
  }

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

  function updateHighlight() {
    clearHighlight()

    const houseId = get(selectedHouseId)
    const roomIdx = get(selectedRoomIndex)
    if (houseId == null || roomIdx == null) return

    const house = housingManager.getHouseById(houseId)
    if (!house || roomIdx >= house.rooms.length) return

    const room = house.rooms[roomIdx]
    const overhang = floorOverhang(room.floorLevel)
    const highlightW = room.sizeX + overhang * 2
    const highlightD = room.sizeZ + overhang * 2
    const geo = new THREE.BoxGeometry(highlightW, room.wallHeight, highlightD)
    const edgesGeo = new THREE.EdgesGeometry(geo)
    geo.dispose()
    highlightEdges = new THREE.LineSegments(edgesGeo, highlightEdgeMat)
    highlightEdges.renderOrder = 999
    const yBase =
      floorYBase(room.floorLevel, room.wallHeight) + FLOOR_THICKNESS / 2
    highlightEdges.position.set(
      house.origin.x + room.localX + room.sizeX / 2,
      house.origin.y + yBase + room.wallHeight / 2,
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
      canvas.style.cursor = v === 'select' ? 'pointer' : ''
      if (v !== 'select') clearHighlight()
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

  // Register callbacks for Panel buttons
  setDeleteSelectedRoom(() => deleteSelectedRoom())
  setFlattenSelectedRoomTerrain(() => flattenSelectedRoomTerrain())
  setReinstallSelectedHouse(() => reinstallSelectedHouse())
  setMoveSelectedHouse((deltaX, deltaZ) => moveSelectedHouseBy(deltaX, deltaZ))

  function updateRaycaster(event: MouseEvent) {
    if (!camera) return false
    const rect = canvas.getBoundingClientRect()
    mouseNDC.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(mouseNDC, camera)
    return true
  }

  function raycastTerrain(event: MouseEvent): THREE.Intersection | null {
    if (!updateRaycaster(event)) return null
    const meshes = terrainMeshes.filter((m): m is THREE.Mesh => m !== undefined)
    if (meshes.length === 0) return null
    const intersects = raycaster.intersectObjects(meshes, false)
    return intersects.length > 0 ? intersects[0] : null
  }

  function raycastHousingAll(event: MouseEvent): THREE.Intersection[] {
    if (!housingGroup || !updateRaycaster(event)) return []
    return raycaster.intersectObjects(housingGroup.children, true)
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
      return housingManager.hasFloorSupport(
        previewPos.x,
        previewPos.z,
        sx,
        sz,
        { floorLevel: currentFloorLevel }
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
    if (currentFloorLevel >= 1) {
      return housingManager.hasFloorSupport(
        previewPos.x,
        previewPos.z,
        sx,
        sz,
        { floorLevel: currentFloorLevel }
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
  }

  function handleMouseDown(event: MouseEvent) {
    if (event.button === 1) {
      event.preventDefault()
      isPanning = true
      lastPanX = event.clientX
      lastPanY = event.clientY
      return
    }
    if (event.button !== 0 || get(cameraRotationEnabled)) return
    event.preventDefault()

    if (currentTool === 'select') {
      selectRoomAtCursor(event)
      return
    }

    if (!currentTemplate || !previewPos || !placementValid) return
    placeHouse()
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (isTypingTarget(event.target)) return
    if (event.key === 'r' || event.key === 'R') {
      placementRotation.set((currentRotation + 90) % 360)
    }
    if (event.key === 'Delete' && currentTool === 'select') {
      deleteSelectedRoom()
    }
  }

  let lastSelectKey = ''

  /** Rooms orphaned by removing seedIdx: upper floors without support, and
   *  stairwells missing a regular room on either their entry or exit floor.
   *  Fixpoint loop handles chain reactions (F1 falls → F2 falls → stair orphaned). */
  function computeCascadeDelete(
    house: HouseData,
    seedIdx: number
  ): Set<number> {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const toDelete = new Set<number>([seedIdx])
    let changed = true
    while (changed) {
      changed = false

      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const regularByFloor = new Map<number, true>()
      for (let j = 0; j < house.rooms.length; j++) {
        if (toDelete.has(j)) continue
        const r = house.rooms[j]
        if (r.roomType === 'stairwell') continue
        regularByFloor.set(r.floorLevel, true)
      }

      for (let i = 0; i < house.rooms.length; i++) {
        if (toDelete.has(i)) continue
        const room = house.rooms[i]

        if (room.roomType === 'stairwell') {
          if (
            !regularByFloor.has(room.floorLevel) ||
            !regularByFloor.has(room.floorLevel + 1)
          ) {
            toDelete.add(i)
            changed = true
          }
          continue
        }

        if (room.floorLevel === 0) continue
        const supportLevel = room.floorLevel - 1
        let allCellsSupported = true
        outer: for (let x = room.localX; x < room.localX + room.sizeX; x++) {
          for (let z = room.localZ; z < room.localZ + room.sizeZ; z++) {
            let supported = false
            for (let j = 0; j < house.rooms.length; j++) {
              if (toDelete.has(j)) continue
              const other = house.rooms[j]
              if (other.floorLevel !== supportLevel) continue
              if (
                x >= other.localX &&
                x < other.localX + other.sizeX &&
                z >= other.localZ &&
                z < other.localZ + other.sizeZ
              ) {
                supported = true
                break
              }
            }
            if (!supported) {
              allCellsSupported = false
              break outer
            }
          }
        }
        if (!allCellsSupported) {
          toDelete.add(i)
          changed = true
        }
      }
    }
    return toDelete
  }

  async function deleteSelectedRoom() {
    const houseId = get(selectedHouseId)
    const roomIdx = get(selectedRoomIndex)
    if (houseId == null || roomIdx == null) return

    const house = housingManager.getHouseById(houseId)
    if (!house || roomIdx >= house.rooms.length) return

    const deletedRoom = house.rooms[roomIdx]

    const toDelete = computeCascadeDelete(house, roomIdx)
    if (toDelete.size > 1) {
      const extra = toDelete.size - 1
      if (
        !confirm(
          `이 방을 지우면 연관된 방 ${extra}개도 함께 삭제됩니다. 계속하시겠습니까?`
        )
      ) {
        return
      }
    }

    selectedHouseId.set(null)
    selectedRoomIndex.set(null)

    if (toDelete.size >= house.rooms.length) {
      await housingManager.deleteHouse(house.id)
    } else {
      const updatedRooms = house.rooms.filter((_, i) => !toDelete.has(i))
      const updatedHouse: HouseData = { ...house, rooms: updatedRooms }
      await housingManager.updateHouse(updatedHouse)
    }

    if (
      deletedRoom.floorLevel === 0 &&
      deletedRoom.roomType !== 'stairwell' &&
      heightManager
    ) {
      await restoreGroundAfterRemoval([roomRect(house, deletedRoom)])
    }
  }

  function flattenSelectedRoomTerrain() {
    const houseId = get(selectedHouseId)
    const roomIdx = get(selectedRoomIndex)
    if (houseId == null || roomIdx == null || !heightManager) return

    const house = housingManager.getHouseById(houseId)
    if (!house || roomIdx >= house.rooms.length) return

    const room = house.rooms[roomIdx]
    if (room.floorLevel !== 0 || room.roomType === 'stairwell') return

    const roomWorldX = house.origin.x + room.localX
    const roomWorldZ = house.origin.z + room.localZ
    const protectedRects = buildGroundFloorRects(
      (ph, pr) => pr === room && ph.id === house.id
    )

    heightManager.flattenArea(
      roomWorldX,
      roomWorldZ,
      roomWorldX + room.sizeX,
      roomWorldZ + room.sizeZ,
      house.origin.y,
      BLEND_RADIUS,
      (wx, wz) => isInAnyRect(protectedRects, wx, wz)
    )
    heightManager.saveAllDirty()
  }

  function groundFloorRooms(house: HouseData): RoomData[] {
    return house.rooms.filter(
      (room) => room.floorLevel === 0 && room.roomType !== 'stairwell'
    )
  }

  function roomGrassRect(house: HouseData, room: RoomData): Rect {
    return roomRect(house, room, GRASS_MARGIN)
  }

  function roomRect(house: HouseData, room: RoomData, margin = 0): Rect {
    const minX = house.origin.x + room.localX - margin
    const minZ = house.origin.z + room.localZ - margin
    return {
      minX,
      minZ,
      maxX: minX + room.sizeX + margin * 2,
      maxZ: minZ + room.sizeZ + margin * 2,
    }
  }

  async function restoreGroundAfterRemoval(
    removedRooms: Rect[],
    includeHouseId?: string
  ) {
    const heights = heightManager
    if (!heights || removedRooms.length === 0) return

    const restoreBounds: Rect = {
      minX: Infinity,
      minZ: Infinity,
      maxX: -Infinity,
      maxZ: -Infinity,
    }
    for (const room of removedRooms) {
      const restore = {
        minX: room.minX - BLEND_RADIUS,
        minZ: room.minZ - BLEND_RADIUS,
        maxX: room.maxX + BLEND_RADIUS,
        maxZ: room.maxZ + BLEND_RADIUS,
      }
      restoreBounds.minX = Math.min(restoreBounds.minX, restore.minX)
      restoreBounds.minZ = Math.min(restoreBounds.minZ, restore.minZ)
      restoreBounds.maxX = Math.max(restoreBounds.maxX, restore.maxX)
      restoreBounds.maxZ = Math.max(restoreBounds.maxZ, restore.maxZ)
      heights.restoreFromOriginal(
        restore.minX,
        restore.minZ,
        restore.maxX,
        restore.maxZ
      )
    }

    const currentRooms: { house: HouseData; rect: Rect }[] = []
    for (const house of housingManager.getAllHouses()) {
      for (const room of groundFloorRooms(house)) {
        currentRooms.push({ house, rect: roomRect(house, room) })
      }
    }

    for (const current of currentRooms) {
      const nearRestore =
        current.rect.minX - BLEND_RADIUS <= restoreBounds.maxX &&
        current.rect.maxX + BLEND_RADIUS >= restoreBounds.minX &&
        current.rect.minZ - BLEND_RADIUS <= restoreBounds.maxZ &&
        current.rect.maxZ + BLEND_RADIUS >= restoreBounds.minZ
      if (current.house.id !== includeHouseId && !nearRestore) continue

      heights.flattenArea(
        current.rect.minX,
        current.rect.minZ,
        current.rect.maxX,
        current.rect.maxZ,
        current.house.origin.y,
        BLEND_RADIUS,
        (wx, wz) =>
          currentRooms.some(
            (other) => other !== current && isInRect(other.rect, wx, wz)
          )
      )
    }
    await heights.saveAllDirty()

    const grass = grassDataManager
    if (!grass) return

    const affectedTiles: { x: number; z: number }[] = []
    for (const room of removedRooms) {
      const grassRect = {
        minX: room.minX - GRASS_MARGIN,
        minZ: room.minZ - GRASS_MARGIN,
        maxX: room.maxX + GRASS_MARGIN,
        maxZ: room.maxZ + GRASS_MARGIN,
      }
      const bounds = worldRectToTileBounds(
        grassRect.minX,
        grassRect.minZ,
        grassRect.maxX,
        grassRect.maxZ
      )
      for (let z = bounds.tileMinZ; z <= bounds.tileMaxZ; z++) {
        for (let x = bounds.tileMinX; x <= bounds.tileMaxX; x++) {
          if (!affectedTiles.some((tile) => tile.x === x && tile.z === z)) {
            affectedTiles.push({ x, z })
          }
        }
      }
    }
    await Promise.all(
      affectedTiles.map(({ x, z }) => grass.restoreFromOriginal(x, z))
    )

    const recarveRects = currentRooms
      .map(({ rect }) => ({
        minX: rect.minX - GRASS_MARGIN,
        minZ: rect.minZ - GRASS_MARGIN,
        maxX: rect.maxX + GRASS_MARGIN,
        maxZ: rect.maxZ + GRASS_MARGIN,
      }))
      .filter((rect) => {
        const bounds = worldRectToTileBounds(
          rect.minX,
          rect.minZ,
          rect.maxX,
          rect.maxZ
        )
        return affectedTiles.some(
          ({ x, z }) =>
            x >= bounds.tileMinX &&
            x <= bounds.tileMaxX &&
            z >= bounds.tileMinZ &&
            z <= bounds.tileMaxZ
        )
      })
    await grass.removeGrassInRects(recarveRects)
  }

  async function reinstallSelectedHouse() {
    const houseId = get(selectedHouseId)
    if (houseId == null || !heightManager) return

    const house = housingManager.getHouseById(houseId)
    if (!house) return

    const rooms = groundFloorRooms(house)
    if (rooms.length === 0) return

    const protectedRects = buildGroundFloorRects((ph) => ph.id === house.id)
    for (const room of rooms) {
      const roomWorldX = house.origin.x + room.localX
      const roomWorldZ = house.origin.z + room.localZ
      heightManager.flattenArea(
        roomWorldX,
        roomWorldZ,
        roomWorldX + room.sizeX,
        roomWorldZ + room.sizeZ,
        house.origin.y,
        BLEND_RADIUS,
        (wx, wz) => isInAnyRect(protectedRects, wx, wz)
      )
    }
    // Height tiles and grass tiles are disjoint files — persist both in parallel.
    await Promise.all([
      heightManager.saveAllDirty(),
      grassDataManager?.removeGrassInRects(
        rooms.map((room) => roomGrassRect(house, room))
      ),
    ])

    const saved = await housingManager.updateHouse(house)
    if (!saved) {
      console.warn(`Failed to reinstall house ${house.id}`)
    }
  }

  async function moveSelectedHouseBy(
    deltaX: number,
    deltaZ: number
  ): Promise<boolean> {
    const houseId = get(selectedHouseId)
    if (houseId == null || !heightManager) return false

    const house = housingManager.getHouseById(houseId)
    if (!house) return false

    const previous = structuredClone(house)
    const moved = structuredClone(house)
    moved.origin.x += deltaX
    moved.origin.z += deltaZ

    const saved = await housingManager.updateHouse(moved)
    if (!saved) return false

    const movedObjects = await objectManager
      .moveHouseContents(previous, deltaX, deltaZ)
      .catch((error) => {
        console.error('Failed to load house contents:', error)
        return false
      })
    if (!movedObjects) {
      const rolledBack = await housingManager.updateHouse(previous)
      if (!rolledBack) {
        console.error(`Failed to roll back house ${house.id} after object move`)
      }
      scheduleUpdateHighlight()
      return false
    }

    const oldRooms = groundFloorRooms(previous)
    await restoreGroundAfterRemoval(
      oldRooms.map((room) => roomRect(previous, room)),
      houseId
    )

    scheduleUpdateHighlight()
    return true
  }

  function applyRoomSelection(
    results: { house: HouseData; roomIndex: number }[]
  ) {
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

  /** Find rooms containing a world point. When checkY is true, validates Y range too. */
  function findRoomsAtPoint(
    px: number,
    py: number,
    pz: number,
    checkY: boolean,
    seen: Set<string>,
    out: { house: HouseData; roomIndex: number }[]
  ) {
    for (const house of housingManager.getAllHouses()) {
      for (let i = 0; i < house.rooms.length; i++) {
        const room = house.rooms[i]
        const rx = house.origin.x + room.localX
        const rz = house.origin.z + room.localZ
        if (px < rx || px > rx + room.sizeX) continue
        if (pz < rz || pz > rz + room.sizeZ) continue
        if (checkY) {
          const ryBase =
            house.origin.y + floorYBase(room.floorLevel, room.wallHeight)
          const yTop =
            room.roomType === 'stairwell'
              ? house.origin.y +
                floorYBase(room.floorLevel + 1, room.wallHeight) +
                room.wallHeight
              : ryBase + room.wallHeight
          if (py < ryBase - 0.5 || py > yTop + 0.5) continue
        }
        const key = `${house.id}:${i}`
        if (!seen.has(key)) {
          seen.add(key)
          out.push({ house, roomIndex: i })
        }
      }
    }
  }

  function selectRoomAtCursor(event: MouseEvent) {
    const results: { house: HouseData; roomIndex: number }[] = []
    const seen = new Set<string>()

    // Raycast through all housing meshes — each hit point is matched
    // to its room, giving a natural cycle of only the rooms the ray pierces.
    for (const hit of raycastHousingAll(event)) {
      const p = hit.point
      findRoomsAtPoint(p.x, p.y, p.z, true, seen, results)
    }

    // Fallback: terrain raycast for clicking on exposed floor (XZ only)
    if (results.length === 0) {
      const terrainHit = raycastTerrain(event)
      if (terrainHit) {
        const p = terrainHit.point
        findRoomsAtPoint(p.x, p.y, p.z, false, seen, results)
      }
    }

    if (results.length === 0) {
      selectedHouseId.set(null)
      selectedRoomIndex.set(null)
      lastSelectKey = ''
      return
    }
    applyRoomSelection(results)
  }

  async function placeHouse() {
    if (!currentTemplate || !previewPos || !heightManager) return

    const pos = { ...previewPos }
    const { sx, sz } = getRotatedSize()

    const newRoom = buildRoomData(sx, sz)
    const shouldFlattenTerrain =
      currentFloorLevel === 0 && currentRoomType !== 'stairwell'

    // Build protected rects BEFORE saving (so the new room isn't included)
    const protectedRects = shouldFlattenTerrain ? buildGroundFloorRects() : []

    // Stairwells and 2F rooms attach to the house with supporting 1F rooms
    // 1F rooms check edge adjacency
    let targetHouse: HouseData | null
    if (currentRoomType === 'stairwell' || currentFloorLevel >= 1) {
      targetHouse = housingManager.findSupportingHouse(
        pos.x,
        pos.z,
        sx,
        sz,
        currentFloorLevel
      )
    } else {
      targetHouse = housingManager.findAdjacentHouse(pos.x, pos.z, sx, sz)
    }

    // Attached rooms inherit the house's floor height so the floors line up;
    // a new house averages terrain across the footprint to balance cut/fill.
    let targetHeight: number
    if (targetHouse) {
      targetHeight = targetHouse.origin.y
    } else {
      let sum = 0
      for (let j = 0; j < sz; j++) {
        for (let i = 0; i < sx; i++) {
          sum += heightManager.getHeightAtWorldPosition(
            pos.x + i + 0.5,
            pos.z + j + 0.5
          )
        }
      }
      targetHeight = sum / (sx * sz)
    }

    let saved: HouseData | null
    if (targetHouse) {
      // Add room to existing house — localX/Z relative to house origin
      newRoom.localX = pos.x - targetHouse.origin.x
      newRoom.localZ = pos.z - targetHouse.origin.z

      const updatedRooms = [...targetHouse.rooms, newRoom]
      setSharedWallsOpen(updatedRooms, newRoom)

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
        (wx, wz) => isInAnyRect(protectedRects, wx, wz)
      )
      heightManager.saveAllDirty()

      // Remove grass under the house footprint (+ 1m margin)
      if (grassDataManager) {
        await grassDataManager.removeGrassInRects([
          {
            minX: pos.x - GRASS_MARGIN,
            minZ: pos.z - GRASS_MARGIN,
            maxX: pos.x + sx + GRASS_MARGIN,
            maxZ: pos.z + sz + GRASS_MARGIN,
          },
        ])
      }
    }
  }

  function fillWall(
    count: number,
    variant: WallVariant,
    texture: number
  ): WallConfig[] {
    const base: WallVariant =
      variant === 'door' || variant === 'window' ? 'solid' : variant
    const segs: WallConfig[] = Array.from({ length: count }, () => ({
      variant: base,
      texture,
    }))
    if (variant === 'door' && count % 2 === 0 && count >= 2) {
      segs[count / 2 - 1] = { variant: 'double-door', texture }
      segs[count / 2] = { variant: 'double-door', texture }
    } else if (variant === 'door' || variant === 'window') {
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
      roofType: get(placementRoofType),
      ...(currentRoomType === 'stairwell' && {
        stairReversed: currentRotation === 180 || currentRotation === 270,
      }),
      localX: 0,
      localZ: 0,
      sizeX,
      sizeZ,
      floorLevel: currentFloorLevel,
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
   * Auto-set overlapping 1m wall segments to 'open' where `added` touches
   * another room. Only pairs involving the new room are touched so interior
   * walls edited on existing rooms survive later placements.
   */
  function setSharedWallsOpen(rooms: RoomData[], a: RoomData) {
    for (const b of rooms) {
      if (b === a) continue
      // Same floor, or a stairwell and the room on the floor above it
      const sameFloor = a.floorLevel === b.floorLevel
      const stairwellCrossFloor =
        (a.roomType === 'stairwell' && b.floorLevel === a.floorLevel + 1) ||
        (b.roomType === 'stairwell' && a.floorLevel === b.floorLevel + 1)
      if (!sameFloor && !stairwellCrossFloor) continue

      if (a.localZ + a.sizeZ === b.localZ)
        openOverlappingSegments(a, 'wallSouth', b, 'wallNorth', 'x')
      if (b.localZ + b.sizeZ === a.localZ)
        openOverlappingSegments(b, 'wallSouth', a, 'wallNorth', 'x')
      if (a.localX + a.sizeX === b.localX)
        openOverlappingSegments(a, 'wallEast', b, 'wallWest', 'z')
      if (b.localX + b.sizeX === a.localX)
        openOverlappingSegments(b, 'wallEast', a, 'wallWest', 'z')
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
    setDeleteSelectedRoom(null)
    setFlattenSelectedRoomTerrain(null)
    setReinstallSelectedHouse(null)
    setMoveSelectedHouse(null)
    placementPreview.set(null)
    previewMatValid.dispose()
    previewMatInvalid.dispose()
    highlightEdgeMat.dispose()
    clearHighlight()

    if (previewMesh) {
      previewGroup.remove(previewMesh)
      disposeHouseGroup(previewMesh)
      previewMesh = null
    }
  })
</script>

<T is={previewGroup} />
