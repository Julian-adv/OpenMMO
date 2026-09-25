<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import { networkManager } from '../../network/socket'
  import { housingManager } from '../../managers/housingManager'
  import {
    houseDemolitionConfirmation,
    houseDemolitionMode,
    housePlacementMode,
    requestHouseDemolitionConfirmation,
    rotateHousePlacement,
    stopHouseInteraction,
  } from '../../stores/housePlacementStore'
  import {
    cameraRotationEnabled,
    housingEditorMode,
    mapEditorMode,
  } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { playerTrade } from '../../stores/playerTradeStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import { landscapingMode } from '../../stores/landscapingStore'
  import {
    buildHouseGroup,
    disposeHouseGroup,
  } from '../../utils/house-geometry'
  import { shortestWrappedDeltaX, wrapWorldX } from '../../terrain/world-wrap'
  import { ownsEstatePosition } from '../../terrain/landscaping'
  import type { HouseData } from '../../types/housing'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { LocalPlayer } from '../../stores/gameStore'

  let {
    heightManager,
    terrainMeshes,
    player,
    housingGroup,
  }: {
    heightManager: TerrainHeightManager
    terrainMeshes: (THREE.Mesh | undefined)[]
    player: LocalPlayer | null
    housingGroup: THREE.Group | null
  } = $props()

  const { camera, renderer } = useThrelte()
  const group = new THREE.Group()
  const raycaster = new THREE.Raycaster()
  const pointer = new THREE.Vector2()
  const demolitionBox = new THREE.Box3()
  const demolitionHelper = new THREE.Box3Helper(demolitionBox, '#80ff9a')
  let cursor: { x: number; y: number } | null = null
  let preview: THREE.Group | null = null
  let previewInstanceId: number | null = null
  let previewFoundationOffsets: [number, number][] = []
  let lastValid: boolean | null = null
  let demolitionObject: THREE.Object3D | null = null

  demolitionHelper.visible = false
  demolitionHelper.renderOrder = 7
  group.add(demolitionHelper)

  function updateRay() {
    const canvas = renderer.domElement
    const rect = canvas.getBoundingClientRect()
    const point = cursor ?? {
      x: rect.left + rect.width / 2,
      y: rect.top + rect.height / 2,
    }
    pointer.set(
      ((point.x - rect.left) / rect.width) * 2 - 1,
      -((point.y - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(pointer, get(camera))
  }

  function foundationRooms(house: HouseData) {
    return house.rooms.filter(
      (room) => room.floorLevel === 0 && room.roomType !== 'stairwell'
    )
  }

  function foundationOffsets(house: HouseData) {
    const offsets: [number, number][] = []
    for (const room of foundationRooms(house)) {
      for (let dz = 0; dz < room.sizeZ; dz++) {
        for (let dx = 0; dx < room.sizeX; dx++) {
          const x = room.localX + dx + 0.5
          const z = room.localZ + dz + 0.5
          if (!offsets.some(([otherX, otherZ]) => x === otherX && z === otherZ))
            offsets.push([x, z])
        }
      }
    }
    return offsets
  }

  function rotateLocalBounds(
    localX: number,
    localZ: number,
    sizeX: number,
    sizeZ: number,
    quarterTurns: number
  ) {
    for (let turn = 0; turn < quarterTurns; turn++) {
      ;[localX, localZ, sizeX, sizeZ] = [localZ, -localX - sizeX, sizeZ, sizeX]
    }
    return { localX, localZ, sizeX, sizeZ }
  }

  function overlapsExisting(
    house: HouseData,
    x: number,
    z: number,
    quarterTurns: number
  ) {
    return house.rooms.some((source) => {
      const room = rotateLocalBounds(
        source.localX,
        source.localZ,
        source.sizeX,
        source.sizeZ,
        quarterTurns
      )
      return housingManager.checkOverlap(
        x + room.localX,
        z + room.localZ,
        room.sizeX,
        room.sizeZ,
        source.floorLevel
      )
    })
  }

  function disposePreview() {
    if (!preview) return
    group.remove(preview)
    disposeHouseGroup(preview)
    preview.traverse((object) => {
      if (object instanceof THREE.Mesh) {
        const materials = Array.isArray(object.material)
          ? object.material
          : [object.material]
        for (const material of materials) material.dispose()
      }
    })
    preview = null
    previewInstanceId = null
    previewFoundationOffsets = []
  }

  function buildPreview(house: HouseData, instanceId: number) {
    disposePreview()
    const result = buildHouseGroup(house)
    preview = result.houseGroup
    preview.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return
      const clone = (material: THREE.Material) => {
        const copy = material.clone()
        copy.transparent = true
        copy.opacity = 0.58
        copy.depthWrite = false
        if (copy instanceof THREE.MeshStandardMaterial)
          copy.userData.baseColor = copy.color.clone()
        return copy
      }
      object.material = Array.isArray(object.material)
        ? object.material.map(clone)
        : clone(object.material)
      object.renderOrder = 6
    })
    preview.visible = false
    previewInstanceId = instanceId
    previewFoundationOffsets = foundationOffsets(house)
    group.add(preview)
  }

  function tintPreview(valid: boolean) {
    if (!preview || lastValid === valid) return
    lastValid = valid
    const tint = new THREE.Color(valid ? '#80ff9a' : '#ff6868')
    preview.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return
      const materials = Array.isArray(object.material)
        ? object.material
        : [object.material]
      for (const material of materials) {
        if (!(material instanceof THREE.MeshStandardMaterial)) continue
        const base = material.userData.baseColor as THREE.Color | undefined
        material.color.copy(base ?? new THREE.Color('white')).lerp(tint, 0.55)
      }
    })
  }

  function clearTarget(reason = 'Point at your estate to choose a position') {
    if (preview) preview.visible = false
    housePlacementMode.update((mode) =>
      mode ? { ...mode, target: null, valid: false, reason } : mode
    )
  }

  function setDemolitionTarget(
    targetHouseId: string | null,
    valid: boolean,
    reason: string
  ) {
    houseDemolitionMode.update((mode) => {
      if (
        !mode ||
        (mode.targetHouseId === targetHouseId &&
          mode.valid === valid &&
          mode.reason === reason)
      )
        return mode
      return { targetHouseId, valid, reason }
    })
  }

  function clearDemolitionTarget(reason = 'Point at one of your houses') {
    demolitionHelper.visible = false
    demolitionObject = null
    setDemolitionTarget(null, false, reason)
  }

  function houseObjectFromHit(object: THREE.Object3D) {
    let current: THREE.Object3D | null = object
    while (current && current !== housingGroup) {
      if (current.name.startsWith('house_')) {
        return { object: current, houseId: current.name.slice(6) }
      }
      current = current.parent
    }
    return null
  }

  function distanceToHouse(house: HouseData) {
    if (!player) return Infinity
    return foundationRooms(house).reduce((nearest, room) => {
      const originX = shortestWrappedDeltaX(player.position.x, house.origin.x)
      const minX = originX + room.localX
      const maxX = minX + room.sizeX
      const minZ = house.origin.z + room.localZ
      const maxZ = minZ + room.sizeZ
      const dx = minX > 0 ? minX : maxX < 0 ? -maxX : 0
      const dz =
        player.position.z < minZ
          ? minZ - player.position.z
          : player.position.z > maxZ
            ? player.position.z - maxZ
            : 0
      return Math.min(nearest, Math.hypot(dx, dz))
    }, Infinity)
  }

  function updateDemolitionTarget() {
    if (get(houseDemolitionConfirmation)) return
    if (
      !get(houseDemolitionMode) ||
      !player ||
      !housingGroup ||
      get(cameraRotationEnabled)
    ) {
      clearDemolitionTarget()
      return
    }
    updateRay()
    const hit = raycaster.intersectObject(housingGroup, true)[0]
    const target = hit ? houseObjectFromHit(hit.object) : null
    if (!target) {
      clearDemolitionTarget()
      return
    }
    const house = housingManager.getHouseById(target.houseId)
    if (!house) {
      clearDemolitionTarget('That house is not loaded')
      return
    }
    const ownsHouse = house.ownerId === String(get(landscapingMode)?.owner_id)
    const inRange = distanceToHouse(house) <= 30
    const valid = ownsHouse && inRange
    const reason = !ownsHouse
      ? 'You can only demolish your own house'
      : !inRange
        ? 'Move within 30 metres of the house'
        : 'Click to select this house'
    if (demolitionObject !== target.object) {
      demolitionObject = target.object
      demolitionBox.setFromObject(target.object)
    }
    ;(demolitionHelper.material as THREE.LineBasicMaterial).color.set(
      valid ? '#80ff9a' : '#ff6868'
    )
    demolitionHelper.visible = true
    setDemolitionTarget(house.id, valid, reason)
  }

  function updatePreview() {
    const mode = get(housePlacementMode)
    if (!mode || !player || !preview || get(cameraRotationEnabled)) {
      clearTarget()
      return
    }
    updateRay()
    const grounds = terrainMeshes.filter((mesh): mesh is THREE.Mesh => !!mesh)
    const hit = raycaster.intersectObjects(grounds, false)[0]
    if (!hit) {
      clearTarget('Point at outdoor ground')
      return
    }
    const x = wrapWorldX(Math.round(hit.point.x))
    const z = Math.round(hit.point.z)
    const cells = previewFoundationOffsets.map(
      ([localX, localZ]): [number, number] => {
        const { localX: rotatedX, localZ: rotatedZ } = rotateLocalBounds(
          localX,
          localZ,
          0,
          0,
          mode.quarterTurns
        )
        return [wrapWorldX(x + rotatedX), z + rotatedZ]
      }
    )
    const heights = cells.map(([cx, cz]) => heightManager.groundYOrNull(cx, cz))
    const knownHeights = heights.filter(
      (height): height is number => height !== null
    )
    const y = knownHeights.length
      ? knownHeights.reduce((sum, height) => sum + height, 0) /
        knownHeights.length
      : hit.point.y
    let reason: string | null = null
    if (
      Math.hypot(
        shortestWrappedDeltaX(player.position.x, x),
        z - player.position.z
      ) > 30
    )
      reason = 'Choose a position within 30 metres'
    else if (
      !cells.length ||
      !cells.every(([cx, cz]) => ownsEstatePosition(mode.plots, cx, cz))
    )
      reason = 'The whole house must fit inside your estate'
    else if (
      knownHeights.length !== heights.length ||
      Math.max(...knownHeights) - Math.min(...knownHeights) > 1
    )
      reason = 'The ground is too uneven for this house'
    else if (overlapsExisting(mode.house, x, z, mode.quarterTurns))
      reason = 'That position overlaps another house'
    preview.visible = true
    preview.position.set(x, y, z)
    tintPreview(reason === null)
    housePlacementMode.update((current) => {
      if (!current) return current
      const moved = current.target?.x !== x || current.target?.z !== z
      const error = moved ? null : current.error
      if (
        !moved &&
        current.target?.y === y &&
        current.valid === (reason === null) &&
        current.reason === reason
      )
        return current
      return {
        ...current,
        target: { x, y, z },
        valid: reason === null,
        error,
        reason,
      }
    })
  }

  const unsubscribe = housePlacementMode.subscribe((mode) => {
    if (!mode) {
      disposePreview()
      return
    }
    if (previewInstanceId !== mode.instanceId)
      buildPreview(mode.house, mode.instanceId)
    if (preview) preview.rotation.y = mode.quarterTurns * (Math.PI / 2)
  })

  onMount(() => {
    const canvas = renderer.domElement
    const move = (event: PointerEvent) => {
      cursor = { x: event.clientX, y: event.clientY }
    }
    const leave = () => {
      cursor = null
    }
    const click = (event: MouseEvent) => {
      if (event.button !== 0 || get(cameraRotationEnabled)) return
      const mode = get(housePlacementMode)
      const demolitionMode = get(houseDemolitionMode)
      if (!mode && !demolitionMode) return
      event.preventDefault()
      event.stopImmediatePropagation()
      cursor = { x: event.clientX, y: event.clientY }
      if (mode) {
        updatePreview()
        const current = get(housePlacementMode)
        if (!current?.valid || !current.target || current.pending) return
        housePlacementMode.set({ ...current, pending: true, error: null })
        networkManager.sendPlaceHouse(
          current.instanceId,
          current.target,
          current.quarterTurns
        )
        return
      }
      updateDemolitionTarget()
      const current = get(houseDemolitionMode)
      if (!current?.valid || !current.targetHouseId) return
      const house = housingManager.getHouseById(current.targetHouseId)
      if (!house) return
      requestHouseDemolitionConfirmation(house)
    }
    const keydown = (event: KeyboardEvent) => {
      if (get(houseDemolitionConfirmation)) return
      if (
        event.code === 'KeyR' &&
        get(housePlacementMode) &&
        !get(cameraRotationEnabled)
      ) {
        event.preventDefault()
        event.stopImmediatePropagation()
        rotateHousePlacement()
        updatePreview()
        return
      }
      if (
        event.code !== 'Escape' ||
        (!get(housePlacementMode) && !get(houseDemolitionMode))
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      stopHouseInteraction()
      clearDemolitionTarget()
    }
    canvas.addEventListener('pointermove', move)
    canvas.addEventListener('pointerleave', leave)
    canvas.addEventListener('mousedown', click, true)
    window.addEventListener('keydown', keydown, true)
    return () => {
      canvas.removeEventListener('pointermove', move)
      canvas.removeEventListener('pointerleave', leave)
      canvas.removeEventListener('mousedown', click, true)
      window.removeEventListener('keydown', keydown, true)
    }
  })

  let elapsed = 0
  useTask((delta) => {
    group.visible = get(currentDungeonDepth) < 1
    if (
      (get(housePlacementMode) || get(houseDemolitionMode)) &&
      (!player ||
        player.health <= 0 ||
        get(playerVisualFloorLevel) !== 0 ||
        get(currentDungeonDepth) > 0 ||
        get(mapEditorMode) ||
        get(housingEditorMode) ||
        get(playerTrade))
    ) {
      stopHouseInteraction()
      clearDemolitionTarget()
      return
    }
    elapsed += delta
    if (elapsed >= 0.05) {
      elapsed = 0
      updatePreview()
      updateDemolitionTarget()
    }
  })

  onMount(() => () => {
    unsubscribe()
    disposePreview()
    stopHouseInteraction()
    demolitionHelper.geometry.dispose()
    ;(demolitionHelper.material as THREE.Material).dispose()
  })
</script>

<T is={group} />
