import * as THREE from 'three'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { tuckSleevesIntoRangerGloves } from './rangerGloveCuff'
import { rangerBootCuff } from './rangerBootCuff'
import { meshIslands } from './meshIslands'
import { priestBootCuff } from './priestBootCuff'
import underwearSeat from '../data/underwearSeat.json'
import { bardBootCuff } from './bardBootCuff'
import { cavemanBootCuff } from './cavemanBootCuff'
import { blendSkinToJoint } from './skinWeights'

type Cut =
  | 'bard_hat_hair'
  | 'priest_hat_hair'
  | 'priest_hat_lappets'
  | 'bracers'
  | 'caveman_sleeves'
  | 'gauntlets'
  | 'gloves'
  | 'ranger_gloves'
  | 'ranger_sleeves'
  | 'tuxedo_glove_cuff'
  | 'tuxedo_gloves'
  | 'tuxedo_glove_rim'
  | 'greaves'
  | 'leather_boots'
  | 'plate_boots'
  | 'barbarian_pants_boots'
  | 'caveman_pants_boots'
  | 'tall_boots'
  | 'ranger_pants_boots'
  | 'priest_pants_boots'
  | 'bard_pants_boots'
  | 'collar'
  | 'tripo_collar'
  | 'ranger_collar'
  | 'tripo_waist'
  | 'bard_waist'
  | 'bard_rear_hem'
  | 'bard_underwear_hem'
  | 'bard_plate_hem'
  | 'bard_barbarian_hem'
  | 'bard_caveman_hem'
  | 'bard_fitted_pants_hem'
  | 'bard_matching_pants_hem'
  | 'tripo_covered_waist'
  | 'ranger_plate_waist'
  | 'bard_plate_waist'
  | 'priest_plate_waist'
  | 'priest_fur'
  | 'rogue_pockets'
  | 'tripo_pants_waist'
  | 'linen_untucked'
  | 'linen_waist'
  | 'underwear_seat'
type MaybeCut = Cut | false | undefined
type Distance = (point: THREE.Vector3) => number
type Reshape = {
  reshape: (geometry: THREE.BufferGeometry) => THREE.BufferGeometry
}
type Clip = Distance | Distance[]
type Cuff = { cuff: typeof bardBootCuff }
type HatHair = {
  clip: Distance
  then: (geometry: THREE.BufferGeometry) => void
}

// Distance toward the elbow from a plane `fraction` of the way to the wrist.
function forearmPlane(fraction: number): Distance {
  const planes = [1, -1].map((side) => {
    const elbow = new THREE.Vector3(side * 0.311339, 1.247962, -0.056036)
    const wrist = new THREE.Vector3(side * 0.445653, 0.993593, -0.045557)
    return new THREE.Plane().setFromNormalAndCoplanarPoint(
      elbow.clone().sub(wrist).normalize(),
      elbow.lerp(wrist, fraction)
    )
  })
  return (point) => planes[point.x < 0 ? 1 : 0].distanceToPoint(point)
}

function sleeveCut(fraction: number): Distance {
  const plane = forearmPlane(fraction)
  return (point) => Math.max(0.28 - Math.abs(point.x), plane(point))
}

const tuxedoGloveCuff = forearmPlane(0.97)
const tuxedoGloveRim = forearmPlane(0.9)

const rangerGloves = sleeveCut(0.4)

