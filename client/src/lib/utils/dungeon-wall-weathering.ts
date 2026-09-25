import * as THREE from 'three'
import { DecalGeometry } from 'three/examples/jsm/geometries/DecalGeometry.js'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { createRng } from './simplex-noise'

const MOSS = 0
const CRACK = 1
const LEAK = 2
const WEB = 3

export interface DungeonWallSurface {
  alongX: boolean
  lo: number
  hi: number
  boundary: number
  inward: number
  height: number
  seed: number
}

interface WallFace {
  a: number
  b: number
  c: number
  minAlong: number
  maxAlong: number
  minY: number
  maxY: number
}

export function buildDungeonWallWeathering(
  wall: THREE.BufferGeometry,
  { alongX, lo, hi, boundary, inward, height, seed }: DungeonWallSurface
): THREE.BufferGeometry | null {
  const length = hi - lo
  if (length < 0.9 || height < 0.7) return null
  const random = createRng(
    seed ^
      Math.imul(Math.round(lo * 100), 374761393) ^
      Math.imul(Math.round(boundary * 100), 668265263) ^
      (alongX ? 8191 : 131071) ^
      (inward > 0 ? 7919 : 104729)
  )
  const positions = wall.getAttribute('position')
  const normals = wall.getAttribute('normal')
  const indices = wall.getIndex()!
  if (!wall.boundingBox) wall.computeBoundingBox()
  const baseY = wall.boundingBox!.min.y
  const parts: THREE.BufferGeometry[][] = [[], []]
  const a = new THREE.Vector3()
  const b = new THREE.Vector3()
  const c = new THREE.Vector3()
  const forward = new THREE.Vector3(0, 0, 1)
  const slots = Math.min(10, Math.max(1, Math.floor(length / 2.3)))

  for (const side of [inward, -inward]) {
    const normal = new THREE.Vector3(alongX ? 0 : side, 0, alongX ? side : 0)
    const faces: WallFace[] = []
    for (let i = 0; i < indices.count; i += 3) {
      const ia = indices.getX(i)
      const ib = indices.getX(i + 1)
      const ic = indices.getX(i + 2)
      a.fromBufferAttribute(positions, ia)
      b.fromBufferAttribute(positions, ib).sub(a)
      c.fromBufferAttribute(positions, ic).sub(a)
      if (b.cross(c).normalize().dot(normal) <= 0.35) continue
      const alongA = alongX ? positions.getX(ia) : positions.getZ(ia)
      const alongB = alongX ? positions.getX(ib) : positions.getZ(ib)
      const alongC = alongX ? positions.getX(ic) : positions.getZ(ic)
      const yA = positions.getY(ia)
      const yB = positions.getY(ib)
      const yC = positions.getY(ic)
      faces.push({
        a: ia,
        b: ib,
        c: ic,
        minAlong: Math.min(alongA, alongB, alongC),
        maxAlong: Math.max(alongA, alongB, alongC),
        minY: Math.min(yA, yB, yC),
        maxY: Math.max(yA, yB, yC),
      })
    }

    const patch = (
      kind: number,
      along: number,
      y: number,
      width: number,
      patchHeight: number
    ) => {
      const angle = kind === LEAK || kind === WEB ? 0 : (random() - 0.5) * 1.1
      const rotation = new THREE.Euler().setFromQuaternion(
        new THREE.Quaternion()
          .setFromUnitVectors(forward, normal)
          .multiply(new THREE.Quaternion().setFromAxisAngle(forward, angle))
      )
      const radius = Math.hypot(width, patchHeight) / 2
      const selected: number[] = []
      for (const face of faces) {
        if (
          face.maxAlong < along - radius ||
          face.minAlong > along + radius ||
          face.maxY < baseY + y - radius ||
          face.minY > baseY + y + radius
        )
          continue
        selected.push(face.a, face.b, face.c)
      }
      if (selected.length === 0) return
      const surface = new THREE.BufferGeometry()
      surface.setAttribute('position', positions)
      surface.setAttribute('normal', normals)
      surface.setIndex(selected)
      const geo = new DecalGeometry(
        new THREE.Mesh(surface),
        new THREE.Vector3(
          alongX ? along : boundary,
          baseY + y,
          alongX ? boundary : along
        ),
        rotation,
        new THREE.Vector3(width, patchHeight, 0.7)
      )
      surface.dispose()
      const uv = geo.getAttribute('uv')
      if (uv.count === 0) {
        geo.dispose()
        return
      }
      const row = random() < 0.5 ? 0 : 1
      const flip = random() < 0.5
      const strength =
        kind === CRACK
          ? 0.94 + random() * 0.06
          : (kind === LEAK ? 0.5 : 0.65) + random() * 0.25
      const tint =
        kind === CRACK ? 0.55 + random() * 0.15 : 0.82 + random() * 0.18
      const detail = kind === CRACK || kind === WEB
      const column = detail ? (kind === WEB ? 1 : 0) : kind
      const colors = new THREE.Float32BufferAttribute(
        new Float32Array(uv.count * 4),
        4
      )
      for (let i = 0; i < uv.count; i++) {
        const u = flip ? 1 - uv.getX(i) : uv.getX(i)
        uv.setXY(
          i,
          (column + 0.002 + u * 0.996) / (detail ? 2 : 3),
          (1 - row + 0.002 + uv.getY(i) * 0.996) / 2
        )
        colors.setXYZW(i, tint, tint, tint, strength)
      }
      geo.setAttribute('color', colors)
      parts[detail ? 1 : 0].push(geo)
    }

    for (let slot = 0; slot < slots; slot++) {
      if (random() < 0.15) continue
      const along = lo + ((slot + 0.2 + random() * 0.6) / slots) * length
      const kind = Math.floor(random() * 3)
      const width = Math.min(
        length * 0.95,
        (kind === CRACK ? 1.6 : 0.9) + random() * 1.4
      )
      const patchHeight = Math.min(
        height,
        kind === LEAK
          ? 1.6 + random() * 1.3
          : (kind === CRACK ? 1.5 : 0.9) + random() * 1.1
      )
      const y =
        kind === LEAK
          ? height * 1.035 - patchHeight * 0.37
          : kind === MOSS
            ? patchHeight * (0.25 + random() * 0.35)
            : random() < 0.5
              ? patchHeight * 0.12
              : height - patchHeight * 0.12
      patch(kind, along, y, width, patchHeight)
      if (kind === LEAK && random() < 0.6) {
        patch(
          MOSS,
          along + (random() - 0.5) * width * 0.4,
          Math.max(0.3, y - patchHeight * 0.4),
          width * 0.8,
          0.7 + random() * 0.5
        )
      }
    }
    for (const end of [lo, hi]) {
      if (random() > 0.45) continue
      const width = Math.min(length * 0.75, 0.9 + random() * 0.7)
      const webHeight = Math.min(height * 0.5, 0.85 + random() * 0.6)
      patch(
        WEB,
        end + (end === lo ? 1 : -1) * width * 0.36,
        height - webHeight * 0.46,
        width,
        webHeight
      )
    }
  }
  const allParts = parts.flat()
  if (allParts.length === 0) return null
  const geo = mergeGeometries(allParts, false)!
  let start = 0
  for (const [materialIndex, batch] of parts.entries()) {
    const count = batch.reduce(
      (sum, part) => sum + part.getAttribute('position').count,
      0
    )
    if (count > 0) geo.addGroup(start, count, materialIndex)
    start += count
  }
  for (const part of allParts) part.dispose()
  geo.computeBoundingBox()
  return geo
}
