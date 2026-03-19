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
    type RoomTemplate,
  } from '../../stores/housingEditorStore'
  import type { HouseData, RoomData } from '../../types/housing'
  import { housingManager } from '../../managers/housingManager'
  import { buildHouseGroup, disposeHouseGroup } from '../../utils/house-geometry'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'

  interface Props {
    camera: THREE.OrthographicCamera | undefined
    terrainMeshes: (THREE.Mesh | undefined)[]
    heightManager: TerrainHeightManager | null
  }

  let { camera, terrainMeshes, heightManager }: Props = $props()

  const { renderer } = useThrelte()
  const canvas = renderer.domElement

  const raycaster = new THREE.Raycaster()
  const mouseNDC = new THREE.Vector2()
  const previewGroup = new THREE.Group()
  previewGroup.name = 'housingPreview'

  // Single transparent material for preview (reused, no per-rebuild clones)
  const previewMat = new THREE.MeshBasicMaterial({
    vertexColors: true,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: 0.5,
    depthWrite: false,
  })

  let currentTemplate = $state<RoomTemplate | null>(null)
  let currentRotation = $state(0)
  let deleteMode = $state(false)
  let previewPos = $state<{ x: number; z: number } | null>(null)
  let previewMesh: THREE.Group | null = null

  const BLEND_RADIUS = 4

  const unsubs = [
    selectedRoomTemplate.subscribe((v) => {
      currentTemplate = v
      rebuildPreview()
    }),
    placementRotation.subscribe((v) => {
      currentRotation = v
      updatePreviewTransform()
    }),
    housingDeleteMode.subscribe((v) => {
      deleteMode = v
      canvas.style.cursor = v ? 'crosshair' : ''
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

    // Replace shared material with transparent preview material
    result.houseGroup.traverse((obj) => {
      if (obj instanceof THREE.Mesh) {
        obj.material = previewMat
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

    if (!currentTemplate || !previewPos) return
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

    const clickPoint = hit.point
    // Find which house contains this point
    const allHouses = housingManager.getAllHouses()
    for (const house of allHouses) {
      for (const room of house.rooms) {
        const minX = house.origin.x + room.localX
        const minZ = house.origin.z + room.localZ
        const maxX = minX + room.sizeX
        const maxZ = minZ + room.sizeZ
        const minY = house.origin.y
        const maxY = minY + room.wallHeight

        if (
          clickPoint.x >= minX &&
          clickPoint.x <= maxX &&
          clickPoint.z >= minZ &&
          clickPoint.z <= maxZ &&
          clickPoint.y >= minY - 1 &&
          clickPoint.y <= maxY + 1
        ) {
          housingManager.deleteHouse(house.id)
          return
        }
      }
    }
  }

  function placeHouse() {
    if (!currentTemplate || !previewPos || !heightManager) return

    const centerX = previewPos.x + currentTemplate.sizeX / 2
    const centerZ = previewPos.z + currentTemplate.sizeZ / 2
    const targetHeight = heightManager.getHeightAtWorldPosition(centerX, centerZ)

    heightManager.flattenArea(
      previewPos.x,
      previewPos.z,
      previewPos.x + currentTemplate.sizeX,
      previewPos.z + currentTemplate.sizeZ,
      targetHeight,
      BLEND_RADIUS
    )

    // Save flattened terrain immediately (don't rely on 1s debounce)
    heightManager.saveAllDirty()

    const houseData = templateToHouseData(
      currentTemplate,
      previewPos.x,
      targetHeight,
      previewPos.z
    )

    housingManager.saveHouse(houseData)
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

    const room: RoomData = {
      localX: 0,
      localZ: 0,
      sizeX: template.sizeX,
      sizeZ: template.sizeZ,
      floorLevel: 0,
      floorTexture: floorTex,
      roofTexture: roofTex,
      wallHeight: 3,
      wallNorth: { variant: template.wallNorthVariant, texture: wallTex },
      wallSouth: { variant: template.wallSouthVariant, texture: wallTex },
      wallEast: { variant: template.wallEastVariant, texture: wallTex },
      wallWest: { variant: template.wallWestVariant, texture: wallTex },
    }

    return {
      id: crypto.randomUUID(),
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
    previewMat.dispose()

    if (previewMesh) {
      previewGroup.remove(previewMesh)
      disposeHouseGroup(previewMesh)
      previewMesh = null
    }
  })
</script>

<T is={previewGroup} />