const cuts: Record<Cut, Clip | Reshape | Cuff | HatHair> = {
  bard_hat_hair: { clip: (point) => 1.815 - point.y, then: tuckHairUnderHat },
  priest_hat_hair: { clip: (point) => 1.82 - point.y, then: tuckHairUnderHat },
  priest_hat_lappets: { reshape: drapePriestLappetsOverHair },
  bracers: sleeveCut(0.23),
  caveman_sleeves: sleeveCut(0.19),
  gauntlets: sleeveCut(0.85),
  gloves: sleeveCut(0.89),
  ranger_gloves: rangerGloves,
  ranger_sleeves: rangerGloves,
  // Hiding the whole hand left a jagged wrist.
  tuxedo_gloves: sleeveCut(0.92),
  // The generated rim is dark and torn.
  tuxedo_glove_rim: (point) => -tuxedoGloveRim(point),
  // The cuff is wider than the jacket sleeve.
  tuxedo_glove_cuff: (point) => -tuxedoGloveCuff(point),
  greaves: (point) => point.y - 0.46,
  leather_boots: (point) => point.y - 0.235,
  plate_boots: (point) => point.y - 0.22,
  barbarian_pants_boots: (point) => point.y - 0.479,
  caveman_pants_boots: { cuff: cavemanBootCuff },
  tall_boots: (point) => point.y - 0.43,
  ranger_pants_boots: { cuff: rangerBootCuff },
  priest_pants_boots: { cuff: priestBootCuff },
  bard_pants_boots: { cuff: bardBootCuff },
  collar: (point) => Math.max(1.61 - point.y, Math.abs(point.x) - 0.075),
  tripo_collar: (point) => Math.max(1.54 - point.y, Math.abs(point.x) - 0.075),
  ranger_collar: [
    (point) => 1.54 - point.y,
    (point) => 1.54 + 0.8 * (point.x - 0.04) - point.y,
    (point) => 1.54 + 0.8 * (-point.x - 0.04) - point.y,
    (point) => point.x - 0.09,
    (point) => -point.x - 0.09,
  ],
  tripo_waist: (point) => point.y - 1.14,
  bard_waist: (point) =>
    point.y -
    THREE.MathUtils.lerp(
      1.08,
      1.16,
      THREE.MathUtils.smoothstep(-point.z, 0.02, 0.07)
    ),
  bard_rear_hem: { reshape: bardHem(0.025) },
  bard_underwear_hem: { reshape: bardHem(0.045, 0.02) },
  bard_plate_hem: { reshape: bardHem(0.045, 0.045, true) },
  bard_barbarian_hem: { reshape: bardHem(0.045, 0.09, true) },
  bard_caveman_hem: { reshape: bardHem(0.085, 0.13, true) },
  bard_fitted_pants_hem: { reshape: bardHem(0.065, 0.075, true) },
  bard_matching_pants_hem: { reshape: bardHem(0.095, 0.035, true) },
  tripo_covered_waist: (point) => point.y - 1.105,
  ranger_plate_waist: (point) => {
    const rear = 1 - THREE.MathUtils.smoothstep(point.z, -0.06, -0.02)
    return 1.06 - rear * plateWaistDrop(point.x) - point.y
  },
  bard_plate_waist: (point) =>
    1.06 -
    plateWaistDrop(point.x) +
    0.06 * (1 - THREE.MathUtils.smoothstep(Math.abs(point.x), 0.03, 0.15)) -
    point.y,
  priest_plate_waist: { reshape: tuckWaist(-0.035, 0.23, [0.17, 0.1], false) },
  priest_fur: { reshape: tuckWaist(-0.015, 0.22, [0.145, 0.145], true) },
  rogue_pockets: { reshape: removeRoguePockets },
  tripo_pants_waist: (point) => 1.105 - point.y,
  linen_untucked: { reshape: untuckLinen },
  linen_waist: (point) => point.y - 1.09,
  underwear_seat: { reshape: reshapeUnderwearSeat },
}

function plateWaistDrop(x: number) {
  return Math.max(0, 0.09 - Math.abs(x) * 0.5)
}

function bardHem(rearClearance: number, sideClearance = 0, overPants = false) {
  return (source: THREE.BufferGeometry) => {
    const geometry = source.clone()
    const positions = geometry.getAttribute('position')
    const original = source.getAttribute('position')
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i)
      const y = positions.getY(i)
      const z = positions.getZ(i)
      const hem =
        (1 - THREE.MathUtils.smoothstep(y, 1.05, 1.17)) *
        (1 - THREE.MathUtils.smoothstep(Math.abs(x), 0.23, 0.29))
      if (!hem) continue
      const inner = THREE.MathUtils.smoothstep(Math.abs(x), 0.06, 0.14)
      const rear = THREE.MathUtils.smoothstep(-z, 0.04, 0.12) * hem
      const side = overPants
        ? THREE.MathUtils.smoothstep(-z, -0.14, 0.06) * hem
        : rear
      const front = overPants
        ? THREE.MathUtils.smoothstep(z, -0.02, 0.06) * hem * inner * 0.02
        : 0
      positions.setZ(i, z - rearClearance * rear + front)
      if (sideClearance)
        positions.setX(i, x + Math.sign(x) * sideClearance * side * inner)
    }
    return finishReshape(
      source,
      geometry,
      (i) =>
        positions.getX(i) === original.getX(i) &&
        positions.getZ(i) === original.getZ(i)
    )
  }
}

