<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { onDestroy, untrack } from 'svelte'
  import {
    createParticleInstancedMesh,
    PARTICLE_OPACITY_ATTR,
  } from '../../shaders/wind-particle-material'
  import { createRainParticleMaterial } from '../../shaders/rain-particle-material'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { WindSample } from '../../shaders/grass-material'

  interface Props {
    playerPosition?: THREE.Vector3 | null
    heightManager?: TerrainHeightManager | null
    maxFlakes?: number
  }

  const FULL_FLAKE_LIMIT = 2400
  let {
    playerPosition = null,
    heightManager = null,
    maxFlakes = FULL_FLAKE_LIMIT,
  }: Props = $props()
  const flakeLimit = untrack(() => maxFlakes)

  const SPAWN_RADIUS = 36
  const SPAWN_HEIGHT_MIN = 3
  const SPAWN_HEIGHT_MAX = 12
  const FALL_SPEED_MIN = 0.9
  const FALL_SPEED_MAX = 1.7
  // Flakes live about 6 s, so this keeps a full pool at intensity 1.
  const SPAWN_RATE_AT_FULL = flakeLimit / 6.5
  // Light flakes ride the wind far more than rain does.
  const WIND_SPEED_AT_FULL = 3.2
  const SWAY_SPEED = 0.8
  const MAX_AGE = 16

  interface Flake {
    alive: boolean
    x: number
    y: number
    z: number
    vy: number
    drift: number
    groundY: number
    age: number
    phase: number
    swayFreq: number
    swayAmp: number
    baseOpacity: number
    size: number
  }

  const flakes: Flake[] = Array.from({ length: flakeLimit }, () => ({
    alive: false,
    x: 0,
    y: 0,
    z: 0,
    vy: 0,
    drift: 1,
    groundY: 0,
    age: 0,
    phase: 0,
    swayFreq: 1,
    swayAmp: 0,
    baseOpacity: 0,
    size: 1,
  }))
  let cursor = 0
  let flakesAlive = 0
  let spawnAccumulator = 0
  let gustTime = 0

  // Soft round flake with a brighter core.
  function createFlakeTexture(): THREE.CanvasTexture {
    const s = 32
    const canvas = document.createElement('canvas')
    canvas.width = s
    canvas.height = s
    const ctx = canvas.getContext('2d')!
    const img = ctx.createImageData(s, s)
    const c = (s - 1) / 2
    for (let y = 0; y < s; y++) {
      for (let x = 0; x < s; x++) {
        const r = Math.hypot(x - c, y - c) / c
        const core = Math.max(0, 1 - r * 1.6)
        const halo = Math.max(0, 1 - r) ** 2
        const i = (y * s + x) * 4
        img.data[i] = 235
        img.data[i + 1] = 240
        img.data[i + 2] = 250
        img.data[i + 3] = Math.round(Math.min(1, core + halo * 0.6) * 255)
      }
    }
    ctx.putImageData(img, 0, 0)
    const tex = new THREE.CanvasTexture(canvas)
    tex.colorSpace = THREE.SRGBColorSpace
    return tex
  }

  const snowGroup = new THREE.Group()
  let mesh: THREE.InstancedMesh | null = null
  let flakeTexture: THREE.Texture | null = null

  onDestroy(() => {
    if (mesh) {
      mesh.removeFromParent()
      mesh.dispose()
      mesh.geometry.dispose()
      const materials = Array.isArray(mesh.material)
        ? mesh.material
        : [mesh.material]
      for (const material of materials) material.dispose()
    }
    flakeTexture?.dispose()
  })

  function init() {
    if (mesh) return
    flakeTexture = createFlakeTexture()
    mesh = createParticleInstancedMesh(
      createRainParticleMaterial(flakeTexture),
      0.09,
      0.09,
      flakeLimit
    )
    mesh.renderOrder = 2
  }

  export function getGroup(): THREE.Group {
    return snowGroup
  }

  const tmpMatrix = new THREE.Matrix4()
  const camRight = new THREE.Vector3()
  const camUp = new THREE.Vector3()
  const camBack = new THREE.Vector3()
  const flakeRight = new THREE.Vector3()
  const flakeUp = new THREE.Vector3()
  const zeroMatrix = new THREE.Matrix4().makeScale(0, 0, 0)

  function spawnFlake(
    px: number,
    py: number,
    pz: number,
    windVX: number,
    windVZ: number
  ) {
    const f = flakes[cursor]
    cursor = (cursor + 1) % flakeLimit
    if (f.alive) return

    const angle = Math.random() * Math.PI * 2
    const dist = Math.sqrt(Math.random()) * SPAWN_RADIUS
    const x = px + Math.cos(angle) * dist
    const z = pz + Math.sin(angle) * dist
    const fallHeight =
      SPAWN_HEIGHT_MIN + Math.random() * (SPAWN_HEIGHT_MAX - SPAWN_HEIGHT_MIN)
    f.vy = -(FALL_SPEED_MIN + Math.random() * (FALL_SPEED_MAX - FALL_SPEED_MIN))
    f.drift = 0.7 + Math.random() * 0.6
    // Start upwind so the flake settles near the chosen spot.
    const fallTime = fallHeight / -f.vy
    f.x = x - windVX * f.drift * fallTime
    f.z = z - windVZ * f.drift * fallTime
    f.groundY = heightManager?.groundYOrNull(x, z) ?? py
    f.y = f.groundY + fallHeight
    f.age = 0
    f.phase = Math.random() * Math.PI * 2
    f.swayFreq = 0.6 + Math.random() * 0.9
    f.swayAmp = 0.25 + Math.random() * 0.45
    f.baseOpacity = 0.55 + Math.random() * 0.4
    f.size = 0.6 + Math.random() * 0.8
    f.alive = true
  }

  /** `intensity` 0..1 gates spawning. */
  export function update(
    deltaTime: number,
    camera: THREE.Camera | undefined,
    intensity: number,
    wind: WindSample | null
  ) {
    if (!camera) return
    if (intensity <= 0 && flakesAlive === 0) {
      spawnAccumulator = 0
      return
    }
    init()

    const dt = Math.min(deltaTime / 1000, 0.1)
    gustTime += dt
    const gust = 1 + 0.35 * Math.sin(gustTime * 0.7) * Math.sin(gustTime * 0.23)
    const windSpeed =
      (wind?.windStrength ?? 0) * WIND_SPEED_AT_FULL * (0.6 + 0.4 * intensity)
    const windVX = (wind?.windDirX ?? 0) * windSpeed * gust
    const windVZ = (wind?.windDirZ ?? 0) * windSpeed * gust
    camera.matrixWorld.extractBasis(camRight, camUp, camBack)

    if (intensity > 0 && playerPosition) {
      spawnAccumulator += dt
      const spawnInterval = 1.0 / (SPAWN_RATE_AT_FULL * intensity)
      while (spawnAccumulator >= spawnInterval) {
        spawnAccumulator -= spawnInterval
        spawnFlake(
          playerPosition.x,
          playerPosition.y,
          playerPosition.z,
          windVX,
          windVZ
        )
      }
    } else {
      spawnAccumulator = 0
    }

    const opacityAttr = mesh!.geometry.getAttribute(
      PARTICLE_OPACITY_ATTR
    ) as THREE.InstancedBufferAttribute
    const opacity = opacityAttr.array as Float32Array
    let alive = 0
    for (let i = 0; i < flakes.length; i++) {
      const f = flakes[i]
      if (!f.alive) continue

      f.age += dt
      const sway = Math.sin(f.age * f.swayFreq * Math.PI * 2 + f.phase)
      const swayCross = Math.cos(f.age * f.swayFreq * 1.3 + f.phase)
      f.y += f.vy * dt
      f.x += (windVX * f.drift + sway * f.swayAmp * SWAY_SPEED) * dt
      f.z += (windVZ * f.drift + swayCross * f.swayAmp * SWAY_SPEED) * dt

      if (f.y <= f.groundY || f.age > MAX_AGE) {
        f.alive = false
        mesh!.setMatrixAt(i, zeroMatrix)
        opacity[i] = 0
        continue
      }
      alive++

      const fadeIn = Math.min(1, f.age / 0.6)
      const fadeOut = Math.min(1, (f.y - f.groundY) / 0.25)
      opacity[i] = f.baseOpacity * fadeIn * fadeOut
      flakeRight.copy(camRight).multiplyScalar(f.size)
      flakeUp.copy(camUp).multiplyScalar(f.size)
      tmpMatrix
        .makeBasis(flakeRight, flakeUp, camBack)
        .setPosition(f.x, f.y, f.z)
      mesh!.setMatrixAt(i, tmpMatrix)
    }

    flakesAlive = alive
    if (alive > 0) {
      mesh!.instanceMatrix.needsUpdate = true
      opacityAttr.needsUpdate = true
      if (!mesh!.parent) snowGroup.add(mesh!)
    } else if (mesh!.parent) {
      snowGroup.remove(mesh!)
    }
  }
</script>

<T is={snowGroup} />
