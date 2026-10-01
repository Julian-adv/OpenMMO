import * as THREE from '../../client/node_modules/three/build/three.module.js'

const cross = (a, b) => a[0] * b[1] - a[1] * b[0]

function prepareSections(meshes, reference, inverse) {
  const center = new THREE.Vector3().fromArray(reference.center)
  const axis = new THREE.Vector3().fromArray(reference.axis)
  const basis = reference.basis.map((v) => new THREE.Vector3().fromArray(v))
  const side = reference.side === 'Left' ? 1 : -1
  return meshes.map(({ mesh, positions }) => {
    const source = mesh.geometry.attributes.position
    const indices = mesh.geometry.index
    const local = positions.map((point) => point.clone().applyMatrix4(inverse).sub(center))
    const heights = local.map((point) => point.dot(axis))
    const restHeights = Array.from({ length: source.count }, (_, i) =>
      new THREE.Vector3().fromBufferAttribute(source, i).sub(center).dot(axis))
    const faces = []
    for (let i = 0; i < indices.count; i += 3) {
      const face = [indices.getX(i), indices.getX(i + 1), indices.getX(i + 2)]
      if (face.reduce((sum, index) => sum + source.getX(index), 0) * side <= 0) continue
      // Exclude the knee when a folded leg crosses the ankle plane.
      const restHeight = face.reduce((sum, index) => sum + restHeights[index], 0) / 3
      if (restHeight < -.12 || restHeight > .15) continue
      faces.push(face)
    }
    return { local, heights, faces, basis }
  })
}

function section(prepared, height) {
  const segments = []
  for (const { local, heights, faces, basis } of prepared) {
    const distance = heights.map((value) => value - height)
    for (const face of faces) {
      const hits = []
      for (let j = 0; j < 3; j++) {
        const a = face[j], b = face[(j + 1) % 3]
        if ((distance[a] < 0) === (distance[b] < 0)) continue
        const t = distance[a] / (distance[a] - distance[b])
        const point = local[a].clone().lerp(local[b], t)
        hits.push(basis.map((axis) => point.dot(axis)))
      }
      if (hits.length === 2) segments.push(hits)
    }
  }
  return segments
}

function radii(segments, angle) {
  const direction = [Math.cos(angle), Math.sin(angle)]
  const hits = []
  for (const [a, b] of segments) {
    const edge = [b[0] - a[0], b[1] - a[1]]
    const denominator = cross(direction, edge)
    if (Math.abs(denominator) < 1e-12) continue
    const radius = cross(a, edge) / denominator
    const t = cross(a, direction) / denominator
    if (t >= -1e-6 && t <= 1 + 1e-6 && radius > 0 && radius < .16) hits.push(radius)
  }
  return hits
}

export function measureAnkleConnections(posed, references, skeleton) {
  return references.map((reference) => {
    const common = new THREE.Matrix4()
    common.elements.fill(0)
    for (const [name, weight] of Object.entries(reference.weights)) {
      const index = skeleton.bones.findIndex((bone) => bone.name === name)
      if (index < 0) throw Error(`Missing ankle bone ${name}`)
      const matrix = new THREE.Matrix4().multiplyMatrices(skeleton.bones[index].matrixWorld, skeleton.boneInverses[index])
      matrix.elements.forEach((value, i) => common.elements[i] += value * weight)
    }
    const inverse = common.clone().invert()
    const pants = prepareSections(posed.filter(({ mesh }) => mesh.userData.part_id === 'pants_rogue'), reference, inverse)
    const boots = prepareSections(posed.filter(({ mesh }) => mesh.userData.part_id === 'boots_rogue'), reference, inverse)
    const planes = reference.sample_heights_m.map((height) => {
      const inside = section(pants, height)
      const outside = section(boots, height)
      let missing = 0, minimum = Infinity, maximum = -Infinity
      for (let i = 0; i < 144; i++) {
        const angle = i * 2 * Math.PI / 144
        const trouser = radii(inside, angle), boot = radii(outside, angle)
        if (!trouser.length || !boot.length) { missing++; continue }
        const clearance = Math.min(...boot) - Math.max(...trouser)
        minimum = Math.min(minimum, clearance)
        maximum = Math.max(maximum, clearance)
      }
      return { height_m: height, missing_rays: missing, minimum_clearance_m: minimum, maximum_clearance_m: maximum }
    })
    return { side: reference.side, angles_per_plane: 144, planes }
  })
}
