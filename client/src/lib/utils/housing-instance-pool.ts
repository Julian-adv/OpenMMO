/**
 * housing-instance-pool.ts — InstancedMesh batching pool for housing.
 *
 * Groups uniform 1m template pieces (wall) into shared
 * InstancedMeshes keyed by (template, textureIndex). This reduces draw calls
 * from O(houses) to O(templates × textures) ≈ 30.
 *
 * Non-uniform pieces (door/window frames, stairwell steps) are excluded
 * and handled via per-house merged geometry.
 */
import * as THREE from 'three'
import { getHousingMaterial, HOUSING_TEXTURES } from './housing-textures'
import {
  DEFAULT_WALL_HEIGHT,
  WALL_THICKNESS,
  OFFSCREEN_Y,
} from './house-geometry'
const INITIAL_CAPACITY = 2048

export type InstanceTemplate = 'wall'

export interface InstanceDescriptor {
  template: InstanceTemplate
  textureIndex: number
  /** Local X (relative to house origin) */
  x: number
  /** Local Y */
  y: number
  /** Local Z */
  z: number
  rotY: number
  floorLevel: number
  isFront: boolean
}

interface SlotRecord {
  batchKey: string
  slotIndex: number
  originalY: number
  floorLevel: number
  isFront: boolean
}

interface Batch {
  mesh: THREE.InstancedMesh
  capacity: number
  nextSlot: number
  freeSlots: number[]
  dirty: boolean
}

const _tmpMatrix = new THREE.Matrix4()
const _offscreenMatrix = new THREE.Matrix4().makeTranslation(0, OFFSCREEN_Y, 0)

export class HousingInstancePool {
  private templateGeos = new Map<InstanceTemplate, THREE.BufferGeometry>()
  private batches = new Map<string, Batch>()
  private houseSlots = new Map<string, SlotRecord[]>()
  private parent: THREE.Group

  constructor(parent: THREE.Group) {
    this.parent = parent
    this.createTemplateGeometries()
  }

  private createTemplateGeometries() {
    // Wall: 1m wide × 3m tall × 0.1m thick
    const wallGeo = new THREE.BoxGeometry(
      1,
      DEFAULT_WALL_HEIGHT,
      WALL_THICKNESS
    )
    this.scaleUVs(wallGeo, 1, DEFAULT_WALL_HEIGHT)
    this.templateGeos.set('wall', wallGeo)
  }

