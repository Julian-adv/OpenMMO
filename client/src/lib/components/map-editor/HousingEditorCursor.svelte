<script lang="ts">
  import { T, useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy } from 'svelte'
  import { get } from 'svelte/store'
  import {
    selectedRoomTemplate,
    placementRotation,
    placementPreview,
    wallTextureIndex,
    floorTextureIndex,
    roofTextureIndex,
    housingDeleteMode,
    wallVariants,
    type RoomTemplate,
    type WallVariants,
  } from '../../stores/housingEditorStore'
  import type { HouseData, RoomData } from '../../types/housing'
  import { housingManager } from '../../managers/housingManager'
  import { buildHouseGroup, disposeHouseGroup } from '../../utils/house-geometry'
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
  let deleteMode = $state(false)
  let currentWallVariants = $state<WallVariants>({
    north: 'solid',
    south: 'door',
    east: 'solid',
    west: 'solid',
  })
  let previewPos = $state<{ x: number; z: number } | null>(null)
  let previewMesh: THREE.Group | null = null
  let placementValid = false

  const BLEND_RADIUS = 4

  let rebuildScheduled = false
  function scheduleRebuildPreview() {
    if (rebuildScheduled) return
    rebuildScheduled = true
    queueMicrotask(() => {
      rebuildScheduled = false
      rebuildPreview()
    })
  }

  const unsubs = [
    selectedRoomTemplate.subscribe((v) => {
      currentTemplate = v
      scheduleRebuildPreview()
    }),
    placementRotation.subscribe((v) => {
      currentRotation = v
      updatePreviewTransform()
    }),
    housingDeleteMode.subscribe((v) => {
      deleteMode = v
      canvas.style.cursor = v ? 'crosshair' : ''
    }),
    wallVariants.subscribe((v) => {
      currentWallVariants = v
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

    const houseData = templateToHouseData(currentTemplate, 0, 0, 0)
    const result = buildHouseGroup(houseData)

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
    previewMesh.rotation.y = (currentRotation * Math.PI) / 180
  }

  function checkPlacementValid(): boolean {
    if (!currentTemplate || !previewPos) return false
    const rotated = currentRotation === 90 || currentRotation === 270
    const sx = rotated ? currentTemplate.sizeZ : currentTemplate.sizeX
    const sz = rotated ? currentTemplate.sizeX : currentTemplate.sizeZ
    return !housingManager.checkOverlap(previewPos.x, previewPos.z, sx, sz)
  }

  function setPreviewMaterial(valid: boolean) {
    if (!previewMesh) return
    const mat = valid ? previewMatValid : previewMatInvalid
    previewMesh.traverse((obj) => {
      if (obj instanceof THREE.Mesh) obj.material = mat
    })
  }

  function handleMouseMove(event: MouseEvent) {
    const hit = raycastTerrain(event)
    if (!hit || (!currentTemplate && !deleteMode)) {
      placementPreview.set(null)
      previewPos = null
      if (previewMesh) previewMesh.visible = false
      return
    }

    const x = Math.floor(hit.point.x)
    const z = Math.floor(hit.point.z)
    previewPos = { x, z }
    placementPreview.set({ x, z })

    if (previewMesh && !deleteMode) {
      previewMesh.visible = true
      previewMesh.position.set(x, hit.point.y, z)
      previewMesh.rotation.y = (currentRotation * Math.PI) / 180
      const wasValid = placementValid
      placementValid = checkPlacementValid()
      if (placementValid !== wasValid) setPreviewMaterial(placementValid)
    } else if (previewMesh && deleteMode) {
      previewMesh.visible = false
    }
  }

  function handleMouseDown(event: MouseEvent) {
    if (event.button !== 0) return
    event.preventDefault()

    if (deleteMode) {
      deleteHouseAtCursor(event)
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

  function deleteHouseAtCursor(event: MouseEvent) {
    const hit = raycastTerrain(event)
    if (!hit) return

    const house = housingManager.findHouseAtPoint(
      hit.point.x,
      hit.point.y,
      hit.point.z
    )
    if (house) {
      housingManager.deleteHouse(house.id)
      housingDeleteMode.set(false)
    }
  }

  async function placeHouse() {
    if (!currentTemplate || !previewPos || !heightManager) return

    const pos = { ...previewPos }
    const template = currentTemplate
    const centerX = pos.x + template.sizeX / 2
    const centerZ = pos.z + template.sizeZ / 2
    const targetHeight = heightManager.getHeightAtWorldPosition(centerX, centerZ)

    const houseData = templateToHouseData(template, pos.x, targetHeight, pos.z)

    // Save to server first — only modify terrain/grass on success
    const saved = await housingManager.saveHouse(houseData)
    if (!saved) return

    heightManager.flattenArea(
      pos.x,
      pos.z,
      pos.x + template.sizeX,
      pos.z + template.sizeZ,
      targetHeight,
      BLEND_RADIUS
    )
    heightManager.saveAllDirty()

    // Remove grass under the house footprint (+ 1m margin)
    if (grassDataManager) {
      const GRASS_MARGIN = 1
      const rectMinX = pos.x - GRASS_MARGIN
      const rectMinZ = pos.z - GRASS_MARGIN
      const rectMaxX = pos.x + template.sizeX + GRASS_MARGIN
      const rectMaxZ = pos.z + template.sizeZ + GRASS_MARGIN

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

  function templateToHouseData(
    template: RoomTemplate,
    originX: number,
    originY: number,
    originZ: number
  ): HouseData {
    const wallTex = get(wallTextureIndex)
    const floorTex = get(floorTextureIndex)
    const roofTex = get(roofTextureIndex)

    const wv = currentWallVariants
    const room: RoomData = {
      localX: 0,
      localZ: 0,
      sizeX: template.sizeX,
      sizeZ: template.sizeZ,
      floorLevel: 0,
      floorTexture: floorTex,
      roofTexture: roofTex,
      wallHeight: 3,
      wallNorth: { variant: wv.north, texture: wallTex },
      wallSouth: { variant: wv.south, texture: wallTex },
      wallEast: { variant: wv.east, texture: wallTex },
      wallWest: { variant: wv.west, texture: wallTex },
    }

    return {
      id: '',
      ownerId: 'local',
      origin: { x: originX, y: originY, z: originZ },
      rooms: [room],
    }
  }

  canvas.addEventListener('mousemove', handleMouseMove)
  canvas.addEventListener('mousedown', handleMouseDown)
  window.addEventListener('keydown', handleKeyDown)

  onDestroy(() => {
    unsubs.forEach((u) => u())
    canvas.removeEventListener('mousemove', handleMouseMove)
    canvas.removeEventListener('mousedown', handleMouseDown)
    window.removeEventListener('keydown', handleKeyDown)
    canvas.style.cursor = ''
    placementPreview.set(null)
    previewMatValid.dispose()
    previewMatInvalid.dispose()

    if (previewMesh) {
      previewGroup.remove(previewMesh)
      disposeHouseGroup(previewMesh)
      previewMesh = null
    }
  })
</script>

<T is={previewGroup} />
