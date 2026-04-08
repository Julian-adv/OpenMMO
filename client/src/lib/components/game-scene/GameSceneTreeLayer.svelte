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
  }

  let {
    terrainTiles,
    treeDataManager = null,
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
            im.count = 0
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

  // ── Tile data cache ────────────────────────────────────
  const tileTreeDataCache = new SvelteMap<string, TreePlacementData>()
  const fetchedTiles = new SvelteSet<string>()
  const pendingTiles = new SvelteSet<string>()

  /** Write instance transforms into the mesh's instanceMatrix. */
  function writeInstanceMatrices(
    mesh: THREE.InstancedMesh,
    instances: Float32Array[],
  ): number {
    let idx = 0
    for (const raw of instances) {
      const count = raw.length / 5
      for (let i = 0; i < count && idx < MAX_INSTANCES; i++) {
        const base = i * 5
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
    return idx
  }

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
  function scheduleRebuild() {
    if (rebuildScheduled) return
    rebuildScheduled = true
    queueMicrotask(() => {
      rebuildScheduled = false
      rebuildGlobalMeshes()
    })
  }

  /** Rebuild all global slots from cached tile data. */
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

    for (const slot of globalSlots) {
      const { mesh, typeIdx } = slot
      writeInstanceMatrices(mesh, allData[typeIdx])
      mesh.boundingSphere = computeBoundingSphere(allData[typeIdx])

      // Remove + re-add to force WebGPU buffer re-upload
      if (mesh.parent) mesh.parent.remove(mesh)
      treeGroup.add(mesh)
    }
  }

  export function update() {}

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