function tuckBardWaist(
  source: THREE.BufferGeometry,
  skeleton?: THREE.Skeleton
) {
  const geometry = source.clone()
  const positions = geometry.getAttribute('position')
  const original = source.getAttribute('position')
  const joints = geometry.getAttribute('skinIndex')
  const weights = geometry.getAttribute('skinWeight')
  const hips = skeleton?.bones.findIndex((bone) => bone.name === 'Hips') ?? -1
  for (let i = 0; i < positions.count; i++) {
    const x = original.getX(i),
      y = original.getY(i),
      z = original.getZ(i)
    const hem = 1.06 - plateWaistDrop(x)
    const amount = THREE.MathUtils.smoothstep(y, hem, hem + 0.025)
    const radius = Math.hypot(x / 0.16, (z + 0.035) / 0.1)
    if (amount && radius > 1) {
      const scale = THREE.MathUtils.lerp(1, 1 / radius, amount)
      positions.setXYZ(i, x * scale, y, (z + 0.035) * scale - 0.035)
    }
    if (z < -0.035) {
      const rear = THREE.MathUtils.smoothstep(y, 0.84, 0.96)
      const depth = -0.035 + (z + 0.035) * (1 - 0.55 * rear)
      positions.setZ(i, Math.max(positions.getZ(i), depth))
      const follow = THREE.MathUtils.smoothstep(y, 0.9, 1.01)
      if (hips >= 0 && follow && joints && weights)
        blendSkinToJoint(joints, weights, i, hips, follow)
    }
  }
  return finishReshape(
    source,
    geometry,
    (i) =>
      positions.getX(i) === original.getX(i) &&
      positions.getZ(i) === original.getZ(i)
  )
}

function topInfluences(influences: Map<number, number>) {
  const sorted = [...influences].sort((a, b) => b[1] - a[1]).slice(0, 4)
  const total = sorted.reduce((sum, [, weight]) => sum + weight, 0)
  return Array.from({ length: 4 }, (_, i) => [
    sorted[i]?.[0] ?? 0,
    total ? (sorted[i]?.[1] ?? 0) / total : 0,
  ])
}

function finishReshape(
  source: THREE.BufferGeometry,
  geometry: THREE.BufferGeometry,
  untouched?: (index: number) => boolean
) {
  geometry.deleteAttribute('tangent')
  geometry.computeVertexNormals()
  const normals = geometry.getAttribute('normal')
  const originalNormals = source.getAttribute('normal')
  if (originalNormals && untouched)
    for (let i = 0; i < normals.count; i++)
      if (untouched(i))
        normals.setXYZ(
          i,
          originalNormals.getX(i),
          originalNormals.getY(i),
          originalNormals.getZ(i)
        )
  geometry.boundingBox = null
  geometry.boundingSphere = null
  return geometry
}

function reshapeUnderwearSeat(source: THREE.BufferGeometry) {
  const geometry = source.clone()
  const positions = geometry.getAttribute('position') as THREE.BufferAttribute
  const { depths, x_step: stepX, y_min: minY, y_step: stepY } = underwearSeat
  let changed = false
  for (let i = 0; i < positions.count; i++) {
    const x = positions.getX(i),
      y = positions.getY(i),
      z = positions.getZ(i)
    const amount =
      THREE.MathUtils.smoothstep(y, 0.76, 0.86) *
      (1 - THREE.MathUtils.smoothstep(y, 1.14, 1.2)) *
      THREE.MathUtils.smoothstep(-z, 0.005, 0.065) *
      (1 - THREE.MathUtils.smoothstep(Math.abs(x), 0.13, 0.2))
    if (!amount) continue
    const gx = Math.min(Math.abs(x) / stepX, depths[0].length - 1)
    const gy = THREE.MathUtils.clamp((y - minY) / stepY, 0, depths.length - 1)
    const ix = Math.min(Math.floor(gx), depths[0].length - 2)
    const iy = Math.min(Math.floor(gy), depths.length - 2)
    const values = [
      depths[iy][ix],
      depths[iy][ix + 1],
      depths[iy + 1][ix],
      depths[iy + 1][ix + 1],
    ]
    if (values.some((value) => value === null)) continue
    const [a, b, c, d] = values as number[]
    const target = THREE.MathUtils.lerp(
      THREE.MathUtils.lerp(a, b, gx - ix),
      THREE.MathUtils.lerp(c, d, gx - ix),
      gy - iy
    )
    positions.setZ(i, THREE.MathUtils.lerp(z, target, amount))
    changed ||= positions.getZ(i) !== z
  }
  if (!changed) {
    geometry.dispose()
    return source
  }
  const original = source.getAttribute('position')
  return finishReshape(
    source,
    geometry,
    (i) => positions.getZ(i) === original.getZ(i)
  )
}

