<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import {
    loadEstateFurnitureModel,
    furnitureModelDefinition,
  } from '../../utils/estateFurnitureModels'
  import { buildShopSignText, getShopSignStyle } from '../../utils/shop-sign'
  import {
    TorchFireParticles,
    CampfireFireParticles,
    type FireParticles,
  } from '../../effects/fire-particles'
  import {
    cameraRotationEnabled,
    housingEditorMode,
    mapEditorMode,
  } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import {
    estateFurniturePlacementMode,
    estateFurniturePlacementRotation,
    estateFurniturePlacementHeight,
    initializeEstateFurniturePlacementHeight,
    adjustEstateFurniturePlacementHeight,
    rotateEstateFurniturePlacement,
  } from '../../stores/estateFurniturePlacementStore'
  import { isTypingTarget } from '../../utils/dom'
  import {
    EstatePlacementGrid,
    furnitureFootprintsOverlap,
    furniturePlacementRotation,
    footprintOnOwnedEstate,
    footprintOnHouseFloor,
    houseFloorY,
    housingPlacementFloor,
    housingPlacementHouseId,
    pointOnHouseFloor,
    snapPlacementCoordinate,
    type EstateFurniturePlacement,
    type EstateFurniturePlacementDefinition,
    type EstatePlot,
    type FurnitureFootprintPose,
  } from '../../terrain/estatePlacement'
  import {
    EstateFurniturePlacementCursor,
    isFurnitureNudgeKey,
  } from '../../terrain/estateFurniturePlacementCursor'
  import { unwrapWorldXNear, wrapWorldX } from '../../terrain/world-wrap'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import { housingManager } from '../../managers/housingManager'
  import { playerInsideHouseId } from '../../stores/housingStore'
  import type { LocalPlayer } from '../../stores/gameStore'

  let {
    definition,
    plots,
    pending,
    terrainMeshes,
    housingGroup,
    furnitureGroup,
    ignoreFurnitureId,
    heightManager,
    player,
    floorLevel,
    obstacles = [],
    onplace,
    oncancel,
    onerror,
  }: {
    definition: EstateFurniturePlacementDefinition
    plots: EstatePlot[]
    pending: boolean
    terrainMeshes: (THREE.Mesh | undefined)[]
    housingGroup?: THREE.Group
    furnitureGroup?: THREE.Group
    ignoreFurnitureId?: number
    heightManager: TerrainHeightManager
    player: LocalPlayer | null
    floorLevel: number
    obstacles?: FurnitureFootprintPose[]
    onplace: (placement: EstateFurniturePlacement) => void
    oncancel: () => void
    onerror: (message: string) => void
  } = $props()

  const { camera, renderer } = useThrelte()
  const group = new THREE.Group()
  const raycaster = new THREE.Raycaster()
  const pointer = new THREE.Vector2()
  function placementHouse() {
    const houseId = get(playerInsideHouseId)
    return houseId ? housingManager.getHouseById(houseId) : undefined
  }

  const grid = new EstatePlacementGrid(
    (x, z) => {
      const house = placementHouse()
      if (house) return houseFloorY(house, floorLevel, x, z)
      return floorLevel === 0 ? heightManager.groundYOrNull(x, z) : null
    },
    (x, z) => {
      const house = placementHouse()
      return !house || pointOnHouseFloor(house, floorLevel, x, z)
    }
  )
  group.add(grid.object)

  let ghost: THREE.Group | null = null
  const originalColors: {
    material: THREE.MeshStandardMaterial
    color: THREE.Color
  }[] = []
  const invalidColor = new THREE.Color('#ef5b5b')
  const firePosition = new THREE.Vector3()
  let fire: FireParticles | null = null
  let fireOffset: THREE.Vector3 | null = null
  let torchFire = false
  let signText: THREE.Mesh | null = null
  let renderedSignText: string | undefined
  const cursor = new EstateFurniturePlacementCursor()
  let preview: EstateFurniturePlacement | null = null
  let previewValid = false
  let lastGridFloor = Infinity
  let lastGridHouseId: string | null = null
  let disposed = false

  export function getFirePositions() {
    return fire && !torchFire && ghost?.visible && group.visible
      ? [firePosition]
      : []
  }

  export function getTorchPositions() {
    return fire && torchFire && ghost?.visible && group.visible
      ? [firePosition]
      : []
  }

  function setPointer(clientX: number, clientY: number) {
    const rect = renderer.domElement.getBoundingClientRect()
    pointer.set(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(pointer, get(camera))
  }

  function hidePreview() {
    if (ghost) ghost.visible = false
    preview = null
    previewValid = false
  }

  function surfaceHit() {
    const surfaces: THREE.Object3D[] = terrainMeshes.filter(
      (mesh): mesh is THREE.Mesh => !!mesh
    )
    if (housingGroup) surfaces.push(housingGroup)
    if (furnitureGroup && definition.maxHeightOffset)
      surfaces.push(furnitureGroup)
    return raycaster.intersectObjects(surfaces, true).find((hit) => {
      if (!hit.face) return false
      if (ignoreFurnitureId !== undefined) {
        for (
          let object: THREE.Object3D | null = hit.object;
          object;
          object = object.parent
        ) {
          if (object.userData.estateChestId === ignoreFurnitureId) return false
        }
      }
      const upward =
        hit.face.normal.clone().transformDirection(hit.object.matrixWorld).y >
        0.65
      if (!upward) return false
      const housingFloor = housingPlacementFloor(hit.object)
      const insideHouseId = get(playerInsideHouseId)
      if (housingFloor !== null)
        return (
          insideHouseId !== null &&
          housingFloor === floorLevel &&
          housingPlacementHouseId(hit.object) === insideHouseId
        )
      let parent: THREE.Object3D | null = hit.object
      while (parent) {
        if (parent === furnitureGroup) return true
        if (parent === housingGroup) return false
        parent = parent.parent
      }
      return floorLevel === 0 && insideHouseId === null
    })
  }

  function tintGhost(valid: boolean) {
    for (const { material, color } of originalColors) {
      material.color.copy(color)
      if (!valid) material.color.lerp(invalidColor, 0.65)
    }
  }

  function clearSignText() {
    if (!signText) return
    signText.removeFromParent()
    signText.geometry.dispose()
    const materials = Array.isArray(signText.material)
      ? signText.material
      : [signText.material]
    for (const material of materials) {
      ;(material as THREE.MeshBasicMaterial).map?.dispose()
      material.dispose()
    }
    signText = null
  }

  function updateSignText() {
    if (!ghost) return
    const mode = get(estateFurniturePlacementMode)
    const text = mode?.kind === 'move' ? (mode.furniture.text ?? '') : ''
    if (text === renderedSignText) return
    renderedSignText = text
    clearSignText()
    const model = furnitureModelDefinition(definition.modelId)
    if (text && model?.procedural === 'shopSign') {
      const style = getShopSignStyle(model.shopSignStyle)
      signText = buildShopSignText(text, style.board, style.text)
      ghost.add(signText)
    } else ghost.userData.objectText = text || undefined
  }

  function updatePreview() {
    const mode = get(estateFurniturePlacementMode)
    if (
      mode?.kind === 'move' &&
      get(estateFurniturePlacementHeight).offset === null
    ) {
      const { position, floor_level } = mode.furniture
      const house = placementHouse()
      const baseY = house
        ? houseFloorY(house, floor_level, position.x, position.z)
        : heightManager.groundYOrNull(position.x, position.z)
      if (baseY === null) {
        hidePreview()
        return
      }
      initializeEstateFurniturePlacementHeight(baseY)
    }
    if (!player || !ghost) {
      hidePreview()
      return
    }
    if (get(cameraRotationEnabled)) return
    if (cursor.pointer && !cursor.position)
      setPointer(cursor.pointer.x, cursor.pointer.y)
    const point =
      cursor.position ??
      (cursor.pointer
        ? surfaceHit()?.point
        : mode?.kind === 'move'
          ? mode.furniture.position
          : null)
    if (!point) {
      hidePreview()
      return
    }
    const x = snapPlacementCoordinate(
      unwrapWorldXNear(player.position.x, point.x),
      definition.snapStep
    )
    const z = snapPlacementCoordinate(point.z, definition.snapStep)
    const house = placementHouse()
    const fits = (rotation: number) =>
      footprintOnOwnedEstate(x, z, rotation, definition.footprint, plots) &&
      (definition.solid === false ||
        !obstacles.some((obstacle) =>
          furnitureFootprintsOverlap(
            { x, z, rotationDeg: rotation, footprint: definition.footprint },
            obstacle
          )
        )) &&
      (!house ||
        footprintOnHouseFloor(
          house,
          floorLevel,
          x,
          z,
          rotation,
          definition.footprint,
          definition.floorEdgeClearance
        ))
    const rotation = get(estateFurniturePlacementRotation)
    const rotationDeg = furniturePlacementRotation(
      rotation.degrees,
      definition.rotationStep,
      rotation.manual,
      fits
    )
    if (rotationDeg !== rotation.degrees) {
      estateFurniturePlacementRotation.set({
        ...rotation,
        degrees: rotationDeg,
      })
    }
    const baseY =
      (house
        ? houseFloorY(house, floorLevel, x, z)
        : heightManager.groundYOrNull(x, z)) ?? point.y
    const height = get(estateFurniturePlacementHeight)
    const offset = Math.max(
      0,
      Math.min(
        definition.maxHeightOffset ?? 0,
        height.manual ? (height.offset ?? 0) : point.y - baseY
      )
    )
    if (height.offset !== offset)
      estateFurniturePlacementHeight.set({ ...height, offset })
    const y = baseY + offset
    previewValid =
      floorLevel >= definition.minFloor &&
      floorLevel <= definition.maxFloor &&
      fits(rotationDeg)
    ghost.visible = true
    ghost.position.set(x, y, z)
    ghost.rotation.y = THREE.MathUtils.degToRad(rotationDeg)
    tintGhost(previewValid)
    preview = {
      position: { x: wrapWorldX(x), y, z },
      rotationDeg,
      floorLevel,
    }
  }

  onMount(() => {
    const unsubscribeHeight = heightManager.onHeightChanged(() =>
      grid.markDirty()
    )
    const unsubscribeHouses = housingManager.onHousesChanged(() =>
      grid.markDirty()
    )
    loadEstateFurnitureModel(definition)
      .then((gltf) => {
        if (disposed) return
        ghost = gltf.scene.clone(true)
        ghost.visible = false
        ghost.traverse((object) => {
          if (!(object instanceof THREE.Mesh)) return
          object.castShadow = true
          object.receiveShadow = true
          const sources = Array.isArray(object.material)
            ? object.material
            : [object.material]
          const materials = sources.map((source) => {
            const material = source.clone()
            if (material instanceof THREE.MeshStandardMaterial)
              originalColors.push({ material, color: material.color.clone() })
            return material
          })
          object.material = Array.isArray(object.material)
            ? materials
            : materials[0]
        })
        const model = furnitureModelDefinition(definition.modelId)
        updateSignText()
        if (model?.fire) {
          torchFire = model.fireKind === 'torch'
          fire = torchFire
            ? new TorchFireParticles()
            : new CampfireFireParticles()
          fireOffset = new THREE.Vector3(
            model.fire.x,
            model.fire.y,
            model.fire.z
          )
          fire.group.visible = false
          group.add(fire.group)
        }
        group.add(ghost)
      })
      .catch((error) => {
        if (disposed) return
        console.error('Failed to load estate furniture preview:', error)
        onerror('Could not load the furniture preview. Reload to try again.')
      })

    const canvas = renderer.domElement
    const move = (event: PointerEvent) => {
      cursor.movePointer(event.clientX, event.clientY)
    }
    const save = () => {
      updatePreview()
      if (preview && previewValid) onplace(preview)
      else onerror('Choose a valid position before saving.')
    }
    const click = (event: MouseEvent) => {
      if (
        event.button !== 0 ||
        pending ||
        get(cameraRotationEnabled) ||
        get(mapEditorMode) ||
        get(housingEditorMode) ||
        get(currentDungeonDepth) > 0
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      cursor.prepareSave(event.clientX, event.clientY)
      save()
    }
    const wheel = (event: WheelEvent) => {
      if (
        pending ||
        event.deltaY === 0 ||
        event.ctrlKey ||
        get(cameraRotationEnabled)
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      adjustEstateFurniturePlacementHeight(event.deltaY < 0 ? 1 : -1)
      updatePreview()
    }
    const keydown = (event: KeyboardEvent) => {
      if (
        isTypingTarget(event.target) &&
        !(
          event.target instanceof HTMLInputElement &&
          event.target.type === 'range'
        )
      )
        return
      const saveKey = event.code === 'Enter' || event.code === 'NumpadEnter'
      const nudgeKey = isFurnitureNudgeKey(event.code) ? event.code : null
      if (saveKey || nudgeKey) {
        event.preventDefault()
        event.stopImmediatePropagation()
        if (
          pending ||
          (saveKey && event.repeat) ||
          event.ctrlKey ||
          event.metaKey ||
          event.altKey ||
          get(cameraRotationEnabled) ||
          get(mapEditorMode) ||
          get(housingEditorMode) ||
          get(currentDungeonDepth) > 0
        )
          return
        if (nudgeKey) {
          updatePreview()
          if (preview) {
            cursor.nudge(nudgeKey, preview.position)
            updatePreview()
          }
        } else save()
        return
      }
      if (
        event.code === 'KeyR' &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !get(cameraRotationEnabled)
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        if (!pending && !event.repeat) {
          rotateEstateFurniturePlacement(event.shiftKey ? -1 : 1)
          updatePreview()
        }
        return
      }
      if (event.code !== 'Escape') return
      event.preventDefault()
      event.stopImmediatePropagation()
      if (pending) return
      oncancel()
      hidePreview()
    }
    canvas.addEventListener('pointermove', move)
    canvas.addEventListener('mousedown', click, true)
    canvas.addEventListener('wheel', wheel, { capture: true, passive: false })
    window.addEventListener('keydown', keydown, true)
    return () => {
      disposed = true
      unsubscribeHeight()
      unsubscribeHouses()
      canvas.removeEventListener('pointermove', move)
      canvas.removeEventListener('mousedown', click, true)
      canvas.removeEventListener('wheel', wheel, true)
      window.removeEventListener('keydown', keydown, true)
      clearSignText()
      ghost?.traverse((object) => {
        if (!(object instanceof THREE.Mesh)) return
        const materials = Array.isArray(object.material)
          ? object.material
          : [object.material]
        for (const material of materials) material.dispose()
      })
      originalColors.length = 0
      fire?.dispose()
      grid.dispose()
    }
  })

  useTask((delta) => {
    group.visible = get(currentDungeonDepth) < 1
    const insideHouseId = get(playerInsideHouseId)
    if (floorLevel !== lastGridFloor || insideHouseId !== lastGridHouseId) {
      lastGridFloor = floorLevel
      lastGridHouseId = insideHouseId
      grid.markDirty()
    }
    grid.update(!!player, plots, player?.position.x ?? 0)
    updateSignText()
    updatePreview()
    if (fire && fireOffset && ghost) {
      fire.group.visible = ghost.visible
      if (ghost.visible) {
        firePosition
          .copy(fireOffset)
          .applyEuler(ghost.rotation)
          .add(ghost.position)
        fire.setOrigin(firePosition)
        fire.update(delta, get(camera))
      }
    }
  })
</script>

<T is={group} />
