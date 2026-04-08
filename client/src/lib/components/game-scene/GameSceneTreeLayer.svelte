<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { TerrainTile } from './terrain-utils'
  import { TERRAIN_TILE_SIZE } from './terrain-utils'
  import type { TerrainTreeDataManager } from '../../managers/terrainTreeDataManager'
  import { getTreeInstanceData, type TreePlacementData } from '../../utils/tree-data'
  import { loadGLB } from '../../utils/gltfCache'
  import {
    createTreeInstanceContext,
    createTreeMaterial,
    writeTreeInstanceData,
    type TreeInstanceContext,
  } from '../../shaders/tree-material'

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
  // Reuse same mesh UUIDs across tile changes to avoid WebGPU pipeline
  // recompilation. Instance data lives in instancedArray buffers, NOT
  // in the instanceMatrix (which stays zeroed — positionNode overrides).
  interface GlobalSlot {
    mesh: THREE.InstancedMesh
    ctx: TreeInstanceContext
    typeIdx: number
  }
   
  const globalSlots: GlobalSlot[] = []
  let modelsReady = false
  let modelsLoadPromise: Promise<boolean> | null = null

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

            // Bake sub-mesh local transform into geometry so shader
            // only needs instance position/rotation/scale
            const localMatrix = new THREE.Matrix4()
              .copy(mesh.matrixWorld)
              .premultiply(sceneInv)
            const geo = mesh.geometry.clone()
            geo.applyMatrix4(localMatrix)

            // Instance buffers (like grass bladeData / bladeScale)
            const ctx = createTreeInstanceContext(MAX_INSTANCES)

            // TSL material reads from instancedArray via positionNode
            const mat = createTreeMaterial(ctx, srcMat)

            // GLB meshes: "Tw"/"Tw.001" = trunk, "Fronds"/"Fronds.001" = leaves
            const isTrunk = mesh.name.startsWith('Tw')
            if (isTrunk) mat.side = THREE.FrontSide

            const im = new THREE.InstancedMesh(geo, mat, MAX_INSTANCES)
            im.castShadow = true
            im.receiveShadow = true
            im.count = 0
            treeGroup.add(im)

            globalSlots.push({ mesh: im, ctx, typeIdx: t })
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
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const tileTreeDataCache = new Map<string, TreePlacementData>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fetchedTiles = new Set<string>()
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const pendingTiles = new Set<string>()

  /** Compute bounding sphere from instance position data. */
  function computeBoundingSphere(ctx: TreeInstanceContext): THREE.Sphere {
    const posArr = ctx.posData.value.array as Float32Array
    if (ctx.count === 0) {
      return new THREE.Sphere(new THREE.Vector3(), 0)
    }

    let minX = Infinity, maxX = -Infinity
    let minY = Infinity, maxY = -Infinity
    let minZ = Infinity, maxZ = -Infinity

    // posData layout: vec4(worldX, worldZ, worldY, rotation)
    for (let i = 0; i < ctx.count; i++) {
      const p = i * 4
      const x = posArr[p]
      const z = posArr[p + 1]
      const y = posArr[p + 2]
      if (x < minX) minX = x; if (x > maxX) maxX = x
      if (y < minY) minY = y; if (y > maxY) maxY = y
      if (z < minZ) minZ = z; if (z > maxZ) maxZ = z
    }

    // Add margin for tree crown/trunk extent
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
      const { mesh, ctx, typeIdx } = slot
      writeTreeInstanceData(ctx, allData[typeIdx], MAX_INSTANCES)
      mesh.count = ctx.count
      mesh.boundingSphere = computeBoundingSphere(ctx)

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
