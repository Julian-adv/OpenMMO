<script lang="ts">
  import { useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import type { LocalPlayer } from '../../stores/gameStore'
  import {
    cameraRotationEnabled,
    mapEditorMode,
    housingEditorMode,
  } from '../../stores/debugStore'
  import {
    furnitureShop,
    furnitureBasket,
    furnitureShopHover,
    furniturePurchasePending,
    displayProduct,
    addFurnitureToBasket,
    clearFurnitureBasket,
  } from '../../stores/furnitureShopStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { networkManager } from '../../network/socket'

  let {
    player,
    getObjectGroup,
  }: { player: LocalPlayer | null; getObjectGroup: () => THREE.Group | null } =
    $props()
  const { camera, renderer } = useThrelte()
  const raycaster = new THREE.Raycaster()
  const pointer = new THREE.Vector2()
  let cursor: { x: number; y: number } | null = null

  function shoppingAllowed() {
    return (
      player &&
      player.health > 0 &&
      player.position.x >= furnitureShop.bounds[0] - 4 &&
      player.position.x <= furnitureShop.bounds[2] + 3 &&
      player.position.z >= furnitureShop.bounds[1] - 3 &&
      player.position.z <= furnitureShop.bounds[3] + 3 &&
      player.position.y >= 0 &&
      player.position.y <= 4 &&
      get(playerVisualFloorLevel) === 0 &&
      get(currentDungeonDepth) < 1 &&
      !get(mapEditorMode) &&
      !get(housingEditorMode)
    )
  }

  function updateHover() {
    let target: {
      displayId: number
      product: (typeof furnitureShop.products)[number]
    } | null = null
    const group = getObjectGroup()
    if (shoppingAllowed() && cursor && group && !get(cameraRotationEnabled)) {
      const rect = renderer.domElement.getBoundingClientRect()
      pointer.set(
        ((cursor.x - rect.left) / rect.width) * 2 - 1,
        (-(cursor.y - rect.top) / rect.height) * 2 + 1
      )
      raycaster.setFromCamera(pointer, get(camera))
      const hit = raycaster.intersectObject(group, true)[0]
      let object: THREE.Object3D | null = hit?.object ?? null
      while (object && typeof object.userData.objectId !== 'number')
        object = object.parent
      if (object && player) {
        const product = displayProduct(object.userData.objectId)
        const world = object.getWorldPosition(new THREE.Vector3())
        if (
          product &&
          product.objectType === object.userData.objectType &&
          world.x >= furnitureShop.bounds[0] &&
          world.x <= furnitureShop.bounds[2] &&
          world.z >= furnitureShop.bounds[1] &&
          world.z <= furnitureShop.bounds[3] &&
          Math.hypot(
            player.position.x - world.x,
            player.position.z - world.z
          ) <= 3
        ) {
          target = { displayId: object.userData.objectId, product }
        }
      }
    }
    if (target?.displayId !== get(furnitureShopHover)?.displayId)
      furnitureShopHover.set(target)
  }

  onMount(() => {
    const canvas = renderer.domElement
    const move = (event: PointerEvent) => {
      cursor = { x: event.clientX, y: event.clientY }
    }
    const leave = () => {
      cursor = null
      furnitureShopHover.set(null)
    }
    const click = (event: MouseEvent) => {
      if (event.button !== 0 || get(furniturePurchasePending)) return
      cursor = { x: event.clientX, y: event.clientY }
      updateHover()
      const target = get(furnitureShopHover)
      if (!target) return
      event.preventDefault()
      event.stopImmediatePropagation()
      if (addFurnitureToBasket(target.displayId))
        networkManager.sendSelectFurnitureDisplay(target.displayId)
    }
    canvas.addEventListener('pointermove', move)
    canvas.addEventListener('pointerleave', leave)
    canvas.addEventListener('mousedown', click, true)
    return () => {
      canvas.removeEventListener('pointermove', move)
      canvas.removeEventListener('pointerleave', leave)
      canvas.removeEventListener('mousedown', click, true)
      leave()
    }
  })

  useTask(() => {
    const position = player?.position
    if (get(furnitureBasket).length && !get(furniturePurchasePending)) {
      if (
        !shoppingAllowed() ||
        !position ||
        position.x < furnitureShop.bounds[0] - 4 ||
        position.x > furnitureShop.bounds[2] + 2 ||
        position.z < furnitureShop.bounds[1] - 2 ||
        position.z > furnitureShop.bounds[3] + 2
      )
        clearFurnitureBasket()
    }
    updateHover()
  })
</script>