export function clipSkinnedGeometry(
  source: THREE.BufferGeometry,
  distance: Distance,
  refineIntersection = false
): THREE.BufferGeometry {
  const attributes = Object.entries(source.attributes)
  const values = Object.fromEntries(
    attributes.map(([name]) => [name, [] as number[]])
  )
  const vertices = new Map<number, number>()
  const edges = new Map<string, number>()
  const indices: number[] = []
  const point = new THREE.Vector3()
  const first = new THREE.Vector3()
  const last = new THREE.Vector3()
  const position = source.getAttribute('position')
  const distances = Array.from({ length: position.count }, (_, i) =>
    distance(point.fromBufferAttribute(position, i))
  )
  const vertex = (index: number) => {
    const existing = vertices.get(index)
    if (existing !== undefined) return existing
    const next = values.position.length / 3
    for (const [name, attribute] of attributes)
      for (let i = 0; i < attribute.itemSize; i++)
        values[name].push(attribute.getComponent(index, i))
    vertices.set(index, next)
    return next
  }
  const intersect = (a: number, b: number) => {
    if (distances[a] === 0) return vertex(a)
    if (distances[b] === 0) return vertex(b)
    if (a > b) [a, b] = [b, a]
    const key = `${a}:${b}`
    const existing = edges.get(key)
    if (existing !== undefined) return existing
    const next = values.position.length / 3
    let t = distances[a] / (distances[a] - distances[b])
    if (refineIntersection) {
      first.fromBufferAttribute(position, a)
      last.fromBufferAttribute(position, b)
      let low = 0,
        high = 1
      for (let i = 0; i < 20; i++) {
        t = (low + high) / 2
        const inside = distance(point.copy(first).lerp(last, t)) >= 0
        if (inside === distances[a] >= 0) low = t
        else high = t
      }
    }
    for (const [name, attribute] of attributes) {
      if (name === 'skinIndex' || name === 'skinWeight') continue
      const interpolated = Array.from({ length: attribute.itemSize }, (_, i) =>
        THREE.MathUtils.lerp(
          attribute.getComponent(a, i),
          attribute.getComponent(b, i),
          t
        )
      )
      if (name === 'normal' || name === 'tangent') {
        point.fromArray(interpolated).normalize().toArray(interpolated)
        if (name === 'tangent')
          interpolated[3] = attribute.getComponent(t < 0.5 ? a : b, 3)
      }
      values[name].push(...interpolated)
    }
    if (source.hasAttribute('skinIndex') && source.hasAttribute('skinWeight')) {
      const weights = new Map<number, number>()
      for (const [index, factor] of [
        [a, 1 - t],
        [b, t],
      ])
        for (let i = 0; i < 4; i++) {
          const joint = source.attributes.skinIndex.getComponent(index, i)
          const weight =
            source.attributes.skinWeight.getComponent(index, i) * factor
          weights.set(joint, (weights.get(joint) ?? 0) + weight)
        }
      for (const [joint, weight] of topInfluences(weights)) {
        values.skinIndex.push(joint)
        values.skinWeight.push(weight)
      }
    }
    edges.set(key, next)
    return next
  }
  const geometry = new THREE.BufferGeometry()
  const count = source.index?.count ?? position.count
  for (const group of source.groups.length
    ? source.groups
    : [{ start: 0, count }]) {
    const start = indices.length
    for (
      let i = group.start;
      i < Math.min(group.start + group.count, count);
      i += 3
    ) {
      const face = [0, 1, 2].map(
        (offset) => source.index?.getX(i + offset) ?? i + offset
      )
      const polygon: number[] = []
      for (let j = 0; j < 3; j++) {
        const a = face[j],
          b = face[(j + 1) % 3]
        if (distances[a] >= 0) polygon.push(vertex(a))
        if (distances[a] >= 0 !== distances[b] >= 0)
          polygon.push(intersect(a, b))
      }
      for (let j = 1; j + 1 < polygon.length; j++) {
        const a = polygon[0],
          b = polygon[j],
          c = polygon[j + 1]
        if (a !== b && a !== c && b !== c) indices.push(a, b, c)
      }
    }
    if (source.groups.length)
      geometry.addGroup(start, indices.length - start, group.materialIndex)
  }
  for (const [name, attribute] of attributes) {
    const clipped =
      name === 'skinIndex'
        ? new THREE.Uint16BufferAttribute(values[name], attribute.itemSize)
        : new THREE.Float32BufferAttribute(values[name], attribute.itemSize)
    geometry.setAttribute(name, clipped)
  }
  geometry.setIndex(indices)
  return geometry
}

