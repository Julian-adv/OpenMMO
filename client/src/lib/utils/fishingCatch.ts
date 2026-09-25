import * as THREE from 'three'
import {
  FISHING_CATCH_DURATION,
  type FishingCatch,
} from '../stores/fishingStore'
import { unwrapWorldXNear } from '../terrain/world-wrap'

const UP = new THREE.Vector3(0, 1, 0)

export const catchLift = (time: number) =>
  THREE.MathUtils.smoothstep(time, 0, 0.65) * catchWeight(time)
export const catchReach = (time: number) =>
  THREE.MathUtils.smoothstep(time, 0.65, 1.25) * catchWeight(time)
const catchWeight = (time: number) =>
  1 - THREE.MathUtils.smoothstep(time, 3.05, FISHING_CATCH_DURATION)

function fishModel(itemId: string) {
  const group = new THREE.Group()
  const golden = itemId.includes('sturgeon')
  const perch = itemId.includes('perch')
  const trout = itemId.includes('trout')
  const dark = new THREE.Color(
    golden ? '#70622b' : perch ? '#566f37' : '#426369'
  )
  const silver = new THREE.Color(golden ? '#d4b35b' : '#aebfbb')
  const belly = new THREE.Color(golden ? '#f1dda3' : '#e8e3cf')
  const profile = new THREE.SplineCurve(
    [
      [0, 0.015],
      [-0.055, 0.048],
      [-0.14, 0.09],
      [-0.27, 0.12],
      [-0.43, 0.115],
      [-0.6, 0.083],
      [-0.76, 0.043],
      [-0.86, 0.018],
    ].map(([y, radius]) => new THREE.Vector2(y, radius))
  ).getPoints(48)
  const vertices: number[] = []
  const colors: number[] = []
  const indices: number[] = []
  const sides = 32
  const color = new THREE.Color()
  for (let i = 0; i < profile.length; i++) {
    const { x: y, y: radius } = profile[i]
    for (let j = 0; j < sides; j++) {
      const angle = (j * Math.PI * 2) / sides
      const dorsal = Math.cos(angle)
      vertices.push(dorsal * radius, y, Math.sin(angle) * radius * 0.52)
      color
        .copy(silver)
        .lerp(dorsal > 0 ? dark : belly, Math.abs(dorsal) ** 1.5)
      if (perch && y < -0.15 && Math.sin(y * 42) > 0.55 && dorsal > -0.5)
        color.multiplyScalar(0.53)
      if (trout && y < -0.15 && (i * 7 + j * 13) % 31 < 2)
        color.multiplyScalar(0.45)
      colors.push(color.r, color.g, color.b)
      if (i > 0) {
        const a = (i - 1) * sides + j
        const b = (i - 1) * sides + ((j + 1) % sides)
        indices.push(a, b, a + sides, b, b + sides, a + sides)
      }
    }
  }
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(vertices, 3)
  )
  geometry.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3))
  geometry.setIndex(indices)
  geometry.computeVertexNormals()
  group.add(
    new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({
        vertexColors: true,
        metalness: 0.28,
        roughness: 0.4,
      })
    )
  )

  const finMaterial = new THREE.MeshStandardMaterial({
    color: golden ? '#b28a3b' : perch ? '#ba733d' : '#738d85',
    side: THREE.DoubleSide,
    roughness: 0.68,
  })
  const fin = (points: number[], triangles: number[]) => {
    const geo = new THREE.BufferGeometry()
    geo.setAttribute('position', new THREE.Float32BufferAttribute(points, 3))
    geo.setIndex(triangles)
    geo.computeVertexNormals()
    const mesh = new THREE.Mesh(geo, finMaterial)
    group.add(mesh)
    return mesh
  }
  const tail = fin(
    [0, -0.84, 0, 0.15, -1, 0, 0, -0.95, 0, -0.15, -1, 0],
    [0, 1, 2, 0, 2, 3]
  )
  fin(
    [0.1, -0.23, 0, 0.19, -0.35, 0, 0.15, -0.47, 0, 0.1, -0.53, 0],
    [0, 1, 2, 0, 2, 3]
  )
  for (const side of [-1, 1]) {
    fin(
      [
        0,
        -0.19,
        side * 0.045,
        -0.035,
        -0.42,
        side * 0.15,
        -0.045,
        -0.32,
        side * 0.04,
      ],
      [0, 1, 2]
    )
    const eye = new THREE.Mesh(
      new THREE.SphereGeometry(0.019, 12, 8),
      new THREE.MeshStandardMaterial({ color: '#d7c78f', roughness: 0.35 })
    )
    eye.position.set(0.017, -0.09, side * 0.045)
    eye.scale.z = 0.45
    const pupil = new THREE.Mesh(
      new THREE.SphereGeometry(0.011, 10, 8),
      new THREE.MeshStandardMaterial({ color: '#101b1c', roughness: 0.16 })
    )
    pupil.position.set(0.017, -0.09, side * 0.053)
    pupil.scale.z = 0.35
    group.add(eye, pupil)
    const gill = new THREE.CatmullRomCurve3([
      new THREE.Vector3(0.055, -0.13, side * 0.036),
      new THREE.Vector3(0.025, -0.18, side * 0.052),
      new THREE.Vector3(-0.03, -0.19, side * 0.048),
      new THREE.Vector3(-0.072, -0.16, side * 0.024),
    ])
    group.add(
      new THREE.Mesh(
        new THREE.TubeGeometry(gill, 10, 0.0025, 4, false),
        new THREE.MeshStandardMaterial({ color: '#4a615a', roughness: 0.6 })
      )
    )
  }
  const mouth = new THREE.Mesh(
    new THREE.TorusGeometry(0.015, 0.003, 6, 12),
    finMaterial
  )
  mouth.rotation.x = Math.PI / 2
  group.add(mouth)
  group.traverse((object) => {
    object.raycast = () => {}
    object.castShadow = true
  })
  return { group, tail }
}

