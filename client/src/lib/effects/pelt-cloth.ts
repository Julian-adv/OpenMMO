import * as THREE from 'three'
import type { PeltPhysics } from './pelt-rig'
import type { WindSample } from '../shaders/grass-material'

type Contact = (point: THREE.Vector3, outward: THREE.Vector3) => void
const STEP = 1 / 60
const ITERATIONS = 4

export function createPeltCloth(
  geometry: THREE.BufferGeometry,
  original: THREE.BufferGeometry,
  config: PeltPhysics
) {
  const { columns, rows, pinned_rows: pinnedRows } = config.cloth!
  const count = columns * rows
  const pinned = columns * pinnedRows
  const positions = geometry.getAttribute('position') as THREE.BufferAttribute
  if (positions.count !== count * 2) throw new Error('Invalid pelt cloth grid')
  const rest = original.getAttribute('position') as THREE.BufferAttribute
  const normals = geometry.getAttribute('normal') as THREE.BufferAttribute
  const restNormals = original.getAttribute('normal') as THREE.BufferAttribute
  geometry.deleteAttribute('tangent')
  const points = Array.from({ length: count }, () => new THREE.Vector3())
  const previous = points.map(() => new THREE.Vector3())
  const targets = points.map(() => new THREE.Vector3())
  const directions = points.map((_, i) => {
    const v = new THREE.Vector3().fromBufferAttribute(rest, i)
    return v
      .sub(new THREE.Vector3().fromBufferAttribute(rest, i + count))
      .normalize()
  })
  const outward = directions.map(() => new THREE.Vector3())
  const thickness = new Float32Array(count)
  const segmentLengths = new Float32Array(count)
  const contacts = new Uint8Array(count)
  const links: { a: number; b: number; length: number; stiffness: number }[] =
    []
  const local = new THREE.Vector3()
  const delta = new THREE.Vector3()
  const normal = new THREE.Vector3()
  const down = new THREE.Vector3()
  const next = new THREE.Vector3()
  const inverse = new THREE.Matrix4()
  const fur = config.kind === 'fur'
  let scale = 1
  let accumulator = 0
  for (let i = 0; i < count; i++) {
    thickness[i] = local
      .fromBufferAttribute(rest, i)
      .distanceTo(delta.fromBufferAttribute(rest, i + count))
  }
  for (let i = columns; i < count; i++)
    segmentLengths[i] = local
      .fromBufferAttribute(rest, i)
      .distanceTo(delta.fromBufferAttribute(rest, i - columns))
  const orientation =
    local
      .fromBufferAttribute(rest, 1)
      .sub(delta.fromBufferAttribute(rest, 0))
      .cross(down.fromBufferAttribute(rest, columns).sub(delta))
      .dot(directions[0]) >= 0
      ? 1
      : -1
  const link = (a: number, b: number, stiffness: number) => {
    links.push({
      a,
      b,
      stiffness,
      length: local
        .fromBufferAttribute(rest, a)
        .distanceTo(delta.fromBufferAttribute(rest, b)),
    })
  }
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < columns; col++) {
      const i = row * columns + col
      if (col + 1 < columns) link(i, i + 1, 1)
      if (col + 2 < columns) link(i, i + 2, 0.6)
      if (row + 1 < rows) {
        link(i, i + columns, 1)
        if (col + 1 < columns) {
          link(i, i + columns + 1, 0.65)
          link(i + 1, i + columns, 0.65)
        }
      }
      if (row + 2 < rows) link(i, i + columns * 2, fur ? 0.28 : 0.12)
    }
  }

  function refresh(frame: THREE.Matrix4) {
    scale = frame.getMaxScaleOnAxis()
    inverse.copy(frame).invert()
    for (let i = 0; i < count; i++) {
      targets[i].fromBufferAttribute(rest, i).applyMatrix4(frame)
      outward[i].copy(directions[i]).transformDirection(frame)
    }
  }

  function constrain(contact: Contact) {
    contacts.fill(0)
    for (let iteration = 0; iteration < ITERATIONS; iteration++) {
      for (const { a, b, length, stiffness } of links) {
        const wa = a < pinned ? 0 : 1,
          wb = b < pinned ? 0 : 1
        if (wa + wb === 0) continue
        delta.copy(points[b]).sub(points[a])
        const distance = delta.length()
        if (distance < 1e-8) continue
        delta.multiplyScalar(
          (((distance - length * scale) / distance) * stiffness) / (wa + wb)
        )
        if (wa) points[a].add(delta)
        if (wb) points[b].sub(delta)
      }
      for (let i = pinned; i < count; i++) {
        local.copy(points[i])
        contact(points[i], outward[i])
        if (points[i].distanceToSquared(local) > 1e-12) contacts[i] = 1
      }
    }
  }

  function preserveLengths() {
    for (let i = pinned; i < count; i++) {
      delta.copy(points[i]).sub(points[i - columns])
      const maximum = segmentLengths[i] * scale * 1.04
      if (delta.lengthSq() > maximum * maximum)
        points[i].copy(points[i - columns]).add(delta.setLength(maximum))
    }
  }

  function deform() {
    for (let i = 0; i < count; i++) {
      local.copy(points[i]).applyMatrix4(inverse)
      if (i < pinned) local.fromBufferAttribute(rest, i)
      positions.setXYZ(i, local.x, local.y, local.z)
    }
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < columns; col++) {
        const i = row * columns + col
        local.fromBufferAttribute(positions, i)
        if (i < pinned) next.fromBufferAttribute(rest, i + count)
        else {
          normal
            .fromBufferAttribute(
              positions,
              row * columns + Math.min(col + 1, columns - 1)
            )
            .sub(
              delta.fromBufferAttribute(
                positions,
                row * columns + Math.max(col - 1, 0)
              )
            )
          down
            .fromBufferAttribute(
              positions,
              Math.min(row + 1, rows - 1) * columns + col
            )
            .sub(
              delta.fromBufferAttribute(
                positions,
                Math.max(row - 1, 0) * columns + col
              )
            )
          normal.cross(down).normalize().multiplyScalar(orientation)
          next.copy(local).addScaledVector(normal, -thickness[i])
        }
        positions.setXYZ(i + count, next.x, next.y, next.z)
      }
    }
    positions.needsUpdate = true
    geometry.computeVertexNormals()
    for (let i = 0; i < pinned; i++) {
      for (const index of [i, i + count]) {
        normal.fromBufferAttribute(restNormals, index)
        normals.setXYZ(index, normal.x, normal.y, normal.z)
      }
    }
  }

  function reset(frame: THREE.Matrix4, contact: Contact) {
    refresh(frame)
    for (let i = 0; i < count; i++) points[i].copy(targets[i])
    constrain(contact)
    preserveLengths()
    for (let i = 0; i < count; i++) previous[i].copy(points[i])
    accumulator = 0
    deform()
  }

  function update(
    frame: THREE.Matrix4,
    dt: number,
    wind: WindSample | null,
    contact: Contact,
    step = STEP
  ) {
    accumulator = Math.min(accumulator + dt, step * 4)
    if (accumulator + 1e-10 < step) return
    const damping = Math.exp(-(fur ? 48 : 24) * step)
    const step2 = step * step
    refresh(frame)
    const push = wind
      ? wind.windStrength * (fur ? 0.06 : 0.12) * scale * step2
      : 0
    while (accumulator + 1e-10 >= step) {
      accumulator -= step
      for (let i = 0; i < count; i++) {
        if (i < pinned) {
          points[i].copy(targets[i])
          previous[i].copy(targets[i])
          continue
        }
        next
          .copy(points[i])
          .sub(previous[i])
          .multiplyScalar(damping)
          .add(points[i])
          .addScaledVector(
            delta.copy(targets[i]).sub(points[i]),
            (fur ? 40 : 18) * step2
          )
        next.y -= 12 * scale * step2
        if (wind) {
          next.x += wind.windDirX * push
          next.z += wind.windDirZ * push
        }
        const row = Math.floor(i / columns)
        delta
          .copy(next)
          .sub(targets[i])
          .clampLength(
            0,
            (0.025 + (row / (rows - 1)) * config.length * (fur ? 0.55 : 0.7)) *
              scale
          )
        previous[i].copy(points[i])
        points[i].copy(targets[i]).add(delta)
      }
      constrain(contact)
      preserveLengths()
      // Contact corrections must not become velocity on the next step.
      for (let i = pinned; i < count; i++)
        if (contacts[i]) previous[i].copy(points[i])
    }
    deform()
  }
  return { reset, update }
}