interface Variants {
  source: THREE.BufferGeometry
  cuts: Map<string, THREE.BufferGeometry>
}

const variants = new WeakMap<THREE.BufferGeometry, Variants>()

function setGeometry(mesh: THREE.SkinnedMesh, geometry: THREE.BufferGeometry) {
  if (mesh.geometry === geometry) return
  const layout = Object.keys(mesh.geometry.attributes).join()
  mesh.geometry = geometry
  // Pipelines are keyed by attribute names; rebuild only when they change.
  if (Object.keys(geometry.attributes).join() !== layout) mesh.dispose()
  const bounds: {
    boundingBox: THREE.Box3 | null
    boundingSphere: THREE.Sphere | null
  } = mesh
  bounds.boundingBox = null
  bounds.boundingSphere = null
}

export function trimModularClothing(
  mesh: THREE.SkinnedMesh,
  cut?: MaybeCut | readonly MaybeCut[],
  skin = false
) {
  const applied = [cut ?? []].flat().filter((name): name is Cut => !!name)
  let cached = variants.get(mesh.geometry)
  if (!applied.length) {
    if (cached) setGeometry(mesh, cached.source)
    return
  }
  if (!mesh.geometry.hasAttribute('position')) return
  if (!cached) {
    cached = { source: mesh.geometry, cuts: new Map() }
    variants.set(mesh.geometry, cached)
    const owned = cached
    mesh.geometry.addEventListener('dispose', () => {
      for (const geometry of owned.cuts.values()) {
        if (geometry === owned.source) continue
        variants.delete(geometry)
        geometry.dispose()
      }
      owned.cuts.clear()
      variants.delete(owned.source)
    })
  }
  const key = `${applied.join('+')}:${skin}`
  let geometry = cached.cuts.get(key)
  if (!geometry) {
    geometry = applyCuts(cached.source, applied, skin, mesh.skeleton)
    cached.cuts.set(key, geometry)
    variants.set(geometry, cached)
  }
  setGeometry(mesh, geometry)
}

function drapePriestLappetsOverHair(source: THREE.BufferGeometry) {
  const geometry = source.clone()
  const positions = geometry.getAttribute('position')
  for (let i = 0; i < positions.count; i++) {
    const y = positions.getY(i)
    const z = positions.getZ(i)
    if (z < 0)
      positions.setZ(
        i,
        z - 0.045 * THREE.MathUtils.smoothstep(1.815 - y, 0, 0.065)
      )
  }
  return finishReshape(source, geometry)
}

function tuckHairUnderHat(geometry: THREE.BufferGeometry) {
  const positions = geometry.getAttribute('position')
  const originalNormals = geometry.getAttribute('normal')?.clone()
  const originalPositions = positions.clone()
  for (let i = 0; i < positions.count; i++) {
    const y = positions.getY(i)
    const x = positions.getX(i)
    let z = positions.getZ(i)
    const rearLimit = -0.1 - 0.3 * Math.max(0, 1.82 - y)
    z = THREE.MathUtils.lerp(
      z,
      Math.max(z, rearLimit),
      THREE.MathUtils.smoothstep(y, 1.58, 1.65)
    )
    const amount = THREE.MathUtils.smoothstep(y, z > 0.03 ? 1.79 : 1.74, 1.815)
    const radius = Math.hypot(x / 0.084, z / (z > 0 ? 0.105 : 0.102))
    const scale = radius > 1 ? THREE.MathUtils.lerp(1, 1 / radius, amount) : 1
    positions.setXYZ(i, x * scale, y, z * scale)
  }
  geometry.deleteAttribute('tangent')
  geometry.computeVertexNormals()
  if (originalNormals) {
    const normals = geometry.getAttribute('normal')
    for (let i = 0; i < positions.count; i++)
      if (
        positions.getX(i) === originalPositions.getX(i) &&
        positions.getZ(i) === originalPositions.getZ(i)
      )
        normals.setXYZ(
          i,
          originalNormals.getX(i),
          originalNormals.getY(i),
          originalNormals.getZ(i)
        )
  }
}

