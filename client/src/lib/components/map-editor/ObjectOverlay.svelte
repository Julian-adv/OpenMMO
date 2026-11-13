<script lang="ts">
  import * as THREE from 'three'
  import { T, useLoader, useThrelte } from '@threlte/core'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
  import { getMonsterDef } from '../../data/monsterDefs'
  import { onDestroy, untrack } from 'svelte'
  import {
    editorTool,
    currentObjectData,
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
  import type { Position } from '../../network/networkTypes'
  import { playerDebugInfo } from '../../stores/debugStore'
  import type { PlayerDebugInfo } from '../../stores/debugStore'
  import { mapEditorMode } from '../../stores/debugStore'
  import { tileToRegion } from '../../terrain/terrain-constants'
  import { TERRAIN_TILE_SIZE } from '../game-scene/terrain-utils'
  import { objectManager } from '../../managers/objectManager'
  import type { ObjectClickKind } from '../../managers/inputHandler'
  import { objectDefsById } from '../../data/objectCatalog'
  import { setShelterFootprints } from '../../utils/shelterFootprints'
  import { monsterManager } from '../../managers/monsterManager'
  import { shortestWrappedDeltaX } from '../../terrain/world-wrap'
  import { worldView } from '../../network/worldView'
  import { bridgeManager } from '../../managers/bridgeManager'
  import { furnitureManager } from '../../managers/furnitureManager'
  import {
    playerVisualFloorLevel,
    playerInsideHouseId,
  } from '../../stores/housingStore'
  import { housingManager } from '../../managers/housingManager'
  import { loadGLB } from '../../utils/gltfCache'
  import {
    CampfireFireParticles,
    TorchFireParticles,
    type FireParticles,
  } from '../../effects/fire-particles'
  import {
    DORMANT_SINK_MS,
    DormantWakeEffect,
    wakeBounds,
  } from '../../effects/dormant-wake'
  import {
    SLEEP_RISE_FROM_MS,
    SLEEP_SINK_MS,
    shake,
    sleepRise,
  } from '../../effects/dormant-sleep'
  import { playMonsterSound } from '../../managers/sfxManager'
  import {
    takeFieldBossSleeps,
    takeFieldBossWakes,
    type FieldBossSleep,
    type FieldBossWake,
  } from '../../stores/fieldBossWakeStore'
  import { getObjectModelPath } from '../../utils/modelPaths'
  import { createSelectionBox } from '../../utils/objectSelectionBox'
  import { doorwaysOf, shelterRoomAt } from '../../utils/shelter'
  import { isoCameraOccludesPlayer } from '../../utils/iso-occlusion'
  import { rotatedRectAabb } from '../../utils/objectFootprint'
  import { objectDoorId } from '../../network/doorId'
  import {
    openDoors,
    resetDoorStates,
    setObjectDoorways,
  } from '../../stores/objectDoorStore'
  import {
    buildShopSignBoard,
    buildShopSignText,
    getShopSignStyle,
  } from '../../utils/shop-sign'
  import { buildNoticeBoard, mountNoticePapers } from '../../utils/notice-board'
  import type { Unsubscriber } from 'svelte/store'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { disposeObjectTree } from '../../utils/disposeObjectTree'
  import { emblemClothMaterial } from '../../utils/familyEmblem'
  import { translucentClone } from '../../utils/translucentMaterial'

  const PREVIEW_OPACITY = 0.5
  const GHOST_OPACITY = 0.3

  let tool = $state<EditorTool>('height')
  let placements = $state<ObjectPlacement[]>([])
  let selectedId = $state<number | null>(null)
  let previewPos = $state<{ x: number; y: number; z: number } | null>(null)
  let rotation = $state(0)
  let selectedType = $state<string | null>(null)
  let debugInfo = $state<PlayerDebugInfo | null>(null)
  let openDoorIds = $state<ReadonlySet<string>>(new Set())
  /** Open-door bits of the loaded region's placements, by placement id. */
  let openHere: Map<number, number> = new Map()
  let isEditorMode = $state(false)
  let currentFloor = $state(0)
  let currentHouseId = $state<string | null>(null)

  let lastLoadedRegion = { rx: NaN, rz: NaN }
  let regionLoadGeneration = 0
  let regionRetry: ReturnType<typeof setTimeout> | undefined

  const unsubs: Unsubscriber[] = [
    editorTool.subscribe((v) => (tool = v)),
    currentObjectData.subscribe((v) => {
      placements = v.placements
      // Re-sync bridge + furniture collision so newly placed/edited objects
      // take effect immediately. Skip the initial empty-state fire before any
      // region has been loaded.
      const { rx, rz } = lastLoadedRegion
      if (Number.isNaN(rx)) return
      bridgeManager.syncRegion(rx, rz, v.placements, objectDefsById)
      syncFurniture(v.placements)
    }),
    selectedObjectPlacementId.subscribe((v) => (selectedId = v)),
    objectPreviewPos.subscribe((v) => (previewPos = v)),
    objectRotation.subscribe((v) => (rotation = v)),
    selectedObjectType.subscribe((v) => (selectedType = v)),
    playerDebugInfo.subscribe((v) => (debugInfo = v)),
    openDoors.subscribe((v) => {
      openDoorIds = v
      if (refreshOpenHere()) syncFurniture(placements)
    }),
    mapEditorMode.subscribe((v) => (isEditorMode = v)),
    playerVisualFloorLevel.subscribe((v) => (currentFloor = v)),
    playerInsideHouseId.subscribe((v) => (currentHouseId = v)),
    objectManager.onRegionChanged(({ rx, rz, data }) => {
      if (rx === lastLoadedRegion.rx && rz === lastLoadedRegion.rz) {
        currentObjectData.set(data)
      }
    }),
    objectManager.onWorldReset(() => {
      const { rx, rz } = lastLoadedRegion
      regionLoadGeneration++
      clearTimeout(regionRetry)
      lastLoadedRegion = { rx: NaN, rz: NaN }
      furnitureManager.reset()
      bridgeManager.reset()
      resetDoorStates()
      currentObjectData.set({ placements: [] })
      if (Number.isFinite(rx)) void loadRegionObject(rx, rz)
    }),
  ]
  onDestroy(() => {
    regionLoadGeneration++
    clearTimeout(regionRetry)
    unsubs.forEach((u) => u())
  })

  async function loadRegionObject(rx: number, rz: number) {
    if (rx === lastLoadedRegion.rx && rz === lastLoadedRegion.rz) return
    lastLoadedRegion = { rx, rz }
    const generation = ++regionLoadGeneration
    clearTimeout(regionRetry)
    worldView.staticReady = false
    try {
      const data = await objectManager.fetchObject(rx, rz)
      if (generation !== regionLoadGeneration) return
      currentObjectData.set(data)
      furnitureManager.evictDistant(rx, rz)
      bridgeManager.evictDistant(rx, rz)
      worldView.staticReady = true
    } catch (error) {
      if (generation !== regionLoadGeneration) return
      console.error('Object region remains pending', error)
      regionRetry = setTimeout(() => {
        if (generation !== regionLoadGeneration) return
        lastLoadedRegion = { rx: NaN, rz: NaN }
        void loadRegionObject(rx, rz)
      }, 1000)
    }
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
  const modelBounds = new SvelteMap<
    string,
    { center: THREE.Vector3; size: THREE.Vector3 }
  >()
  const loadingModels = new SvelteSet<string>()
  const failedModels = new SvelteSet<string>()

  function buildProceduralModel(def: ObjectDef): THREE.Group | null {
    if (def.procedural === 'shopSign') {
      return buildShopSignBoard(getShopSignStyle(def.shopSignStyle).board)
    }
    if (def.procedural === 'noticeBoard') return buildNoticeBoard()
    return null
  }

  // Share GPU resources across rebuilds; disposing sign text breaks WebGPU samplers.
  const signTextCache = new SvelteMap<string, THREE.Mesh>()

  function getSignText(type: string, text: string): THREE.Object3D {
    const key = `${type}\u0000${text}`
    let base = signTextCache.get(key)
    if (!base) {
      const style = getShopSignStyle(objectDefsById.get(type)?.shopSignStyle)
      base = buildShopSignText(text, style.board, style.text)
      signTextCache.set(key, base)
    }
    // Clone shares the cached geometry/material/texture; a Mesh can only have
    // one parent, so each placement needs its own lightweight clone.
    return base.clone()
  }

  async function getModel(objectId: string): Promise<THREE.Group | null> {
    if (modelCache.has(objectId)) return modelCache.get(objectId)!
    if (loadingModels.has(objectId) || failedModels.has(objectId)) return null

    const def = objectDefsById.get(objectId)
    if (!def) return null

    loadingModels.add(objectId)
    try {
      let model: THREE.Group | null
      if (def.procedural) {
        // Defer to avoid re-entering rebuild() and duplicating placements.
        await Promise.resolve()
        model = buildProceduralModel(def)
      } else {
        if (!def.model) return null
        const gltf = await loadGLB(getObjectModelPath(def.model))
        if (def.kind === 'bridge' && def.bridge) {
          bridgeManager.registerBridgeMesh(objectId, gltf.scene, def.bridge)
          buildGhostMaterials(gltf.scene)
        }
        model = gltf.scene.clone()
        model.traverse((child) => {
          if (child instanceof THREE.Mesh) {
            // The alpha=0 collision plane sits on the deck and would shadow it
            const shadows = !isCollisionMaterial(child.material)
            child.castShadow = shadows
            child.receiveShadow = shadows
          }
        })
      }
      if (!model) return null

      const box = new THREE.Box3().setFromObject(model)
      const center = new THREE.Vector3()
      const size = new THREE.Vector3()
      box.getCenter(center)
      box.getSize(size)
      modelBounds.set(objectId, { center, size })
      modelCache.set(objectId, model)
      rebuild()
      // If the user is currently previewing this object, build the preview now —
      // otherwise the cursor stays empty until the mouse moves again.
      if (selectedType === objectId) updatePreview()
      return model
    } catch (error) {
      failedModels.add(objectId)
      console.error(`Failed to load object model ${objectId}:`, error)
      return null
    } finally {
      loadingModels.delete(objectId)
    }
  }

  let group = new THREE.Group()
  group.name = 'object-overlay'

  let previewGroup: THREE.Group | null = null
  let previewType: string | null = null

  function disposeClone(obj: THREE.Object3D) {
    obj.traverse((child) => {
      // Shop-sign text uses MeshBasicNodeMaterial + CanvasTexture, whose sampler
      // bindings crash the WebGPU backend if disposed (see TextLabel.svelte and
      // shop-sign.ts). The board reuses the shared housing door material, which
      // other houses depend on. Leave both for GC / shared ownership.
      if (child.userData?.isSignText || child.userData?.isSignBoard) return
      // Plain clones share the cached template's materials — disposing those
      // would force every placement to re-upload textures on the next draw.
      if (child instanceof THREE.Mesh && child.userData.ownsMaterial) {
        ;(child.material as THREE.Material).dispose()
      } else if (child instanceof THREE.LineSegments) {
        child.geometry.dispose()
        ;(child.material as THREE.Material).dispose()
      }
    })
    disposeObjectTree(obj)
  }

  function setPreviewMaterial(obj: THREE.Object3D, opacity: number) {
    obj.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        child.material = translucentClone(
          child.material as THREE.Material,
          opacity
        )
        child.userData.ownsMaterial = true
      }
    })
  }

  /** objectId → its placement clone in `group`, so the transform-only fast path
   *  can move an existing clone instead of tearing the whole region down. */
  const cloneById = new SvelteMap<number, THREE.Object3D>()
  /** Walled objects that hide their near side while the viewer is inside,
   *  and all but `ground` while they stand between viewer and camera. */
  const structures = $derived(
    placements.flatMap((placement) => {
      const def = objectDefsById.get(placement.type)
      const clone = cloneById.get(placement.id)
      const perimeter = def?.perimeter
      if (!def || !perimeter || !clone) return []
      const panel = def.cutaway ? clone.getObjectByName(def.cutaway) : undefined
      const annexPanels = (def.annexes ?? []).map((annex) =>
        annex.cutaway ? clone.getObjectByName(annex.cutaway) : undefined
      )
      if (!panel && !def.ground) return []
      const yaw = (placement.rotation * Math.PI) / 180
      const box = (
        x: [number, number],
        z: [number, number],
        height: number
      ) => {
        const b = rotatedRectAabb(x[0], x[1], z[0], z[1], yaw)
        return new THREE.Box3(
          new THREE.Vector3(
            placement.x + b.minX,
            placement.y,
            placement.z + b.minZ
          ),
          new THREE.Vector3(
            placement.x + b.maxX,
            placement.y + height,
            placement.z + b.maxZ
          )
        )
      }
      const boxes = def.ground
        ? [
            box(
              [-perimeter.width / 2, perimeter.width / 2],
              [-perimeter.depth / 2, perimeter.depth / 2],
              def.roofHeight ?? perimeter.height
            ),
            ...[...(def.blocks ?? []), ...(def.annexes ?? [])].map((b) =>
              box(b.x, b.z, b.height ?? perimeter.height)
            ),
          ]
        : []
      return [
        {
          placement,
          perimeter,
          annexes: def.annexes,
          panel,
          annexPanels,
          clone,
          ground: def.ground,
          boxes,
        },
      ]
    })
  )
  const swingDoors = $derived(
    placements.flatMap((placement) => {
      const def = objectDefsById.get(placement.type)
      const clone = def && cloneById.get(placement.id)
      if (!clone) return []
      return doorwaysOf(placement, def).flatMap(({ door, leaves }) => {
        const nodes = leaves.flatMap((leaf) => {
          const node = clone.getObjectByName(leaf.node)
          return node
            ? [{ node, open: THREE.MathUtils.degToRad(leaf.openDeg) }]
            : []
        })
        return nodes.length > 0
          ? [{ id: placement.id, door, leaves: nodes }]
          : []
      })
    })
  )
  $effect(() => {
    const { rx, rz } = lastLoadedRegion
    setObjectDoorways(
      placements.flatMap((placement) => {
        const def = objectDefsById.get(placement.type)
        if (!def) return []
        return doorwaysOf(placement, def).map(({ door, ax, az, bx, bz }) => ({
          objectId: placement.id,
          index: door,
          door: objectDoorId(rx, rz, placement.id, door),
          y: placement.y,
          ax,
          az,
          bx,
          bz,
        }))
      })
    )
  })
  /** Radians per second; tall leaves swing slower than house doors. */
  const DOOR_SWING_SPEED = 2
  const VIEW_BLOCK_DEPTH = 0.9
  const isEditing = () => isEditorMode && tool === 'object'

  // Keep shared fire materials outside the model group's rebuild/disposal sweep.
  const Y_AXIS = new THREE.Vector3(0, 1, 0)
  const fireRoot = new THREE.Group()
  fireRoot.name = 'objectFires'
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fireSystems = new Map<
    number,
    { system: FireParticles; pos: THREE.Vector3 }
  >()
  /** Currently visible fire systems, refreshed by syncFires(); update() only
   *  ticks these so paused (detached) systems cost nothing per frame. */
  const _activeFires: FireParticles[] = []
  /** World positions of visible flames, refreshed by syncFires(): log-bed
   *  fires (hearths) take the unified light, torches feed the glow pool. */
  const firePositions: THREE.Vector3[] = []
  const torchPositions: THREE.Vector3[] = []
  const _placementPos = new THREE.Vector3()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const _seenFires = new Set<number>()

  function fireWorldPos(
    p: ObjectPlacement,
    local: Position,
    out: THREE.Vector3
  ) {
    out.set(local.x, local.y, local.z)
    out.applyAxisAngle(Y_AXIS, (p.rotation * Math.PI) / 180)
    return out.add(_placementPos.set(p.x, p.y, p.z))
  }

  /** Sync fire systems to the visible placements. Systems whose placement is
   *  merely filtered out (other floor/house, per `_idShown`) are detached and
   *  paused, not disposed — tearing them down and re-creating them on every
   *  house entry churned garbage and GPU buffers, hitching the entry frame. */
  function syncFires(visible: ObjectPlacement[]) {
    firePositions.length = 0
    torchPositions.length = 0
    _activeFires.length = 0
    _seenFires.clear()
    for (const p of visible) {
      const def = objectDefsById.get(p.type)
      const fire = def?.fire
      if (!fire) continue
      const isTorch = def?.fireKind === 'torch'
      _seenFires.add(p.id)
      let entry = fireSystems.get(p.id)
      if (!entry) {
        const system = isTorch
          ? new TorchFireParticles()
          : new CampfireFireParticles()
        entry = { system, pos: new THREE.Vector3() }
        fireSystems.set(p.id, entry)
      }
      if (!entry.system.group.parent) fireRoot.add(entry.system.group)
      entry.system.setOrigin(fireWorldPos(p, fire, entry.pos))
      _activeFires.push(entry.system)
      ;(isTorch ? torchPositions : firePositions).push(entry.pos)
    }
    for (const [id, entry] of fireSystems) {
      if (_seenFires.has(id)) continue
      if (_idShown.has(id)) {
        fireRoot.remove(entry.system.group)
        continue
      }
      entry.system.dispose()
      fireSystems.delete(id)
    }
  }

  const footprintOwner = {}
  $effect(() => {
    setShelterFootprints(footprintOwner, structures)
    return () => setShelterFootprints(footprintOwner, null)
  })

  export function update(deltaTime: number, camera: THREE.Camera | undefined) {
    const viewer = debugInfo?.position
    for (const structure of structures) {
      const { placement, perimeter, annexes, panel, clone, ground } = structure
      const room = viewer
        ? shelterRoomAt(viewer, placement, perimeter, annexes)
        : null
      const inside = room !== null
      if (panel) panel.visible = !inside
      structure.annexPanels.forEach((annexPanel, index) => {
        if (annexPanel) annexPanel.visible = room !== index
      })
      if (!ground) continue
      const hidden =
        !!viewer &&
        !inside &&
        !isEditing() &&
        structure.boxes.some((box) =>
          isoCameraOccludesPlayer(
            box,
            viewer.x,
            viewer.y,
            viewer.z,
            VIEW_BLOCK_DEPTH
          )
        )
      if (hidden === (clone.userData.viewBlocked ?? false)) continue
      clone.userData.viewBlocked = hidden
      clone.traverse((child) => {
        if (child instanceof THREE.Mesh)
          child.visible = !hidden || child.name === ground
      })
    }
    for (const { id, door, leaves } of swingDoors) {
      const open = ((openHere.get(id) ?? 0) >> door) & 1
      const step = (DOOR_SWING_SPEED * deltaTime) / 1000
      for (const leaf of leaves) {
        const target = open ? leaf.open : 0
        const current = leaf.node.rotation.y
        leaf.node.rotation.y =
          current < target
            ? Math.min(current + step, target)
            : Math.max(current - step, target)
      }
    }
    for (const w of takeFieldBossWakes()) startWake(w)
    for (const s of takeFieldBossSleeps()) startSleep(s)
    if (camera) stepShell(camera)
    if (!fireRoot.visible) return
    for (const s of _activeFires) s.update(deltaTime, camera)
  }

  export function getFirePositions(): THREE.Vector3[] {
    return firePositions
  }

  export function getTorchPositions(): THREE.Vector3[] {
    return torchPositions
  }

  /** Apply a placement's position + rotation (yaw + pitch) to its clone. Single
   *  source of transform threading for both the full rebuild and fast path. */
  function applyPlacementTransform(clone: THREE.Object3D, p: ObjectPlacement) {
    clone.position.set(p.x, p.y, p.z)
    clone.rotation.set(
      ((p.rotationX ?? 0) * Math.PI) / 180,
      (p.rotation * Math.PI) / 180,
      0
    )
  }

  /** The woken boss stands where its dormant stand-in lay. */
  const WOKEN_RADIUS_M = 8

  const standIns = $derived(
    placements.flatMap((p) => {
      const kind = objectDefsById.get(p.type)?.dormantMonster
      return kind ? [{ p, kind }] : []
    })
  )

  /** Stand-ins whose monster is up on them, keyed as a string so monster
   *  moves rebuild only when the set changes. Regions without one never read
   *  the monster map. A boss sinking back to sleep no longer counts: its
   *  stand-in is solid again, rising under it. */
  const wokenKey = $derived.by(() => {
    if (standIns.length === 0) return ''
    const ids: number[] = []
    for (const standIn of standIns) {
      for (const m of monsterManager.monsters.values()) {
        if (m.sinkAt === undefined && risesFrom(standIn, m.type, m.position)) {
          ids.push(standIn.p.id)
          break
        }
      }
    }
    return ids.join(',')
  })
  const wokenIds = $derived(
    new Set(wokenKey ? wokenKey.split(',').map(Number) : [])
  )

  function risesFrom(
    standIn: { p: ObjectPlacement; kind: string },
    monster: string,
    at: Position
  ) {
    if (standIn.kind !== monster) return false
    const dx = shortestWrappedDeltaX(standIn.p.x, at.x)
    const dz = at.z - standIn.p.z
    return dx * dx + dz * dz <= WOKEN_RADIUS_M ** 2
  }

  /** A woken stand-in is not solid, so only the boss body blocks. */
  let unsolidIds: Set<number> = new Set()
  function refreshOpenHere(): boolean {
    const { rx, rz } = lastLoadedRegion
    const bitsOf = (id: number) =>
      [0, 1, 2, 3, 4, 5, 6, 7].reduce(
        (bits, door) =>
          openDoorIds.has(objectDoorId(rx, rz, id, door))
            ? bits | (1 << door)
            : bits,
        0
      )
    const next = new Map(
      openDoorIds.size > 0
        ? placements
            .map((p) => [p.id, bitsOf(p.id)] as const)
            .filter(([, bits]) => bits !== 0)
        : []
    )
    const changed =
      next.size !== openHere.size ||
      [...next].some(([id, bits]) => openHere.get(id) !== bits)
    if (changed) openHere = next
    return changed
  }
  function syncFurniture(all: ObjectPlacement[]) {
    const { rx, rz } = lastLoadedRegion
    if (Number.isNaN(rx)) return
    const solid = unsolidIds.size
      ? all.filter((p) => !unsolidIds.has(p.id))
      : all
    refreshOpenHere()
    furnitureManager.syncRegion(
      rx,
      rz,
      openHere.size
        ? solid.map((p) =>
            openHere.has(p.id) ? { ...p, openDoors: openHere.get(p.id) } : p
          )
        : solid
    )
  }
  $effect(() => {
    const ids = wokenIds
    if (ids.size === 0 && unsolidIds.size === 0) return
    unsolidIds = ids
    untrack(() => syncFurniture(placements))
  })

  const wakeEffect = new DormantWakeEffect()
  const { renderer, camera: sceneCamera, scene } = useThrelte()
  /** The stand-in's shell cracking open as its boss wakes, or closing back
   *  over it as it sleeps. */
  let shell: {
    kind: 'wake' | 'sleep'
    p: ObjectPlacement
    startedAt: number
    collapseMs: number
    height: number
    sound: string | undefined
  } | null = null
  const _wakeBox = new THREE.Box3()
  // Same loader and key as Monster.svelte, so the boss model is cached by
  // the time it spawns inside the dust.
  const monsterLoader = useLoader(GLTFLoader, {
    extend: (loader) => loader.setMeshoptDecoder(MeshoptDecoder),
  })

  // Compile once a stand-in is near: the wake plays at its most watched moment.
  let wakeWarmed = false
  $effect(() => {
    if (wakeWarmed || standIns.length === 0) return
    wakeWarmed = true
    wakeEffect.warm(renderer, $sceneCamera, scene, () => !shell)
  })

  /** The stand-in's box at rest, as the effect's bounds. */
  function standInBounds(
    clone: THREE.Object3D,
    p: ObjectPlacement,
    ground: number
  ) {
    applyPlacementTransform(clone, p)
    return wakeBounds(_wakeBox.setFromObject(clone), ground)
  }

  function standInFor(monster: string, at: Position) {
    const standIn = standIns.find((s) => risesFrom(s, monster, at))
    const clone = standIn && cloneById.get(standIn.p.id)
    return clone && { p: standIn.p, clone }
  }

  function startWake(w: FieldBossWake) {
    const def = getMonsterDef(w.monster)
    if (def?.model) monsterLoader.load(`/models/${def.model}`)
    const found = standInFor(w.monster, w.position)
    if (!found?.clone.parent) return
    beginShell(
      'wake',
      found,
      w.position.y,
      w.startedAt,
      w.collapseMs,
      def?.wakeSound
    )
  }

  /** Detached clones count too: the stand-in comes back into view this frame. */
  function startSleep(s: FieldBossSleep) {
    const found = standInFor(s.monster, s.position)
    if (!found) return
    beginShell(
      'sleep',
      found,
      s.position.y,
      s.sinkAt,
      SLEEP_SINK_MS,
      getMonsterDef(s.monster)?.sleepSound
    )
  }

  function beginShell(
    kind: 'wake' | 'sleep',
    { p, clone }: { p: ObjectPlacement; clone: THREE.Object3D },
    ground: number,
    startedAt: number,
    collapseMs: number,
    sound: string | undefined
  ) {
    endShell()
    const bounds = standInBounds(clone, p, ground)
    if (kind === 'wake') wakeEffect.start(bounds, collapseMs)
    else wakeEffect.startSleep(bounds)
    shell = {
      kind,
      p,
      startedAt,
      collapseMs,
      height: bounds.height,
      sound,
    }
  }

  function stepShell(camera: THREE.Camera) {
    if (!shell) return
    const elapsed = Date.now() - shell.startedAt
    // Late by a second or more and the sound would trail the dust.
    if (shell.sound && elapsed >= 0) {
      if (elapsed < 1000) playMonsterSound(shell.sound)
      shell.sound = undefined
    }
    if (!wakeEffect.update(elapsed, camera)) {
      endShell()
      return
    }
    const clone = cloneById.get(shell.p.id)
    if (!clone) return
    const { kind, p, collapseMs, height } = shell
    const t = elapsed / 1000
    if (kind === 'sleep') {
      // Climbs back out from under the ground once the dust is up,
      // shuddering less as it settles. Fetched each frame: the filter flip
      // re-attaches it.
      clone.visible = elapsed >= SLEEP_RISE_FROM_MS
      if (!clone.visible) return
      const rise = sleepRise(elapsed)
      applyPlacementTransform(clone, p)
      shake(clone, t, 0.12 * (1 - rise))
      clone.position.y -= (1 - rise) * height * 1.05
    } else if (elapsed >= collapseMs + DORMANT_SINK_MS) {
      // Hidden directly: a reactive flag would rebuild every placement.
      clone.visible = false
    } else {
      const rumble = Math.min(1, elapsed / collapseMs)
      const sink = Math.max(0, (elapsed - collapseMs) / DORMANT_SINK_MS)
      const amp = 0.03 + 0.12 * rumble * rumble
      applyPlacementTransform(clone, p)
      shake(clone, t, amp)
      clone.position.y +=
        Math.sin(t * 61) * amp * 0.3 - sink * sink * height * 0.6
      clone.rotation.z += Math.sin(t * 29) * amp * 0.02
    }
  }

  /** A woken boss stands on the stand-in by now, which keeps it hidden. */
  function endShell() {
    if (!shell) return
    const { p } = shell
    shell = null
    wakeEffect.clear()
    const clone = cloneById.get(p.id)
    if (!clone) return
    applyPlacementTransform(clone, p)
    clone.visible = true
  }

  /** Placements currently shown, after floor + house filtering. */
  function visiblePlacements(visibleFloor: number): ObjectPlacement[] {
    return placements.filter((p) => {
      if (p.floorLevel !== visibleFloor) return false
      if (!isEditorMode && wokenIds.has(p.id)) return false
      const pHouse = housingManager.findHouseAtPoint(p.x, p.y, p.z)
      return currentHouseId ? pHouse?.id === currentHouseId : pHouse == null
    })
  }

  /** Everything that decides how a placement's clone is built (vs. merely
   *  where it sits). A changed key means teardown + re-clone of that one. */
  function structKeyOf(p: ObjectPlacement): string {
    const sel = isEditing() && p.id === selectedId ? 'S' : ''
    const emblem = p.emblem ? `${p.emblem.texture}/${p.emblem.color}` : ''
    return `${p.type}:${p.text ?? ''}:${emblem}:${sel}`
  }

  function buildClone(
    p: ObjectPlacement,
    template: THREE.Group,
    structKey: string
  ) {
    const clone = template.clone()
    applyPlacementTransform(clone, p)
    if (isEditing() && p.id === selectedId) {
      const bounds = modelBounds.get(p.type)
      if (bounds) clone.add(createSelectionBox(bounds.center, bounds.size))
    }
    clone.userData.objectId = p.id
    if (p.fixed) clone.userData.objectFixed = true
    clone.userData.objectType = p.type
    clone.userData.structKey = structKey
    if (p.emblem) {
      clone.traverse((child) => {
        if (child instanceof THREE.Mesh && child.name === 'emblem_cloth') {
          child.material = emblemClothMaterial(p.emblem)
          child.userData.ownsMaterial = true
        }
      })
    }
    const catDef = objectDefsById.get(p.type)
    if (p.text) {
      if (catDef?.procedural === 'shopSign') {
        // Persistent baked sign face — no hover bubble (so we skip objectText).
        // Reuse a cached (undisposable) text mesh via clone so repeated
        // rebuilds don't leak a fresh CanvasTexture each time.
        clone.add(getSignText(p.type, p.text))
      } else {
        clone.userData.objectText = p.text
      }
    }
    if (catDef?.interaction) {
      clone.userData.objectInteraction = catDef.interaction
      clone.userData.objectInteractOffset = catDef.interactOffset
    }
    if (catDef?.kind) clone.userData.objectKind = catDef.kind
    const clickKind: ObjectClickKind | null = catDef?.altar
      ? 'altar'
      : catDef?.noticeBoard
        ? 'notice_board'
        : catDef?.mailbox
          ? 'mailbox'
          : null
    if (clickKind) clone.userData.objectClickKind = clickKind
    if (catDef) {
      for (const door of doorwaysOf(p, catDef)) {
        const clickPosition = {
          x: (door.ax + door.bx) / 2,
          y: p.y,
          z: (door.az + door.bz) / 2,
        }
        for (const leaf of door.leaves) {
          const node = clone.getObjectByName(leaf.node)
          if (!node) continue
          node.userData = {
            ...node.userData,
            objectId: p.id,
            objectDoor: door.door,
            clickPosition,
          }
        }
      }
    }
    if (catDef?.noticeBoard)
      mountNoticePapers(clone, { Village: { object_id: p.id } })
    return clone
  }

  /** id → currently shown, over every loaded placement. `has()` answers
   *  "still loaded", `get()` answers "passes the floor/house filter". */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const _idShown = new Map<number, boolean>()

  /** Incremental sync of clones to the loaded placements. Every placement gets
   *  a clone up front; ones filtered out (other floor/house) are detached from
   *  the scene graph but kept in cloneById with their geometry, materials and
   *  compiled GPU state alive — destroy/re-create on house entry churned
   *  garbage and GPU bind groups, hitching the entry frame. Detached clones
   *  cost no render traversal and no raycasts. Teardown only when the
   *  placement is gone or its struct key changes. */
  function rebuild() {
    _idShown.clear()
    for (const p of placements) _idShown.set(p.id, false)
    const visible = visiblePlacements(currentFloor)
    for (const p of visible) _idShown.set(p.id, true)
    syncFires(visible)

    for (const [id, clone] of cloneById) {
      if (_idShown.has(id)) continue
      disposeClone(clone)
      group.remove(clone)
      cloneById.delete(id)
    }

    let attached = false
    for (const p of placements) {
      const show = _idShown.get(p.id)
      let clone = cloneById.get(p.id)
      if (clone && show && clone.userData.structKey !== structKeyOf(p)) {
        disposeClone(clone)
        group.remove(clone)
        cloneById.delete(p.id)
        clone = undefined
      }
      if (!clone) {
        const template = modelCache.get(p.type)
        if (!template) {
          getModel(p.type)
          continue
        }
        clone = buildClone(p, template, structKeyOf(p))
        cloneById.set(p.id, clone)
      } else if (show) {
        applyPlacementTransform(clone, p)
      }
      if (show) {
        if (!clone.parent) {
          group.add(clone)
          attached = true
        }
      } else if (clone.parent) {
        group.remove(clone)
      }
    }
    // Freshly attached clones start opaque; the $effect will re-apply ghost
    // next frame if the player is still under a bridge.
    if (attached) ghostBridgeId = null
  }

  function updatePreview() {
    if (!isEditing() || !previewPos || !selectedType) {
      if (previewGroup) {
        disposeClone(previewGroup)
        group.remove(previewGroup)
        previewGroup = null
        previewType = null
      }
      return
    }

    if (previewType !== selectedType) {
      if (previewGroup) {
        disposeClone(previewGroup)
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
    void tool
    void isEditorMode
    void currentFloor
    void wokenKey
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

  /** Ghost twins of each bridge model's materials, built once per model so
   *  every placement shares the same two material sets and the ghost toggle
   *  is a reference swap — no per-placement clones, no needsUpdate recompiles. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const ghostOf = new Map<THREE.Material, THREE.Material>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const solidOf = new Map<THREE.Material, THREE.Material>()

  /** Alpha=0 planes baked into bridge GLBs to fill deck holes for collision. */
  function isCollisionMaterial(m: THREE.Material | THREE.Material[]) {
    return !Array.isArray(m) && m.name?.startsWith('DeckCollisionInvisible')
  }

  function buildGhostMaterials(scene: THREE.Object3D) {
    scene.traverse((o) => {
      if (!(o instanceof THREE.Mesh)) return
      const m = o.material as THREE.Material
      if (ghostOf.has(m) || isCollisionMaterial(m)) return
      const g = translucentClone(m, GHOST_OPACITY)
      ghostOf.set(m, g)
      solidOf.set(g, m)
    })
  }

  function applyBridgeGhost(placementId: number, ghost: boolean) {
    cloneById.get(placementId)?.traverse((o) => {
      if (!(o instanceof THREE.Mesh)) return
      const swapped = (ghost ? ghostOf : solidOf).get(
        o.material as THREE.Material
      )
      if (!swapped) return
      o.material = swapped
      // Draw after the river ribbon (renderOrder=1) so alpha-blended deck
      // sorts above water consistently.
      o.renderOrder = ghost ? 2 : 0
    })
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

  export function setVisible(visible: boolean) {
    group.visible = visible
    fireRoot.visible = visible
    if (!visible) endShell()
  }

  onDestroy(() => {
    wakeEffect.dispose()
    for (const e of fireSystems.values()) e.system.dispose()
    fireSystems.clear()
    for (const obj of new Set([...group.children, ...cloneById.values()])) {
      disposeClone(obj)
    }
    group.clear()
    modelCache.clear()
    for (const g of ghostOf.values()) g.dispose()
    ghostOf.clear()
    solidOf.clear()
  })
</script>

<T is={group} />
<T is={fireRoot} />
<T is={wakeEffect.group} />