export class FishingCatchVisual {
  readonly group = new THREE.Group()
  readonly handTarget = new THREE.Vector3()
  private readonly fish: THREE.Group
  private readonly tail: THREE.Mesh
  private readonly lineGeometry = new THREE.BufferGeometry()
  private readonly linePositions = new THREE.Float32BufferAttribute(
    new Float32Array(9),
    3
  )
  private readonly lineMaterial = new THREE.LineBasicMaterial({
    color: '#f4f8fb',
    transparent: true,
    opacity: 0.8,
  })
  private readonly mouth = new THREE.Vector3()
  private readonly waypoint = new THREE.Vector3()
  private readonly point = new THREE.Vector3()
  private readonly rotation = new THREE.Quaternion()
  private readonly sway = new THREE.Quaternion()
  private readonly materials = new Set<THREE.Material>()

  constructor(
    private readonly root: THREE.Object3D,
    readonly event: FishingCatch
  ) {
    const model = fishModel(event.fish.item_def_id)
    this.fish = model.group
    this.tail = model.tail
    this.fish.name = 'landed_fish'
    this.fish.scale.setScalar(
      THREE.MathUtils.clamp(event.fish.size_cm / 100, 0.18, 1.2)
    )
    this.lineGeometry.setAttribute('position', this.linePositions)
    const line = new THREE.Line(this.lineGeometry, this.lineMaterial)
    line.frustumCulled = false
    line.raycast = () => {}
    this.group.name = 'fishing_catch'
    this.group.matrixAutoUpdate = false
    this.group.add(this.fish, line)
    this.fish.traverse((object) => {
      if (object instanceof THREE.Mesh) {
        const material = object.material as THREE.Material
        material.transparent = true
        this.materials.add(material)
      }
    })
    root.add(this.group)
  }

  target(shoulder: THREE.Object3D, time: number) {
    this.root.getWorldQuaternion(this.rotation)
    this.handTarget
      .set(
        -0.12,
        -0.12 + 0.35 * THREE.MathUtils.smoothstep(time, 1.3, 1.95),
        0.4
      )
      .applyQuaternion(this.rotation)
    shoulder.getWorldPosition(this.point)
    this.handTarget.add(this.point)
    return this.handTarget
  }

  update(time: number, tip: THREE.Vector3, palm: THREE.Vector3) {
    const reach = catchReach(time)
    this.group.matrix.copy(this.root.matrixWorld).invert()
    this.group.matrixWorldNeedsUpdate = true
    this.mouth.copy(this.handTarget).lerp(palm, reach)
    this.mouth.y -= 0.3
    const water = this.event.waterPosition
    const landed = THREE.MathUtils.smoothstep(time, 0, 1.05)
    this.point.set(unwrapWorldXNear(tip.x, water.x), water.y - 0.12, water.z)
    this.mouth.lerpVectors(this.point, this.mouth, landed)
    this.fish.position.copy(this.mouth)
    this.root.getWorldQuaternion(this.rotation)
    this.fish.quaternion
      .copy(this.rotation)
      .multiply(this.sway.setFromAxisAngle(UP, Math.sin(time * 4.5) * 0.45))
    this.fish.rotateZ(Math.sin(time * 11) * 0.12 * Math.exp(-time * 0.4))
    this.tail.rotation.y = Math.sin(time * 22) * 0.35 * Math.exp(-time * 0.45)
    const opacity = 1 - THREE.MathUtils.smoothstep(time, 2.95, 3.35)
    for (const material of this.materials) material.opacity = opacity
    this.lineMaterial.opacity = opacity * 0.8
    this.fish.visible = this.mouth.y > water.y - 0.03
    this.waypoint.lerpVectors(tip, this.mouth, 0.82).lerp(palm, reach)
    this.linePositions.setXYZ(0, tip.x, tip.y, tip.z)
    this.linePositions.setXYZ(
      1,
      this.waypoint.x,
      this.waypoint.y,
      this.waypoint.z
    )
    this.linePositions.setXYZ(2, this.mouth.x, this.mouth.y, this.mouth.z)
    this.linePositions.needsUpdate = true
    this.group.updateWorldMatrix(false, true)
  }

  dispose() {
    this.group.removeFromParent()
    this.fish.traverse((object) => {
      if (object instanceof THREE.Mesh) object.geometry.dispose()
    })
    for (const material of this.materials) material.dispose()
    this.lineGeometry.dispose()
    this.lineMaterial.dispose()
  }
}