function untuckLinen(source: THREE.BufferGeometry) {
  const geometry = source.clone()
  const positions = geometry.getAttribute('position')
  for (let i = 0; i < positions.count; i++) {
    const y = positions.getY(i)
    const amount = 1 - THREE.MathUtils.smoothstep(y, 1.135, 1.2)
    if (!amount) continue
    const x = positions.getX(i)
    const z = positions.getZ(i) + 0.015
    const radius = Math.hypot(x / 0.185, z / 0.14)
    if (radius < 1e-6 || radius >= 1) continue
    const scale = THREE.MathUtils.lerp(1, 1 / radius, amount)
    positions.setXYZ(i, x * scale, y, z * scale - 0.015)
  }
  return finishReshape(source, geometry, (i) => positions.getY(i) >= 1.2)
}

function tuckWaist(
  centerZ: number,
  hipWidth: number,
  depth: [number, number],
  tuckBelowWaist: boolean
) {
  return (source: THREE.BufferGeometry) => {
    const geometry = source.clone()
    const positions = geometry.getAttribute('position') as THREE.BufferAttribute
    for (let i = 0; i < positions.count; i++) {
      const y = positions.getY(i)
      const amount = THREE.MathUtils.smoothstep(y, 0.92, 1.04)
      if (!amount && !tuckBelowWaist) continue
      const x = positions.getX(i)
      const z = positions.getZ(i) - centerZ
      const radius = Math.hypot(
        x / THREE.MathUtils.lerp(hipWidth, 0.16, amount),
        z / THREE.MathUtils.lerp(depth[0], depth[1], amount)
      )
      if (radius > 1) positions.setXYZ(i, x / radius, y, z / radius + centerZ)
    }
    return finishReshape(source, geometry)
  }
}

function removeRoguePockets(source: THREE.BufferGeometry) {
  const { islands, islandAt } = meshIslands(source, 1e5)
  const pockets = new Set(
    [...islands]
      .filter(
        ([, { bounds }]) =>
          bounds.min.y > 0.9 && bounds.min.y < 1 && bounds.max.y > 1.05
      )
      .map(([root]) => root)
  )
  return clipSkinnedGeometry(source, (p) => (pockets.has(islandAt(p)) ? -1 : 1))
}

function applyCuts(
  source: THREE.BufferGeometry,
  applied: Cut[],
  skin: boolean,
  skeleton?: THREE.Skeleton
): THREE.BufferGeometry {
  let geometry = source
  const replace = (next: THREE.BufferGeometry) => {
    if (geometry !== source) geometry.dispose()
    geometry = next
  }
  const clip = (distance: Distance, refine = false) =>
    replace(
      clipSkinnedGeometry(
        geometry,
        (point) => distance(point) * (skin ? -1 : 1),
        refine
      )
    )
  for (const name of applied) {
    const cut = cuts[name]
    if ('reshape' in cut) replace(cut.reshape(geometry))
    else if ('cuff' in cut) {
      clip(cut.cuff.distance, true)
      cut.cuff.tuck(geometry)
    } else if ('then' in cut) {
      clip(cut.clip)
      cut.then(geometry)
    } else for (const distance of [cut].flat()) clip(distance)
    if (name === 'bard_plate_waist') replace(tuckBardWaist(geometry, skeleton))
    if (name === 'ranger_sleeves' && !skin) {
      const transition = sleeveCut(0.2)
      const halves = [
        clipSkinnedGeometry(geometry, transition),
        clipSkinnedGeometry(geometry, (point) => -transition(point)),
      ]
      const merged = mergeGeometries(halves)!
      let offset = 0
      for (const half of halves) {
        for (const group of half.groups)
          merged.addGroup(
            offset + group.start,
            group.count,
            group.materialIndex
          )
        offset += half.index!.count
        half.dispose()
      }
      geometry.dispose()
      geometry = merged
      tuckSleevesIntoRangerGloves(geometry)
    }
  }
  return geometry
}
