import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'

interface OutfitMask {
  mesh_index: number
  primitive_index: number
  primitive_triangles: number
  hide_triangle_indices: number[]
  vertex_adjustments?: VertexAdjustment[]
  clip_below_y?: TriangleClip
}

interface VertexAdjustment {
  vertex: number
  position: [number, number, number]
  normal: [number, number, number]
}

interface TriangleClip {
  height: number
  outer_start?: number
  outer_slope?: number
  triangle_indices: number[]
}

function clipTriangles(
  geometry: THREE.BufferGeometry,
  clips: TriangleClip[],
  hidden: Set<number>
) {
  const index = geometry.index!
  const position = geometry.attributes.position
  const extras: { a: number; b: number; t: number }[] = []
  const edges = new Map<string, number>()
  const appended: number[] = []
  const clipped = new Set<number>()
  const distance = (x: number, y: number, clip: TriangleClip) =>
    y -
    clip.height -
    Math.max(0, Math.abs(x) - (clip.outer_start ?? Infinity)) *
      (clip.outer_slope ?? 0)
  const crossing = (a: number, b: number, clip: TriangleClip) => {
    const key = `${Math.min(a, b)}:${Math.max(a, b)}:${clip.height}:${clip.outer_start}:${clip.outer_slope}`
    const existing = edges.get(key)
    if (existing !== undefined) return existing
    const ax = position.getX(a),
      ay = position.getY(a)
    const bx = position.getX(b),
      by = position.getY(b)
    let low = 0,
      high = 1
    const insideA = distance(ax, ay, clip) >= 0
    for (let i = 0; i < 24; i++) {
      const t = (low + high) / 2
      if (
        distance(
          THREE.MathUtils.lerp(ax, bx, t),
          THREE.MathUtils.lerp(ay, by, t),
          clip
        ) >=
          0 ===
        insideA
      )
        low = t
      else high = t
    }
    const t = (low + high) / 2
    const result = position.count + extras.length
    extras.push({ a, b, t })
    edges.set(key, result)
    return result
  }
  for (const clip of clips) {
    for (const triangle of clip.triangle_indices) {
      if (hidden.has(triangle) || clipped.has(triangle)) continue
      clipped.add(triangle)
      const vertices = [0, 1, 2].map((i) => index.getX(triangle * 3 + i))
      const polygon: number[] = []
      for (let i = 0; i < 3; i++) {
        const a = vertices[i],
          b = vertices[(i + 1) % 3]
        const insideA = distance(position.getX(a), position.getY(a), clip) >= 0
        const insideB = distance(position.getX(b), position.getY(b), clip) >= 0
        if (insideA) polygon.push(a)
        if (insideA !== insideB) polygon.push(crossing(a, b, clip))
      }
      for (let i = 1; i + 1 < polygon.length; i++)
        appended.push(polygon[0], polygon[i], polygon[i + 1])
    }
  }
  if (!clipped.size) {
    if (hidden.size)
      geometry.setIndex(
        Array.from({ length: index.count }, (_, i) => i)
          .filter((i) => !hidden.has(Math.floor(i / 3)))
          .map((i) => index.getX(i))
      )
    return
  }
  for (const [name, attribute] of Object.entries(geometry.attributes)) {
    const array =
      name === 'skinIndex'
        ? new Uint16Array(
            (attribute.count + extras.length) * attribute.itemSize
          )
        : new Float32Array(
            (attribute.count + extras.length) * attribute.itemSize
          )
    for (let i = 0; i < attribute.count; i++)
      for (let j = 0; j < attribute.itemSize; j++)
        array[i * attribute.itemSize + j] = attribute.getComponent(i, j)
    if (name !== 'skinIndex' && name !== 'skinWeight') {
      for (let i = 0; i < extras.length; i++) {
        const { a, b, t } = extras[i]
        for (let j = 0; j < attribute.itemSize; j++)
          array[(attribute.count + i) * attribute.itemSize + j] =
            THREE.MathUtils.lerp(
              attribute.getComponent(a, j),
              attribute.getComponent(b, j),
              t
            )
      }
    }
    geometry.setAttribute(
      name,
      new THREE.BufferAttribute(array, attribute.itemSize)
    )
  }
  const joint = geometry.attributes.skinIndex
  const weight = geometry.attributes.skinWeight
  for (let i = 0; i < extras.length; i++) {
    const { a, b, t } = extras[i]
    const weights = new Map<number, number>()
    for (const [vertex, factor] of [
      [a, 1 - t],
      [b, t],
    ])
      for (let k = 0; k < 4; k++) {
        const bone = joint.getComponent(vertex, k)
        weights.set(
          bone,
          (weights.get(bone) ?? 0) + weight.getComponent(vertex, k) * factor
        )
      }
    const largest = [...weights.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 4)
    const sum = largest.reduce((value, entry) => value + entry[1], 0)
    for (let k = 0; k < 4; k++) {
      joint.setComponent(position.count + i, k, largest[k]?.[0] ?? 0)
      weight.setComponent(position.count + i, k, (largest[k]?.[1] ?? 0) / sum)
    }
  }
  const kept: number[] = []
  for (let i = 0; i < index.count; i++)
    if (!hidden.has(Math.floor(i / 3)) && !clipped.has(Math.floor(i / 3)))
      kept.push(index.getX(i))
  geometry.setIndex([...kept, ...appended])
}

