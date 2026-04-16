<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainTreeDataManager } from '../../managers/terrainTreeDataManager'
  import { getTreeInstanceData, type TreePlacementData } from '../../utils/tree-data'
  import { loadGLB } from '../../utils/gltfCache'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'

  interface Props {
    terrainTiles: TerrainTile[]
    treeDataManager: TerrainTreeDataManager | null
    playerPosition: { x: number; y: number; z: number } | null
  }

  let {
    terrainTiles,
    treeDataManager = null,
    playerPosition = null,
  }: Props = $props()

  const treeGroup = new THREE.Group()

  export function getGroup(): THREE.Group {
    return treeGroup
  }

  // ── Constants ──────────────────────────────────────────
  const MAX_INSTANCES = 1024

  // ── Global meshes: one InstancedMesh per tree-type × sub-mesh ──
  interface GlobalSlot {
    mesh: THREE.InstancedMesh
    typeIdx: number
  }

  const globalSlots: GlobalSlot[] = []
  let modelsReady = false
  let modelsLoadPromise: Promise<boolean> | null = null

  // Reusable temp objects for matrix composition
  const _mat4 = new THREE.Matrix4()
  const _pos = new THREE.Vector3()
  const _quat = new THREE.Quaternion()
  const _scale = new THREE.Vector3()
  const _up = new THREE.Vector3(0, 1, 0)

  function ensureModelsLoaded(): Promise<boolean> {
    if (modelsReady) return Promise.resolve(true)
    if (modelsLoadPromise) return modelsLoadPromise

    modelsLoadPromise = (async () => {
      try {
        const [gltf1, gltf2] = await Promise.all([
          loadGLB('/models/tree.glb'),
          loadGLB('/models/tree2.glb'),
        ])

        for (let t = 0; t < 2; t++) {
          const scene = t === 0 ? gltf1.scene : gltf2.scene
          scene.updateMatrixWorld(true)
          const sceneInv = new THREE.Matrix4()
            .copy(scene.matrixWorld)
            .invert()

          scene.traverse((child) => {
            if (!(child as THREE.Mesh).isMesh) return
            const mesh = child as THREE.Mesh
            const srcMat = (
              Array.isArray(mesh.material)
                ? mesh.material[0]
                : mesh.material
            ) as THREE.MeshStandardMaterial

            // Bake sub-mesh local transform into geometry so instanceMatrix
            // only needs instance position/rotation/scale
            const localMatrix = new THREE.Matrix4()
              .copy(mesh.matrixWorld)
              .premultiply(sceneInv)
            const geo = mesh.geometry.clone()
            geo.applyMatrix4(localMatrix)

            // Use GLB material directly (clone for independence)
            const mat = srcMat.clone()
            // GLB meshes: "Tw"/"Tw.001" = trunk, "Fronds"/"Fronds.001" = leaves
            const isTrunk = mesh.name.startsWith('Tw')
            if (isTrunk) mat.side = THREE.FrontSide

            const im = new THREE.InstancedMesh(geo, mat, MAX_INSTANCES)
            im.castShadow = true
            im.receiveShadow = true
            // Seed a dummy instance far below the world so the WebGPU pipeline
            // compiles during the initial load phase. Without this, the first
            // render happens when a tile actually places a real tree — which
            // stalls the main thread for ~100-800ms mid-movement.
            im.count = 1
            _mat4.makeTranslation(0, -100000, 0)
            im.setMatrixAt(0, _mat4)
            im.instanceMatrix.needsUpdate = true
            treeGroup.add(im)

            globalSlots.push({ mesh: im, typeIdx: t })
          })
        }

        modelsReady = true
        return true
      } catch (e) {
        console.error('Failed to load tree models:', e)
        modelsLoadPromise = null
        return false
      }
    })()
    return modelsLoadPromise
  }

  // ── Tree occlusion ─────────────────────────────────────
  // Approximate bounds at scale 1.0 per tree type [tree1, tree2]
  const TREE_OCCLUDE_HEIGHT: [number, number] = [2.9, 3.4]
  const TREE_OCCLUDE_HALF_W: [number, number] = [0.8, 1.1]

  /** Per-tree occlusion bits for frame-to-frame change detection. */
  let occBits = new Uint8Array(256)
  let occBitsLen = 0
  let lastOccPx = NaN
  let lastOccPy = NaN
  let lastOccPz = NaN

  /**
   * Same isometric ray-AABB test as houseOccludesPlayer.
   * Ray from player toward camera: R(s) = (px − s, py + s, pz + s), s >= 0.
   */
  function treeOccludesPlayer(
    tx: number, ty: number, tz: number,
    scale: number, typeIdx: number,
    px: number, py: number, pz: number,
  ): boolean {
    const h = TREE_OCCLUDE_HEIGHT[typeIdx] * scale
    const hw = TREE_OCCLUDE_HALF_W[typeIdx] * scale
    const sHigh = ty + h - py
    if (sHigh <= 0) return false
    const sLow = Math.max(ty - py, 0)
    const sMin = Math.max(px - tx - hw, tz - hw - pz, sLow)
    const sMax = Math.min(px - tx + hw, tz + hw - pz, sHigh)
    return sMin <= sMax
  }

  // ── Tile data cache ────────────────────────────────────
  const tileTreeDataCache = new SvelteMap<string, TreePlacementData>()
  const fetchedTiles = new SvelteSet<string>()
  const pendingTiles = new SvelteSet<string>()

  /** Compute bounding sphere from raw instance arrays. */
  function computeBoundingSphere(instances: Float32Array[]): THREE.Sphere {
    let total = 0
    for (const raw of instances) total += raw.length / 5
    if (total === 0) return new THREE.Sphere(new THREE.Vector3(), 0)

    let minX = Infinity, maxX = -Infinity
    let minY = Infinity, maxY = -Infinity
    let minZ = Infinity, maxZ = -Infinity

    for (const raw of instances) {
      const count = raw.length / 5
      for (let i = 0; i < count; i++) {
        const base = i * 5
        const x = raw[base], y = raw[base + 1], z = raw[base + 2]
        if (x < minX) minX = x; if (x > maxX) maxX = x
        if (y < minY) minY = y; if (y > maxY) maxY = y
        if (z < minZ) minZ = z; if (z > maxZ) maxZ = z
      }
    }

    const TREE_MARGIN = 10
    const center = new THREE.Vector3(
      (minX + maxX) / 2,
      (minY + maxY) / 2 + TREE_MARGIN / 2,
      (minZ + maxZ) / 2,
    )
    const dx = maxX - minX + TREE_MARGIN * 2
    const dy = maxY - minY + TREE_MARGIN * 2
    const dz = maxZ - minZ + TREE_MARGIN * 2
    return new THREE.Sphere(center, Math.sqrt(dx * dx + dy * dy + dz * dz) / 2)
  }

  // Coalesce multiple rebuild requests into a single microtask
  let rebuildScheduled = false
  /** Cached bounding spheres per tree type — invalidated on tile data changes. */
  let cachedBoundingSpheres: [THREE.Sphere, THREE.Sphere] | null = null

  function scheduleRebuild() {
    if (rebuildScheduled) return
    rebuildScheduled = true
    occBitsLen = 0 // Reset occlusion tracking (tile set changed)
    cachedBoundingSpheres = null
    queueMicrotask(() => {
      rebuildScheduled = false
      rebuildGlobalMeshes()
    })
  }

  /** Rebuild all global slots from cached tile data, skipping occluded trees. */
  function rebuildGlobalMeshes() {
    if (!modelsReady) return

    // Collect instance data arrays per tree type
    const allData: [Float32Array[], Float32Array[]] = [[], []]
    for (const data of tileTreeDataCache.values()) {
      for (let t = 0; t < 2; t++) {
        const type = t === 0 ? 'tree1' : ('tree2' as const)
        const raw = getTreeInstanceData(data, type)
        if (raw.length > 0) allData[t].push(raw)
      }
    }

    const doOcc = playerPosition !== null
    const px = playerPosition?.x ?? 0
    const py = playerPosition?.y ?? 0
    const pz = playerPosition?.z ?? 0

    for (const slot of globalSlots) {
      const { mesh, typeIdx } = slot
      let idx = 0
      for (const raw of allData[typeIdx]) {
        const count = raw.length / 5
        for (let i = 0; i < count && idx < MAX_INSTANCES; i++) {
          const base = i * 5
          if (doOcc && treeOccludesPlayer(raw[base], raw[base + 1], raw[base + 2], raw[base + 4], typeIdx, px, py, pz)) {
            continue
          }
          _pos.set(raw[base], raw[base + 1], raw[base + 2])
          _quat.setFromAxisAngle(_up, raw[base + 3])
          const s = raw[base + 4]
          _scale.set(s, s, s)
          _mat4.compose(_pos, _quat, _scale)
          mesh.setMatrixAt(idx, _mat4)
          idx++
        }
      }
      mesh.count = idx
      mesh.instanceMatrix.needsUpdate = true

      if (!cachedBoundingSpheres) {
        cachedBoundingSpheres = [
          computeBoundingSphere(allData[0]),
          computeBoundingSphere(allData[1]),
        ]
      }
      mesh.boundingSphere = cachedBoundingSpheres[typeIdx]

      // Remove + re-add to force WebGPU buffer re-upload
      if (mesh.parent) mesh.parent.remove(mesh)
      treeGroup.add(mesh)
    }
  }

  export function update() {
    if (!playerPosition || !modelsReady || tileTreeDataCache.size === 0) return

    const { x: px, y: py, z: pz } = playerPosition
    const dx = px - lastOccPx
    const dy = py - lastOccPy
    const dz = pz - lastOccPz
    if (dx * dx + dy * dy + dz * dz < 0.01) return
    lastOccPx = px
    lastOccPy = py
    lastOccPz = pz

    // Check if any tree's occlusion state changed
    let total = 0
    for (const data of tileTreeDataCache.values()) {
      total += data.tree1Count + data.tree2Count
    }
    if (occBits.length < total) {
      const newBits = new Uint8Array(Math.max(total, 256))
      newBits.set(occBits)
      occBits = newBits
    }

    let changed = total !== occBitsLen
    let idx = 0
    for (const data of tileTreeDataCache.values()) {
      for (let t = 0; t < 2; t++) {
        const type = t === 0 ? 'tree1' : ('tree2' as const)
        const raw = getTreeInstanceData(data, type)
        const count = raw.length / 5
        for (let i = 0; i < count; i++) {
          const base = i * 5
          const occ: number = treeOccludesPlayer(
            raw[base], raw[base + 1], raw[base + 2], raw[base + 4],
            t, px, py, pz,
          ) ? 1 : 0
          if (occBits[idx] !== occ) {
            occBits[idx] = occ
            changed = true
          }
          idx++
        }
      }
    }
    occBitsLen = total

    if (changed) rebuildGlobalMeshes()
  }

  // ── Tile update listener (e.g. vegetation removal from splat painting) ──
  $effect(() => {
    const tMgr = treeDataManager
    if (!tMgr) return

    return tMgr.onTileUpdated((tileX, tileZ) => {
      const treeData = tMgr.getCachedTreeData(tileX, tileZ)
      const tk = `${tileX}_${tileZ}`
      if (treeData && (treeData.tree1Count > 0 || treeData.tree2Count > 0)) {
        tileTreeDataCache.set(tk, treeData)
      } else {
        tileTreeDataCache.delete(tk)
      }
      scheduleRebuild()
    })
  })

  // ── Invalidation listener ─────────────────────────────
  $effect(() => {
    const tMgr = treeDataManager
    if (!tMgr) return

    return tMgr.onInvalidateAll(() => {
      tileTreeDataCache.clear()
      fetchedTiles.clear()
      pendingTiles.clear()
      scheduleRebuild()
    })
  })

  // ── Tile data lifecycle ─────────────────────────────────
  $effect(() => {
    const tMgr = treeDataManager
    if (!tMgr) return

    for (const tile of terrainTiles) {
      const tk = tile.id
      if (fetchedTiles.has(tk) || pendingTiles.has(tk)) continue

      const tileX = Math.round(tile.position[0] / TERRAIN_TILE_SIZE)
      const tileZ = Math.round(tile.position[2] / TERRAIN_TILE_SIZE)

      pendingTiles.add(tk)

      tMgr
        .loadTreeData(tileX, tileZ)
        .then(async (treeData: TreePlacementData | null) => {
          if (!pendingTiles.has(tk)) return
          pendingTiles.delete(tk)

          if (treeData && (treeData.tree1Count > 0 || treeData.tree2Count > 0)) {
            tileTreeDataCache.set(tk, treeData)
          }
          fetchedTiles.add(tk)

          await ensureModelsLoaded()
          scheduleRebuild()
        })
        .catch(() => {
          pendingTiles.delete(tk)
        })
    }

    const tileIds = new Set(terrainTiles.map((t) => t.id))
    let changed = false
    for (const tk of fetchedTiles) {
      if (!tileIds.has(tk)) {
        fetchedTiles.delete(tk)
        tileTreeDataCache.delete(tk)
        changed = true
      }
    }
    if (changed) scheduleRebuild()
  })
</script>

<T is={treeGroup} />