  /**
   * Scale UVs uniformly across all faces (matching bakedGeo's approach).
   * U → scaleX, V → scaleY.
   */
  private scaleUVs(geo: THREE.BufferGeometry, scaleX: number, scaleY: number) {
    const uv = geo.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(i, uv.getX(i) * scaleX, uv.getY(i) * scaleY)
    }
  }

  private getBatchKey(
    template: InstanceTemplate,
    textureIndex: number
  ): string {
    return `${template}_${textureIndex}`
  }

  private getOrCreateBatch(
    template: InstanceTemplate,
    textureIndex: number
  ): Batch {
    const key = this.getBatchKey(template, textureIndex)
    let batch = this.batches.get(key)
    if (batch) return batch

    const geo = this.templateGeos.get(template)!
    const mat = getHousingMaterial(textureIndex % HOUSING_TEXTURES.length)
    const capacity = INITIAL_CAPACITY
    const mesh = new THREE.InstancedMesh(geo, mat, capacity)
    // Set count to capacity for initial GPU buffer allocation, then reset to 0.
    // WebGPU allocates the buffer on first render based on count.
    mesh.count = capacity
    mesh.castShadow = true
    mesh.receiveShadow = true
    mesh.frustumCulled = false // instances span large world area
    mesh.name = `housing_inst_${key}`

    // Add to scene; remove/add for WebGPU GPU buffer binding
    this.parent.add(mesh)
    this.parent.remove(mesh)
    this.parent.add(mesh)

    batch = {
      mesh,
      capacity,
      nextSlot: 0,
      freeSlots: [],
      dirty: false,
    }
    this.batches.set(key, batch)
    return batch
  }

  private allocateSlot(batch: Batch): number {
    if (batch.freeSlots.length > 0) {
      return batch.freeSlots.pop()!
    }
    if (batch.nextSlot >= batch.capacity) {
      this.growBatch(batch)
    }
    const idx = batch.nextSlot++
    // Grow rendered count to cover all allocated slots
    if (batch.mesh.count < batch.nextSlot) {
      batch.mesh.count = batch.nextSlot
    }
    return idx
  }

  private growBatch(batch: Batch) {
    const newCapacity = batch.capacity * 2
    const geo = batch.mesh.geometry
    const mat = batch.mesh.material
    const newMesh = new THREE.InstancedMesh(
      geo,
      mat as THREE.Material,
      newCapacity
    )
    newMesh.castShadow = true
    newMesh.receiveShadow = true
    newMesh.frustumCulled = false
    newMesh.name = batch.mesh.name

    // Copy only the used portion of instance matrices
    const usedFloats = batch.nextSlot * 16
    ;(newMesh.instanceMatrix.array as Float32Array).set(
      (batch.mesh.instanceMatrix.array as Float32Array).subarray(0, usedFloats)
    )
    // Set count to full capacity for GPU buffer allocation, then actual count
    // is managed by allocateSlot
    newMesh.count = newCapacity
    newMesh.instanceMatrix.needsUpdate = true

    // Swap meshes in scene
    this.parent.remove(batch.mesh)
    this.parent.add(newMesh)

    batch.mesh = newMesh
    batch.capacity = newCapacity
  }

  /**
   * Add all instance descriptors for a house.
   * Positions in descriptors are local to the house; houseOrigin offsets to world space.
   */
  addHouse(
    houseId: string,
    descriptors: InstanceDescriptor[],
    houseOrigin: { x: number; y: number; z: number }
  ) {
    if (this.houseSlots.has(houseId)) {
      this.removeHouse(houseId)
    }

    const records: SlotRecord[] = []
    for (const desc of descriptors) {
      const batch = this.getOrCreateBatch(desc.template, desc.textureIndex)
      const slotIndex = this.allocateSlot(batch)

      const wx = houseOrigin.x + desc.x
      const wy = houseOrigin.y + desc.y
      const wz = houseOrigin.z + desc.z

      if (desc.rotY !== 0) {
        _tmpMatrix.makeRotationY(desc.rotY)
        _tmpMatrix.setPosition(wx, wy, wz)
      } else {
        _tmpMatrix.makeTranslation(wx, wy, wz)
      }
      batch.mesh.setMatrixAt(slotIndex, _tmpMatrix)
      batch.dirty = true

      records.push({
        batchKey: this.getBatchKey(desc.template, desc.textureIndex),
        slotIndex,
        originalY: wy,
        floorLevel: desc.floorLevel,
        isFront: desc.isFront,
      })
    }
    this.houseSlots.set(houseId, records)
  }

  /** Remove all instances for a house, freeing their slots. */
  removeHouse(houseId: string) {
    const records = this.houseSlots.get(houseId)
    if (!records) return

    for (const rec of records) {
      const batch = this.batches.get(rec.batchKey)
      if (!batch) continue
      batch.mesh.setMatrixAt(rec.slotIndex, _offscreenMatrix)
      batch.freeSlots.push(rec.slotIndex)
      batch.dirty = true
    }
    this.houseSlots.delete(houseId)
  }

  /**
   * Hide instances based on player floor:
   * - Current floor: hide front (south/west walls + roofs)
   * - Floors above: hide everything (front + back)
   */
  setVisibility(houseId: string, playerFloor: number) {
    const records = this.houseSlots.get(houseId)
    if (!records) return

    for (const rec of records) {
      const hide =
        (rec.floorLevel === playerFloor && rec.isFront) ||
        rec.floorLevel > playerFloor
      const batch = this.batches.get(rec.batchKey)
      if (!batch) continue
      const array = batch.mesh.instanceMatrix.array as Float32Array
      array[rec.slotIndex * 16 + 13] = hide ? OFFSCREEN_Y : rec.originalY
      batch.dirty = true
    }
  }

  /** Restore all instances for a house to their original positions. */
  resetVisibility(houseId: string) {
    const records = this.houseSlots.get(houseId)
    if (!records) return

    for (const rec of records) {
      const batch = this.batches.get(rec.batchKey)
      if (!batch) continue
      const array = batch.mesh.instanceMatrix.array as Float32Array
      array[rec.slotIndex * 16 + 13] = rec.originalY
      batch.dirty = true
    }
  }

  /** Apply pending changes to GPU. Call once per frame after all mutations. */
  flush() {
    for (const batch of this.batches.values()) {
      if (!batch.dirty) continue
      batch.mesh.instanceMatrix.needsUpdate = true
      // WebGPU workaround: remove/add to force GPU buffer re-upload
      this.parent.remove(batch.mesh)
      this.parent.add(batch.mesh)
      batch.dirty = false
    }
  }

  /** Clean up all GPU resources. */
  dispose() {
    for (const batch of this.batches.values()) {
      this.parent.remove(batch.mesh)
      // Don't call mesh.dispose() — geometry and material are shared
    }
    this.batches.clear()
    this.houseSlots.clear()
    for (const geo of this.templateGeos.values()) {
      geo.dispose()
    }
    this.templateGeos.clear()
  }
}
