import * as THREE from 'three'
import { DAGGER_SKILL } from '../data/daggerSkill'

const MAX_SAMPLES = 32
const LIFETIME = 0.085

export class DaggerBladeTrail {
  readonly group = new THREE.Group()
  private geometry = new THREE.BufferGeometry()
  private material = new THREE.MeshBasicMaterial({
    vertexColors: true,
    transparent: true,
    depthWrite: false,
    side: THREE.DoubleSide,
    blending: THREE.AdditiveBlending,
    toneMapped: false,
  })
  private positions = new THREE.BufferAttribute(
    new Float32Array(MAX_SAMPLES * 12 * 3),
    3
  )
  private colors = new THREE.BufferAttribute(
    new Float32Array(MAX_SAMPLES * 12 * 3),
    3
  )
  private samples: {
    time: number
    base: THREE.Vector3
    tip: THREE.Vector3
    strike: number
  }[] = []
  private previous = -1
  private tint = new THREE.Color('#b6ffee')

  constructor() {
    this.positions.setUsage(THREE.DynamicDrawUsage)
    this.colors.setUsage(THREE.DynamicDrawUsage)
    this.geometry.setAttribute('position', this.positions)
    this.geometry.setAttribute('color', this.colors)
    const mesh = new THREE.Mesh(this.geometry, this.material)
    mesh.frustumCulled = false
    this.group.add(mesh)
    this.clear()
  }

  clear() {
    this.samples.length = 0
    this.previous = -1
    this.group.visible = false
  }

  update(weapon: THREE.Object3D, root: THREE.Object3D, time: number) {
    if (time < this.previous) this.clear()
    this.previous = time
    const strike = DAGGER_SKILL.hits.findIndex(
      (hit) => time >= hit - 0.075 && time <= hit + 0.07
    )
    this.samples = this.samples.filter(
      (sample) => time - sample.time <= LIFETIME
    )
    if (strike >= 0) {
      this.samples.push({
        time,
        strike,
        base: root.worldToLocal(
          weapon.localToWorld(new THREE.Vector3(0.19, 0, 0))
        ),
        tip: root.worldToLocal(
          weapon.localToWorld(new THREE.Vector3(0.42, 0, 0))
        ),
      })
    }
    if (this.samples.length > MAX_SAMPLES) this.samples.shift()
    let vertex = 0
    const point = new THREE.Vector3()
    const write = (sample: (typeof this.samples)[number], across: number) => {
      point.lerpVectors(sample.base, sample.tip, across)
      this.positions.setXYZ(vertex, point.x, point.y, point.z)
      const brightness =
        Math.pow(Math.max(0, 1 - (time - sample.time) / LIFETIME), 1.4) *
        (across === 0.5 ? 1 : 0)
      this.colors.setXYZ(
        vertex++,
        this.tint.r * brightness,
        this.tint.g * brightness,
        this.tint.b * brightness
      )
    }
    for (let i = 1; i < this.samples.length; i++) {
      const a = this.samples[i - 1],
        b = this.samples[i]
      if (a.strike !== b.strike) continue
      for (const low of [0, 0.5]) {
        const high = low + 0.5
        write(a, low)
        write(a, high)
        write(b, high)
        write(a, low)
        write(b, high)
        write(b, low)
      }
    }
    this.geometry.setDrawRange(0, vertex)
    this.positions.needsUpdate = this.colors.needsUpdate = true
    this.group.visible = vertex > 0
  }

  dispose() {
    this.geometry.dispose()
    this.material.dispose()
    this.group.removeFromParent()
  }
}
