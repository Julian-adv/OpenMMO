<script lang="ts">
  import * as THREE from 'three'
  import { T } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import {
    editorTool,
    currentFurnitureData,
    furnitureCatalog,
    selectedFurniturePlacementId,
    furniturePreviewPos,
    furnitureRotation,
    selectedFurnitureType,
  } from '../../stores/editorStore'
  import type {
    EditorTool,
    FurnitureDef,
    FurniturePlacement,
  } from '../../stores/editorStore'
  import { playerDebugInfo } from '../../stores/debugStore'
  import type { PlayerDebugInfo } from '../../stores/debugStore'
  import { mapEditorMode } from '../../stores/debugStore'
  import { tileToRegion } from '../../terrain/terrain-constants'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { furnitureManager } from '../../managers/furnitureManager'
  import { playerFloorLevel, playerInsideHouseId } from '../../stores/housingStore'
  import { housingManager } from '../../managers/housingManager'
  import { loadGLB } from '../../utils/gltfCache'
  import type { Unsubscriber } from 'svelte/store'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'

  const HIGHLIGHT_COLOR = new THREE.Color(0x44ccff)
  const PREVIEW_OPACITY = 0.5

  let tool = $state<EditorTool>('height')
  let placements = $state<FurniturePlacement[]>([])
  let catalogLength = $state(0)
  let selectedId = $state<number | null>(null)
  let previewPos = $state<{ x: number; y: number; z: number } | null>(null)
  let rotation = $state(0)
  let selectedType = $state<string | null>(null)
  let debugInfo = $state<PlayerDebugInfo | null>(null)
  let isEditorMode = $state(false)
  let currentFloor = $state(-1)
  let currentHouseId = $state<string | null>(null)

  const unsubs: Unsubscriber[] = [
    editorTool.subscribe((v) => (tool = v)),
    currentFurnitureData.subscribe((v) => (placements = v.placements)),
    furnitureCatalog.subscribe((v) => (catalogLength = v.length)),
    selectedFurniturePlacementId.subscribe((v) => (selectedId = v)),
    furniturePreviewPos.subscribe((v) => (previewPos = v)),
    furnitureRotation.subscribe((v) => (rotation = v)),
    selectedFurnitureType.subscribe((v) => (selectedType = v)),
    playerDebugInfo.subscribe((v) => (debugInfo = v)),
    mapEditorMode.subscribe((v) => (isEditorMode = v)),
    playerFloorLevel.subscribe((v) => (currentFloor = v)),
    playerInsideHouseId.subscribe((v) => (currentHouseId = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  let lastLoadedRegion = { rx: NaN, rz: NaN }

  async function loadRegionFurniture(rx: number, rz: number) {
    if (rx === lastLoadedRegion.rx && rz === lastLoadedRegion.rz) return
    lastLoadedRegion = { rx, rz }

    if (catalogLength === 0) {
      const cat = await furnitureManager.fetchCatalog()
      furnitureCatalog.set(cat)
      catalogById = new Map(cat.map((d) => [d.id, d]))
    }

    const data = await furnitureManager.fetchFurniture(rx, rz)
    currentFurnitureData.set(data)
  }

  $effect(() => {
    if (!debugInfo) return
    const tileX = Math.round(debugInfo.position.x / TERRAIN_TILE_SIZE)
    const tileZ = Math.round(debugInfo.position.z / TERRAIN_TILE_SIZE)
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    loadRegionFurniture(rx, rz)
  })

  const modelCache = new SvelteMap<string, THREE.Group>()
  const loadingModels = new SvelteSet<string>()
  let catalogById = new Map<string, FurnitureDef>()

  async function getModel(furnitureId: string): Promise<THREE.Group | null> {
    if (modelCache.has(furnitureId)) return modelCache.get(furnitureId)!
    if (loadingModels.has(furnitureId)) return null

    const def = catalogById.get(furnitureId)
    if (!def) return null

    loadingModels.add(furnitureId)
    try {
      const gltf = await loadGLB(`/models/furniture/${def.model}`)
      const model = gltf.scene.clone()
      model.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.castShadow = true
        }
      })
      modelCache.set(furnitureId, model)
      lastBuildKey = ''
      rebuild()
      return model
    } finally {
      loadingModels.delete(furnitureId)
    }
  }

  let group = new THREE.Group()
  group.name = 'furniture-overlay'

  let previewGroup: THREE.Group | null = null
  let previewType: string | null = null

  function disposeClonedMaterials(obj: THREE.Object3D) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh && child.material) {
        child.material.dispose()
      }
    })
  }

  function setPreviewMaterial(obj: THREE.Object3D, opacity: number) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        child.material = (child.material as THREE.Material).clone()
        ;(child.material as THREE.Material).transparent = true
        ;(child.material as THREE.Material).opacity = opacity
        ;(child.material as THREE.Material).depthWrite = false
      }
    })
  }

  function applyHighlight(obj: THREE.Object3D) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        const mat = (child.material as THREE.MeshStandardMaterial).clone()
        mat.emissive = HIGHLIGHT_COLOR
        mat.emissiveIntensity = 0.3
        child.material = mat
      }
    })
  }

  let lastBuildKey = ''
  const isEditing = () => isEditorMode && tool === 'furniture'

  function buildKey(p: FurniturePlacement[]): string {
    return p.map((v) => `${v.id}:${v.type}:${v.x}:${v.y}:${v.z}:${v.rotation}`).join('|')
  }

  function rebuild() {
    const visibleFloor = Math.max(0, currentFloor)
    const key = buildKey(placements) + `|sel:${isEditing() ? selectedId : ''}|fl:${visibleFloor}|h:${currentHouseId ?? ''}`
    if (key === lastBuildKey) return
    lastBuildKey = key

    for (let i = group.children.length - 1; i >= 0; i--) {
      const child = group.children[i]
      if (child !== previewGroup) {
        disposeClonedMaterials(child)
        group.remove(child)
      }
    }

    for (const p of placements) {
      if (p.floorLevel !== visibleFloor) continue
      const pHouse = housingManager.findHouseAtPoint(p.x, p.y, p.z)
      if (currentHouseId) {
        if (pHouse?.id !== currentHouseId) continue
      } else {
        if (pHouse != null) continue
      }
      const template = modelCache.get(p.type)
      if (!template) {
        getModel(p.type)
        continue
      }
      const clone = template.clone()
      clone.position.set(p.x, p.y, p.z)
      clone.rotation.y = (p.rotation * Math.PI) / 180
      if (isEditing() && p.id === selectedId) {
        applyHighlight(clone)
      }
      clone.userData.furnitureId = p.id
      clone.userData.furnitureType = p.type
      const catDef = catalogById.get(p.type)
      if (catDef?.interaction) {
        clone.userData.furnitureInteraction = catDef.interaction
        clone.userData.furnitureInteractOffset = catDef.interactOffset
      }
      group.add(clone)
    }
  }

  function updatePreview() {
    if (!isEditing() || !previewPos || !selectedType) {
      if (previewGroup) {
        disposeClonedMaterials(previewGroup)
        group.remove(previewGroup)
        previewGroup = null
        previewType = null
      }
      return
    }

    if (previewType !== selectedType) {
      if (previewGroup) {
        disposeClonedMaterials(previewGroup)
        group.remove(previewGroup)
      }
      const template = modelCache.get(selectedType)
      if (!template) {
        getModel(selectedType)
        previewGroup = null
        previewType = null
        return
      }
      previewGroup = template.clone()
      setPreviewMaterial(previewGroup, PREVIEW_OPACITY)
      previewType = selectedType
    }

    if (previewGroup) {
      previewGroup.position.set(previewPos.x, previewPos.y, previewPos.z)
      previewGroup.rotation.y = (rotation * Math.PI) / 180
      if (!previewGroup.parent) {
        group.add(previewGroup)
      }
    }
  }

  $effect(() => {
    void placements
    void selectedId
    void catalogLength
    void tool
    void isEditorMode
    void currentFloor
    rebuild()
  })

  $effect(() => {
    void previewPos
    void rotation
    void selectedType
    void tool
    void isEditorMode
    updatePreview()
  })

  export function getGroup(): THREE.Group {
    return group
  }

  onDestroy(() => {
    for (const child of [...group.children]) {
      disposeClonedMaterials(child)
    }
    group.clear()
    modelCache.clear()
  })
</script>

<T is={group} />
