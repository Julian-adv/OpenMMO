<script lang="ts">
  import * as THREE from 'three'
  import { T } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import {
    editorTool,
    currentObjectData,
    objectCatalog,
    selectedObjectPlacementId,
    objectPreviewPos,
    objectRotation,
    selectedObjectType,
  } from '../../stores/editorStore'
  import type {
    EditorTool,
    ObjectDef,
    ObjectPlacement,
  } from '../../stores/editorStore'
  import { playerDebugInfo } from '../../stores/debugStore'
  import type { PlayerDebugInfo } from '../../stores/debugStore'
  import { mapEditorMode } from '../../stores/debugStore'
  import { tileToRegion } from '../../terrain/terrain-constants'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { objectManager } from '../../managers/objectManager'
  import { bridgeManager } from '../../managers/bridgeManager'
  import { playerFloorLevel, playerInsideHouseId } from '../../stores/housingStore'
  import { housingManager } from '../../managers/housingManager'
  import { loadGLB } from '../../utils/gltfCache'
  import type { Unsubscriber } from 'svelte/store'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'

  const HIGHLIGHT_COLOR = new THREE.Color(0x44ccff)
  const PREVIEW_OPACITY = 0.5
  const SELECTION_OPACITY = 0.9
  const SELECTION_RENDER_ORDER = 999
  const GHOST_OPACITY = 0.3

  let tool = $state<EditorTool>('height')
  let placements = $state<ObjectPlacement[]>([])
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
    currentObjectData.subscribe((v) => (placements = v.placements)),
    objectCatalog.subscribe((v) => (catalogLength = v.length)),
    selectedObjectPlacementId.subscribe((v) => (selectedId = v)),
    objectPreviewPos.subscribe((v) => (previewPos = v)),
    objectRotation.subscribe((v) => (rotation = v)),
    selectedObjectType.subscribe((v) => (selectedType = v)),
    playerDebugInfo.subscribe((v) => (debugInfo = v)),
    mapEditorMode.subscribe((v) => (isEditorMode = v)),
    playerFloorLevel.subscribe((v) => (currentFloor = v)),
    playerInsideHouseId.subscribe((v) => (currentHouseId = v)),
  ]
  onDestroy(() => unsubs.forEach((u) => u()))

  let lastLoadedRegion = { rx: NaN, rz: NaN }

  async function loadRegionObject(rx: number, rz: number) {
    if (rx === lastLoadedRegion.rx && rz === lastLoadedRegion.rz) return
    lastLoadedRegion = { rx, rz }

    if (catalogLength === 0) {
      const cat = await objectManager.fetchCatalog()
      objectCatalog.set(cat)
      catalogById = new Map(cat.map((d) => [d.id, d]))
    }

    const data = await objectManager.fetchObject(rx, rz)
    currentObjectData.set(data)
    bridgeManager.syncRegion(data.placements, catalogById)
  }

  $effect(() => {
    if (!debugInfo) return
    const tileX = Math.round(debugInfo.position.x / TERRAIN_TILE_SIZE)
    const tileZ = Math.round(debugInfo.position.z / TERRAIN_TILE_SIZE)
    const rx = tileToRegion(tileX)
    const rz = tileToRegion(tileZ)
    loadRegionObject(rx, rz)
  })

  const modelCache = new SvelteMap<string, THREE.Group>()
  const modelBounds = new SvelteMap<string, { center: THREE.Vector3; size: THREE.Vector3 }>()
  const loadingModels = new SvelteSet<string>()
  let catalogById = new Map<string, ObjectDef>()

  async function getModel(objectId: string): Promise<THREE.Group | null> {
    if (modelCache.has(objectId)) return modelCache.get(objectId)!
    if (loadingModels.has(objectId)) return null

    const def = catalogById.get(objectId)
    if (!def) return null

    loadingModels.add(objectId)
    try {
      const gltf = await loadGLB(`/models/objects/${def.model}`)
      const model = gltf.scene.clone()
      model.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.castShadow = true
        }
      })
      const box = new THREE.Box3().setFromObject(model)
      const center = new THREE.Vector3()
      const size = new THREE.Vector3()
      box.getCenter(center)
      box.getSize(size)
      modelBounds.set(objectId, { center, size })
      modelCache.set(objectId, model)
      lastBuildKey = ''
      rebuild()
      return model
    } finally {
      loadingModels.delete(objectId)
    }
  }

  let group = new THREE.Group()
  group.name = 'object-overlay'

  let previewGroup: THREE.Group | null = null
  let previewType: string | null = null

  function disposeClonedMaterials(obj: THREE.Object3D) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh && child.material) {
        child.material.dispose()
      } else if (child instanceof THREE.LineSegments) {
        child.geometry.dispose()
        ;(child.material as THREE.Material).dispose()
      }
    })
  }

  function createSelectionBox(
    center: THREE.Vector3,
    size: THREE.Vector3
  ): THREE.LineSegments {
    const box = new THREE.BoxGeometry(size.x, size.y, size.z)
    const geo = new THREE.EdgesGeometry(box)
    box.dispose()
    const mat = new THREE.LineBasicMaterial({
      color: HIGHLIGHT_COLOR,
      depthTest: false,
      transparent: true,
      opacity: SELECTION_OPACITY,
    })
    const lines = new THREE.LineSegments(geo, mat)
    lines.position.copy(center)
    lines.renderOrder = SELECTION_RENDER_ORDER
    return lines
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

let lastBuildKey = ''
  const isEditing = () => isEditorMode && tool === 'object'

  function buildKey(p: ObjectPlacement[]): string {
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
        const bounds = modelBounds.get(p.type)
        if (bounds) {
          clone.add(createSelectionBox(bounds.center, bounds.size))
        }
      }
      clone.userData.objectId = p.id
      clone.userData.objectType = p.type
      const catDef = catalogById.get(p.type)
      if (catDef?.interaction) {
        clone.userData.objectInteraction = catDef.interaction
        clone.userData.objectInteractOffset = catDef.interactOffset
      }
      if (catDef?.kind) {
        clone.userData.objectKind = catDef.kind
      }
      // Per-instance material clone so the ghost toggle doesn't leak across placements.
      if (catDef?.kind === 'bridge') {
        clone.traverse((o) => {
          if (o instanceof THREE.Mesh && o.material) {
            o.material = (o.material as THREE.Material).clone()
          }
        })
      }
      group.add(clone)
    }
    // Fresh clones start opaque; the $effect will re-apply ghost next frame
    // if the player is still under a bridge.
    ghostBridgeId = null
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

  let ghostBridgeId: number | null = null

  function applyBridgeGhost(placementId: number, ghost: boolean) {
    for (const child of group.children) {
      if (child.userData.objectId !== placementId) continue
      child.traverse((o) => {
        if (!(o instanceof THREE.Mesh)) return
        const m = o.material as THREE.Material
        m.transparent = ghost
        m.opacity = ghost ? GHOST_OPACITY : 1
        m.depthWrite = !ghost
        // Toggling `transparent` changes blend state — without needsUpdate
        // the shader isn't recompiled and opacity is silently ignored.
        m.needsUpdate = true
        // Draw after the river ribbon (renderOrder=1) so alpha-blended deck
        // sorts above water consistently.
        o.renderOrder = ghost ? 2 : 0
      })
    }
  }

  $effect(() => {
    if (!debugInfo) return
    const id = bridgeManager.findOccludingBridgeId(
      debugInfo.position.x,
      debugInfo.position.y,
      debugInfo.position.z
    )
    if (id === ghostBridgeId) return
    if (ghostBridgeId !== null) applyBridgeGhost(ghostBridgeId, false)
    if (id !== null) applyBridgeGhost(id, true)
    ghostBridgeId = id
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