export function femaleOutfitMask(base?: GLTF) {
  const states = new Map<
    THREE.Mesh,
    {
      original: THREE.BufferGeometry
      covered?: THREE.BufferGeometry
    }
  >()
  const slots = new Map<
    string,
    {
      mesh: THREE.Mesh
      hidden: Set<number>
      adjustments: VertexAdjustment[]
      clip?: TriangleClip
    }
  >()
  const equipped = new Set<string>()
  const apply = () => {
    for (const [mesh, state] of states) {
      const hidden = new Set<number>()
      const adjustments = new Map<number, VertexAdjustment>()
      const clips: TriangleClip[] = []
      for (const name of equipped) {
        const slot = slots.get(name)!
        if (slot.mesh === mesh) {
          for (const index of slot.hidden) hidden.add(index)
          for (const item of slot.adjustments)
            adjustments.set(item.vertex, item)
          if (slot.clip) clips.push(slot.clip)
        }
      }
      mesh.geometry = state.original
      state.covered?.dispose()
      state.covered = undefined
      if (hidden.size || adjustments.size || clips.length) {
        const covered = state.original.clone()
        if (hidden.size && !clips.length)
          covered.setIndex(
            new THREE.BufferAttribute(
              state.original.index!.array.filter(
                (_, offset) => !hidden.has(Math.floor(offset / 3))
              ),
              1
            )
          )
        for (const item of adjustments.values()) {
          covered.attributes.position.setXYZ(item.vertex, ...item.position)
          covered.attributes.normal.setXYZ(item.vertex, ...item.normal)
          const tangent = covered.attributes.tangent
          if (tangent) {
            const n = new THREE.Vector3(...item.normal)
            const t = new THREE.Vector3().fromBufferAttribute(
              tangent,
              item.vertex
            )
            t.addScaledVector(n, -t.dot(n)).normalize()
            tangent.setXYZ(item.vertex, t.x, t.y, t.z)
          }
        }
        if (clips.length) clipTriangles(covered, clips, hidden)
        mesh.geometry = state.covered = covered
      }
    }
  }
  const toggle = (name: string, enabled: boolean) => {
    if (enabled && !slots.has(name))
      throw new Error('의상 가림 정보가 없습니다.')
    if (equipped.has(name) === enabled) return false
    if (enabled) equipped.add(name)
    else equipped.delete(name)
    return true
  }
  return {
    register(name: string, value: unknown, target?: THREE.Mesh) {
      const mask = value as OutfitMask | null
      if (
        !mask ||
        !Number.isInteger(mask.mesh_index) ||
        !Number.isInteger(mask.primitive_index) ||
        !Number.isInteger(mask.primitive_triangles) ||
        !Array.isArray(mask.hide_triangle_indices) ||
        mask.hide_triangle_indices.some(
          (index) =>
            !Number.isInteger(index) ||
            index < 0 ||
            index >= mask.primitive_triangles
        )
      )
        throw new Error('의상 가림 정보가 올바르지 않습니다.')
      let body: THREE.Mesh | undefined = target
      if (!target)
        base?.scene.traverse((node) => {
          const association = base.parser.associations.get(node)
          if (
            node instanceof THREE.Mesh &&
            association?.meshes === mask.mesh_index &&
            association.primitives === mask.primitive_index
          )
            body = node
        })
      const original = body && (states.get(body)?.original ?? body.geometry)
      const index = original?.index
      if (
        !body ||
        !original ||
        !index ||
        index.count !== mask.primitive_triangles * 3
      )
        throw new Error('몸체와 의상 가림 정보가 일치하지 않습니다.')
      if (slots.has(name))
        throw new Error('의상 가림 정보가 이미 등록되었습니다.')
      const adjustments = mask.vertex_adjustments ?? []
      const clip = mask.clip_below_y
      if (
        !Array.isArray(adjustments) ||
        adjustments.some(
          (item) =>
            !item ||
            !Number.isInteger(item.vertex) ||
            item.vertex < 0 ||
            item.vertex >= original.attributes.position.count ||
            !Array.isArray(item.position) ||
            item.position.length !== 3 ||
            !item.position.every(Number.isFinite) ||
            !Array.isArray(item.normal) ||
            item.normal.length !== 3 ||
            !item.normal.every(Number.isFinite)
        ) ||
        (adjustments.length > 0 && !original.attributes.normal)
      )
        throw new Error('의상 정점 보정 정보가 올바르지 않습니다.')
      if (
        clip &&
        (!Number.isFinite(clip.height) ||
          !Array.isArray(clip.triangle_indices) ||
          (clip.outer_start !== undefined &&
            (!Number.isFinite(clip.outer_start) || clip.outer_start < 0)) ||
          (clip.outer_slope !== undefined &&
            (!Number.isFinite(clip.outer_slope) || clip.outer_slope < 0)) ||
          clip.triangle_indices.some(
            (i) =>
              !Number.isInteger(i) || i < 0 || i >= mask.primitive_triangles
          ) ||
          !original.attributes.skinIndex ||
          !original.attributes.skinWeight)
      )
        throw new Error('의상 경계 정보가 올바르지 않습니다.')
      if (!states.has(body)) states.set(body, { original })
      slots.set(name, {
        mesh: body,
        hidden: new Set(mask.hide_triangle_indices),
        adjustments,
        clip,
      })
    },
    setEquipped(name: string, enabled: boolean) {
      if (toggle(name, enabled)) apply()
    },
    setEquippedAll(entries: Record<string, boolean>) {
      let changed = false
      for (const [name, enabled] of Object.entries(entries))
        changed = toggle(name, enabled) || changed
      if (changed) apply()
    },
    unregister(name: string) {
      const wasEquipped = equipped.delete(name)
      slots.delete(name)
      if (wasEquipped) apply()
    },
    dispose() {
      for (const [mesh, state] of states) {
        mesh.geometry = state.original
        state.covered?.dispose()
      }
      states.clear()
      slots.clear()
      equipped.clear()
    },
  }
}
