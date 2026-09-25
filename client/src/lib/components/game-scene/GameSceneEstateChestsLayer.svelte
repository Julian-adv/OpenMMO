<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount, untrack } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { get } from 'svelte/store'
  import * as THREE from 'three'
  import GameSceneEstateFurniturePlacementLayer from './GameSceneEstateFurniturePlacementLayer.svelte'
  import {
    loadEstateFurnitureModel,
    furnitureModelDefinition,
    estateFurnitureInteractionData,
  } from '../../utils/estateFurnitureModels'
  import { buildShopSignText, getShopSignStyle } from '../../utils/shop-sign'
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
  import {
    TorchFireParticles,
    CampfireFireParticles,
    type FireParticles,
  } from '../../effects/fire-particles'
  import { networkManager } from '../../network/socket'
  import { playPropSound } from '../../managers/sfxManager'
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
  let lastChests: Map<number, EstateChest> | null = null
  let lastWrapX = Infinity
  let lastFloorLevel = Infinity
  let lastMovingFurnitureId: number | undefined
  let placementLayer = $state<GameSceneEstateFurniturePlacementLayer>()
  let disposed = false
  let lastOpened: number | null = null
  const visuals = new SvelteMap<number, THREE.Group>()
  const mixers = new SvelteMap<number, THREE.AnimationMixer>()
  const loadingModels = new SvelteSet<string>()
  const failedModels = new SvelteSet<string>()
  const signTexts = new SvelteMap<string, THREE.Mesh>()
  const savedPositionMaterials: THREE.Material[] = []
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

  function makeVisual(chest: EstateChest) {
    const source = sources.get(chest.item_def_id)
    if (!source || !player) return
    const moving = chest.id === movingFurnitureId
    const visual = source.scene.clone(true)
    const definition = getEstateStorageDef(chest.item_def_id)
    const model = furnitureModelDefinition(definition?.modelId)
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
    visual.userData.estateChestId = chest.id
    Object.assign(visual.userData, estateFurnitureInteractionData(chest))
    visual.position.set(
      unwrapWorldXNear(player.position.x, chest.position.x),
      chest.position.y,
      chest.position.z
    )
    visual.rotation.y = THREE.MathUtils.degToRad(chest.rotation_deg)
    if (model?.fire && !moving) {
      const fire =
        model.fireKind === 'torch'
          ? new TorchFireParticles()
          : new CampfireFireParticles()
      const position = new THREE.Vector3(
        model.fire.x,
        model.fire.y,
        model.fire.z
      )
        .applyEuler(visual.rotation)
        .add(visual.position)
      fire.setOrigin(position)
      group.add(fire.group)
      fires.push(fire)
      ;(model.fireKind === 'torch' ? torchPositions : firePositions).push(
        position
      )
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
            savedPositionMaterials.push(material)
            return material
          })
          object.material = Array.isArray(object.material)
            ? materials
            : materials[0]
        }
      }
    })
    chestGroup.add(visual)
    visuals.set(chest.id, visual)
    if (source.clips.length)
      mixers.set(chest.id, new THREE.AnimationMixer(visual))
  }

  function clearSavedPositionMaterials() {
    for (const material of savedPositionMaterials) material.dispose()
    savedPositionMaterials.length = 0
  }

  function rebuild() {
    const current = get(estateChests)
    const floorLevel = get(playerVisualFloorLevel)
    if (
      sources.size === 0 ||
      !player ||
      (current === lastChests &&
        floorLevel === lastFloorLevel &&
        movingFurnitureId === lastMovingFurnitureId &&
        Math.abs(player.position.x - lastWrapX) < 100)
    )
      return
    lastChests = current
    lastWrapX = player.position.x
    lastFloorLevel = floorLevel
    lastMovingFurnitureId = movingFurnitureId
    chestGroup.clear()
    clearSavedPositionMaterials()
    for (const fire of fires) {
      group.remove(fire.group)
      fire.dispose()
    }
    fires.length = 0
    firePositions.length = 0
    torchPositions.length = 0
    visuals.clear()
    mixers.clear()
    for (const chest of current.values()) {
      if (chest.floor_level === floorLevel) makeVisual(chest)
    }
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
      canvas.removeEventListener('mousedown', click, true)
      window.removeEventListener('keydown', escape, true)
      clearSavedPositionMaterials()
      for (const fire of fires) fire.dispose()
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
