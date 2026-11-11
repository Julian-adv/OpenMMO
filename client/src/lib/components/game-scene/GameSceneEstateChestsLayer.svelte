<script lang="ts">
  import { getObjectDef } from '../../data/objectCatalog'
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount, untrack } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import GameSceneEstateFurniturePlacementLayer from './GameSceneEstateFurniturePlacementLayer.svelte'
  import {
    loadEstateFurnitureModel,
    estateFurnitureInteractionData,
    estateFurnitureRenderY,
  } from '../../utils/estateFurnitureModels'
  import { buildShopSignText, getShopSignStyle } from '../../utils/shop-sign'
  import { mountNoticePapers } from '../../utils/notice-board'
  import { createSelectionBox } from '../../utils/objectSelectionBox'
  import {
    estateFurnitureSelectionMode,
    estateFurnitureEditorActive,
    estateFurnitureCatalogOpen,
    selectedEstateFurniture,
    selectEstateFurniture,
    beginEstateFurniturePlacementSave,
  } from '../../stores/estateFurniturePlacementStore'
  import { isTypingTarget } from '../../utils/dom'
  import { disposeObjectTree } from '../../utils/disposeObjectTree'
  import {
    TorchFireParticles,
    CampfireFireParticles,
    type FireParticles,
  } from '../../effects/fire-particles'
  import { networkManager } from '../../network/socket'
  import { playPropSound } from '../../managers/sfxManager'
  import { housingManager } from '../../managers/housingManager'
  import {
    estateChests,
    estateChestMode,
    estateChestPending,
    estateChestError,
    openEstateChest,
    stopEstateChestMode,
  } from '../../stores/estateStorageStore'
  import {
    mapEditorMode,
    housingEditorMode,
    cameraRotationEnabled,
  } from '../../stores/debugStore'
  import { currentDungeonDepth } from '../../stores/dungeonStore'
  import { playerVisualFloorLevel } from '../../stores/housingStore'
  import { unwrapWorldXNear } from '../../terrain/world-wrap'
  import type { LocalPlayer } from '../../stores/gameStore'
  import type { EstateChest } from '../../network/networkTypes'
  import type { HouseData } from '../../types/housing'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { EstateFurniturePlacement } from '../../terrain/estatePlacement'
  import {
    estateStorageDefs,
    getEstateStorageDef,
  } from '../../data/estateFurnitureDefs'

  let {
    heightManager,
    terrainMeshes,
    housingGroup,
    player,
  }: {
    heightManager: TerrainHeightManager
    terrainMeshes: (THREE.Mesh | undefined)[]
    housingGroup?: THREE.Group
    player: LocalPlayer | null
  } = $props()

  const { camera, renderer } = useThrelte()
  const group = new THREE.Group()
  const chestGroup = new THREE.Group()
  const raycaster = new THREE.Raycaster()
  const pointer = new THREE.Vector2()
  group.add(chestGroup)

  const sources = new SvelteMap<
    string,
    { scene: THREE.Group; clips: THREE.AnimationClip[] }
  >()
  interface FurnitureEntry {
    chest: EstateChest
    moving: boolean
    visual: THREE.Group
    fire?: FireParticles
    firePosition?: THREE.Vector3
    torch: boolean
    materials: THREE.Material[]
  }

  let lastChests: Map<number, EstateChest> | null = null
  let lastWrapX = Infinity
  let lastFloorLevel = Infinity
  let lastMovingFurnitureId: number | undefined
  let housesChanged = false
  let placementLayer = $state<GameSceneEstateFurniturePlacementLayer>()
  let disposed = false
  let lastOpened: number | null = null
  const entries = new SvelteMap<number, FurnitureEntry>()
  const visuals = new SvelteMap<number, THREE.Group>()
  const mixers = new SvelteMap<number, THREE.AnimationMixer>()
  const loadingModels = new SvelteSet<string>()
  const failedModels = new SvelteSet<string>()
  const signTexts = new SvelteMap<string, THREE.Mesh>()
  const fires: FireParticles[] = []
  const firePositions: THREE.Vector3[] = []
  const torchPositions: THREE.Vector3[] = []
  export function getFirePositions() {
    const preview = placementLayer?.getFirePositions()
    return preview?.length ? [...firePositions, ...preview] : firePositions
  }
  export function getTorchPositions() {
    const preview = placementLayer?.getTorchPositions()
    return preview?.length ? [...torchPositions, ...preview] : torchPositions
  }
  export function getGroup() {
    return chestGroup
  }
  const placementDefinition = $derived(
    getEstateStorageDef($estateChestMode?.item_def_id)
  )
  const placementObstacles = $derived(
    [...$estateChests.values()]
      .filter(
        (chest) =>
          chest.floor_level === $playerVisualFloorLevel &&
          chest.id !== movingFurnitureId
      )
      .flatMap((chest) => {
        const definition = getEstateStorageDef(chest.item_def_id)
        return definition?.solid
          ? [
              {
                x: chest.position.x,
                z: chest.position.z,
                rotationDeg: chest.rotation_deg,
                footprint: definition.footprint,
              },
            ]
          : []
      })
  )
  const movingFurnitureId = $derived(
    $estateChestMode?.kind === 'move'
      ? $estateChestMode.furniture.id
      : undefined
  )

  function setPointer(clientX: number, clientY: number) {
    const rect = renderer.domElement.getBoundingClientRect()
    pointer.set(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(pointer, get(camera))
  }

  function chestFromHit(hit: THREE.Intersection): EstateChest | undefined {
    let object: THREE.Object3D | null = hit.object
    while (object) {
      if (typeof object.userData.estateChestId === 'number')
        return get(estateChests).get(object.userData.estateChestId)
      object = object.parent
    }
  }

  function makeVisual(chest: EstateChest, houses: HouseData[]) {
    const source = sources.get(chest.item_def_id)
    if (!source || !player) return
    const moving = chest.id === movingFurnitureId
    const visual = source.scene.clone(true)
    const definition = getEstateStorageDef(chest.item_def_id)
    const model = getObjectDef(definition?.modelId)
    if (chest.text && model?.procedural === 'shopSign') {
      const style = getShopSignStyle(model.shopSignStyle)
      const key = `${chest.item_def_id}:${chest.text}`
      let text = signTexts.get(key)
      if (!text) {
        text = buildShopSignText(chest.text, style.board, style.text)
        signTexts.set(key, text)
      }
      visual.add(text.clone())
    }
    if (chest.text && model?.procedural !== 'shopSign')
      visual.userData.objectText = chest.text
    if (model?.noticeBoard && !moving)
      mountNoticePapers(visual, { Estate: { furniture_id: chest.id } })
    visual.userData.estateChestId = chest.id
    Object.assign(visual.userData, estateFurnitureInteractionData(chest))
    visual.position.set(
      unwrapWorldXNear(player.position.x, chest.position.x),
      estateFurnitureRenderY(chest, houses),
      chest.position.z
    )
    visual.rotation.y = THREE.MathUtils.degToRad(chest.rotation_deg)
    const entry: FurnitureEntry = {
      chest,
      moving,
      visual,
      torch: model?.fireKind === 'torch',
      materials: [],
    }
    if (model?.fire && !moving) {
      entry.fire = entry.torch
        ? new TorchFireParticles()
        : new CampfireFireParticles()
      entry.firePosition = new THREE.Vector3(
        model.fire.x,
        model.fire.y,
        model.fire.z
      )
        .applyEuler(visual.rotation)
        .add(visual.position)
      entry.fire.setOrigin(entry.firePosition)
      group.add(entry.fire.group)
    }
    visual.traverse((object) => {
      if (object instanceof THREE.Mesh) {
        const shadows = !moving && !object.userData.isSignText
        object.castShadow = shadows
        object.receiveShadow = shadows
        if (moving) {
          const sources = Array.isArray(object.material)
            ? object.material
            : [object.material]
          const materials = sources.map((source) => {
            const material = source.clone()
            material.transparent = true
            material.opacity *= 0.35
            material.depthWrite = false
            entry.materials.push(material)
            return material
          })
          object.material = Array.isArray(object.material)
            ? materials
            : materials[0]
        }
      }
    })
    chestGroup.add(visual)
    entries.set(chest.id, entry)
    visuals.set(chest.id, visual)
    if (source.clips.length)
      mixers.set(chest.id, new THREE.AnimationMixer(visual))
  }

  function removeVisual(id: number) {
    const entry = entries.get(id)
    if (!entry) return
    chestGroup.remove(entry.visual)
    disposeObjectTree(entry.visual)
    for (const material of entry.materials) material.dispose()
    if (entry.fire) {
      group.remove(entry.fire.group)
      entry.fire.dispose()
    }
    entries.delete(id)
    visuals.delete(id)
    mixers.delete(id)
  }

  function syncFires() {
    fires.length = 0
    firePositions.length = 0
    torchPositions.length = 0
    for (const entry of entries.values()) {
      if (!entry.fire || !entry.firePosition) continue
      fires.push(entry.fire)
      ;(entry.torch ? torchPositions : firePositions).push(entry.firePosition)
    }
  }

  // Only touch changed furniture: recreated meshes skip frames while their pipelines warm up.
  function rebuild() {
    const current = get(estateChests)
    const floorLevel = get(playerVisualFloorLevel)
    const rewrap = player && Math.abs(player.position.x - lastWrapX) >= 100
    if (
      sources.size === 0 ||
      !player ||
      (current === lastChests &&
        floorLevel === lastFloorLevel &&
        movingFurnitureId === lastMovingFurnitureId &&
        !housesChanged &&
        !rewrap)
    )
      return
    const relocate = housesChanged || rewrap
    lastChests = current
    lastWrapX = player.position.x
    lastFloorLevel = floorLevel
    lastMovingFurnitureId = movingFurnitureId
    housesChanged = false
    const houses = housingManager.getAllHouses()
    for (const [id, entry] of entries) {
      const chest = current.get(id)
      if (
        !chest ||
        chest !== entry.chest ||
        chest.floor_level !== floorLevel ||
        (id === movingFurnitureId) !== entry.moving ||
        (relocate &&
          (entry.visual.position.x !==
            unwrapWorldXNear(player.position.x, chest.position.x) ||
            entry.visual.position.y !== estateFurnitureRenderY(chest, houses)))
      )
        removeVisual(id)
    }
    for (const chest of current.values()) {
      if (chest.floor_level === floorLevel && !entries.has(chest.id))
        makeVisual(chest, houses)
    }
    syncFires()
  }

  function place(placement: EstateFurniturePlacement) {
    const mode = get(estateChestMode)
    if (!mode || !getEstateStorageDef(mode.item_def_id)) return
    if (!beginEstateFurniturePlacementSave()) return
    if (mode.kind === 'move') {
      networkManager.sendMoveEstateFurniture(
        mode.furniture.id,
        mode.furniture.revision,
        placement.position,
        placement.rotationDeg,
        placement.floorLevel
      )
    } else {
      networkManager.sendPlaceEstateChest(
        mode.instance_id,
        placement.position,
        placement.rotationDeg,
        placement.floorLevel
      )
    }
  }

  function playOpen(chestId: number) {
    const chest = get(estateChests).get(chestId)
    const clips = chest ? sources.get(chest.item_def_id)?.clips : undefined
    const clip =
      clips?.find((entry) => entry.name === 'ChestOpen') ?? clips?.[0]
    const mixer = mixers.get(chestId)
    if (!clip || !mixer) return
    const action = mixer.clipAction(clip)
    action.reset()
    action.setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.timeScale = 1
    action.play()
    playPropSound('chestOpen')
  }

  function playClose(chestId: number) {
    const chest = get(estateChests).get(chestId)
    const clips = chest ? sources.get(chest.item_def_id)?.clips : undefined
    const clip =
      clips?.find((entry) => entry.name === 'ChestOpen') ?? clips?.[0]
    const mixer = mixers.get(chestId)
    if (!clip || !mixer) return
    const action = mixer.clipAction(clip)
    action.enabled = true
    action.paused = false
    action.setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.timeScale = -1
    if (action.time <= 0) action.time = clip.duration
    action.play()
  }

  function loadModel(itemDefId: string) {
    const definition = estateStorageDefs.get(itemDefId)
    if (
      !definition ||
      sources.has(itemDefId) ||
      loadingModels.has(itemDefId) ||
      failedModels.has(itemDefId)
    )
      return
    loadingModels.add(itemDefId)
    loadEstateFurnitureModel(definition)
      .then((gltf) => {
        if (disposed) return
        sources.set(definition.itemDefId, {
          scene: gltf.scene,
          clips: gltf.animations,
        })
        lastChests = null
        rebuild()
      })
      .catch((error) => {
        if (disposed) return
        failedModels.add(itemDefId)
        console.error(
          `Failed to load estate furniture ${definition.itemDefId}:`,
          error
        )
        estateChestError.set(
          'Could not load the furniture model. Reload to try again.'
        )
      })
      .finally(() => loadingModels.delete(itemDefId))
  }

  onMount(() => {
    const unsubscribeHouses = housingManager.onHousesChanged(() => {
      housesChanged = true
    })
    const canvas = renderer.domElement
    const click = (event: MouseEvent) => {
      if (
        get(estateChestMode) ||
        !player ||
        event.button !== 0 ||
        get(cameraRotationEnabled) ||
        get(mapEditorMode) ||
        get(housingEditorMode) ||
        get(currentDungeonDepth) > 0
      )
        return
      const selecting = get(estateFurnitureSelectionMode)
      if (selecting) {
        event.preventDefault()
        event.stopImmediatePropagation()
        if (get(estateChestPending)) return
      }
      setPointer(event.clientX, event.clientY)
      const hit = raycaster.intersectObject(chestGroup, true)[0]
      const chest = hit && chestFromHit(hit)
      if (
        !selecting &&
        chest &&
        estateFurnitureInteractionData(chest).objectInteraction
      )
        return
      if (!chest || chest.floor_level !== get(playerVisualFloorLevel)) {
        if (selecting)
          estateChestError.set('Click furniture on the current floor.')
        return
      }
      if (
        !selecting &&
        Math.hypot(
          unwrapWorldXNear(player.position.x, chest.position.x) -
            player.position.x,
          chest.position.z - player.position.z
        ) > 3
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      const definition = getEstateStorageDef(chest.item_def_id)
      if (selecting) {
        openEstateChest.set(null)
        if (selectEstateFurniture(chest))
          networkManager.sendStartEstateFurnitureMove(chest.id)
      } else if (definition?.capacityKg)
        networkManager.sendOpenEstateChest(chest.id)
      else if (getObjectDef(definition?.modelId)?.mailbox)
        networkManager.sendOpenMailbox({ Estate: { furniture_id: chest.id } })
      else if (getObjectDef(definition?.modelId)?.noticeBoard)
        networkManager.sendOpenNoticeBoard({
          Estate: { furniture_id: chest.id },
        })
    }
    const escape = (event: KeyboardEvent) => {
      if (
        event.code !== 'Escape' ||
        isTypingTarget(event.target) ||
        get(estateChestMode) ||
        (!get(estateFurnitureEditorActive) && !get(estateFurnitureCatalogOpen))
      )
        return
      event.preventDefault()
      event.stopImmediatePropagation()
      if (!get(estateChestPending)) stopEstateChestMode()
    }
    canvas.addEventListener('mousedown', click, true)
    window.addEventListener('keydown', escape, true)
    return () => {
      disposed = true
      unsubscribeHouses()
      canvas.removeEventListener('mousedown', click, true)
      window.removeEventListener('keydown', escape, true)
      for (const id of [...entries.keys()]) removeVisual(id)
      for (const text of signTexts.values()) {
        text.geometry.dispose()
        const materials = Array.isArray(text.material)
          ? text.material
          : [text.material]
        for (const material of materials) {
          const map = (material as THREE.MeshBasicMaterial).map
          map?.dispose()
          material.dispose()
        }
      }
      stopEstateChestMode()
    }
  })

  $effect(() => {
    const floorLevel = $playerVisualFloorLevel
    const itemDefIds = new Set(
      [...$estateChests.values()]
        .filter((chest) => chest.floor_level === floorLevel)
        .map((chest) => chest.item_def_id)
    )
    untrack(() => {
      for (const itemDefId of itemDefIds) loadModel(itemDefId)
    })
  })

  $effect(() => {
    const selectedId = $selectedEstateFurniture?.id ?? movingFurnitureId
    if (selectedId === undefined) return
    const furniture = $estateChests.get(selectedId)
    const visual = visuals.get(selectedId)
    const source = furniture ? sources.get(furniture.item_def_id) : undefined
    if (!visual || !source) return
    const bounds = new THREE.Box3().setFromObject(source.scene)
    if (bounds.isEmpty()) return
    const selection = createSelectionBox(
      bounds.getCenter(new THREE.Vector3()),
      bounds.getSize(new THREE.Vector3())
    )
    visual.add(selection)
    return () => {
      selection.removeFromParent()
      selection.geometry.dispose()
      selection.material.dispose()
    }
  })

  useTask((delta) => {
    group.visible = get(currentDungeonDepth) < 1
    rebuild()
    for (const fire of fires) fire.update(delta, get(camera))
    for (const mixer of mixers.values()) mixer.update(delta)
    const opened = get(openEstateChest)?.chest_id ?? null
    if (opened !== lastOpened) {
      if (lastOpened !== null) playClose(lastOpened)
      if (opened !== null) playOpen(opened)
      lastOpened = opened
    }
  })
</script>

<T is={group} />
{#if placementDefinition}
  {#key `${placementDefinition.itemDefId}:${movingFurnitureId ?? 'new'}`}
    <GameSceneEstateFurniturePlacementLayer
      bind:this={placementLayer}
      definition={placementDefinition}
      plots={$estateChestMode?.plots ?? []}
      pending={$estateChestPending}
      {terrainMeshes}
      {housingGroup}
      furnitureGroup={chestGroup}
      ignoreFurnitureId={movingFurnitureId}
      {heightManager}
      {player}
      floorLevel={$playerVisualFloorLevel}
      obstacles={placementObstacles}
      onplace={place}
      oncancel={stopEstateChestMode}
      onerror={(message) => estateChestError.set(message)}
    />
  {/key}
{/if}
