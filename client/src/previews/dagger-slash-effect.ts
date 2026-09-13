import * as THREE from 'three'

export interface BladeSample {
  time: number
  base: THREE.Vector3
  tip: THREE.Vector3
}

export class DaggerSlashEffect {
  readonly group = new THREE.Group()
  private readonly ribbons: {
    mesh: THREE.Mesh<THREE.BufferGeometry, THREE.ShaderMaterial>
    times: number[]
    hit: number
  }[] = []
  private readonly sparks: {
    mesh: THREE.Mesh<THREE.SphereGeometry, THREE.MeshBasicMaterial>
    origin: THREE.Vector3
    velocity: THREE.Vector3
    hit: number
  }[] = []

  constructor(
    samples: BladeSample[],
    hits: number[],
    private readonly broad: boolean
  ) {
    for (const hit of hits) {
      const sweep = samples.filter(
        (s) => s.time >= hit - 0.075 && s.time <= hit + 0.07
      )
      const layers = broad
        ? [
            [0.04, 1.35, 0.24],
            [0.12, 0.93, 0.62],
            [0.27, 0.43, 1],
          ]
        : [
            [0.19, 0.42, 0.68],
            [0.31, 0.4, 1],
          ]
      for (const [inner, outer, opacity] of layers) {
        const positions: number[] = []
        const times: number[] = []
        const uvs: number[] = []
        const add = (sample: BladeSample, distance: number) => {
          const direction = sample.tip.clone().sub(sample.base).normalize()
          positions.push(
            ...sample.base
              .clone()
              .addScaledVector(direction, distance - 0.075)
              .toArray()
          )
          times.push(sample.time)
          uvs.push(0, distance === inner ? 0 : 1)
        }
        for (let i = 1; i < sweep.length; i++) {
          add(sweep[i - 1], inner)
          add(sweep[i - 1], outer)
          add(sweep[i], outer)
          add(sweep[i - 1], inner)
          add(sweep[i], outer)
          add(sweep[i], inner)
        }
        const geometry = new THREE.BufferGeometry()
        geometry.setAttribute(
          'position',
          new THREE.Float32BufferAttribute(positions, 3)
        )
        geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2))
        geometry.setAttribute(
          'color',
          new THREE.Float32BufferAttribute(
            new Float32Array(positions.length),
            3
          )
        )
        const material = new THREE.ShaderMaterial({
          uniforms: {
            tint: { value: new THREE.Color(broad ? '#8bcaff' : '#b6ffee') },
            strength: { value: opacity },
          },
          vertexShader: `varying vec2 vUv;
            varying float vBrightness;
            void main() {
              vUv = uv;
              vBrightness = color.r;
              gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
            }`,
          fragmentShader: `uniform vec3 tint;
            uniform float strength;
            varying vec2 vUv;
            varying float vBrightness;
            void main() {
              float feather = pow(max(0.0, sin(vUv.y * 3.14159265)), 0.8);
              gl_FragColor = vec4(tint, feather * vBrightness * strength);
            }`,
          transparent: true,
          vertexColors: true,
          side: THREE.DoubleSide,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          toneMapped: false,
        })
        const mesh = new THREE.Mesh(geometry, material)
        mesh.frustumCulled = false
        this.group.add(mesh)
        this.ribbons.push({ mesh, times, hit })
      }
      if (broad) {
        const sample = samples.reduce((best, s) =>
          Math.abs(s.time - hit) < Math.abs(best.time - hit) ? s : best
        )
        for (let i = 0; i < 12; i++) {
          const angle = i * 2.39996
          const mesh = new THREE.Mesh(
            new THREE.SphereGeometry(0.009, 4, 3),
            new THREE.MeshBasicMaterial({
              color: '#d4ecff',
              transparent: true,
              depthWrite: false,
              blending: THREE.AdditiveBlending,
              toneMapped: false,
            })
          )
          const velocity = new THREE.Vector3(
            Math.cos(angle),
            0.2 + (i % 4) * 0.15,
            Math.sin(angle)
          ).multiplyScalar(0.7 + (i % 3) * 0.3)
          this.sparks.push({ mesh, origin: sample.tip.clone(), velocity, hit })
          this.group.add(mesh)
        }
      }
    }
  }

  update(time: number) {
    const lifetime = this.broad ? 0.22 : 0.085
    for (const { mesh, times, hit } of this.ribbons) {
      mesh.visible = time >= hit - 0.075 && time <= hit + 0.07 + lifetime
      if (!mesh.visible) continue
      const colors = mesh.geometry.getAttribute(
        'color'
      ) as THREE.BufferAttribute
      for (let i = 0; i < times.length; i++) {
        const age = time - times[i]
        const brightness =
          age >= 0 && age <= lifetime ? Math.pow(1 - age / lifetime, 1.4) : 0
        colors.setXYZ(i, brightness, brightness, brightness)
      }
      colors.needsUpdate = true
    }
    for (const { mesh, origin, velocity, hit } of this.sparks) {
      const age = time - hit
      mesh.visible = age >= 0 && age <= 0.27
      if (!mesh.visible) continue
      mesh.position.copy(origin).addScaledVector(velocity, age)
      mesh.position.y -= age * age * 1.8
      mesh.material.opacity = 1 - age / 0.27
      mesh.scale.setScalar(1 - age * 2)
    }
  }
}
