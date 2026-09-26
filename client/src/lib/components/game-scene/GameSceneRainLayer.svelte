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
    maxDrops?: number
    enableSplashes?: boolean
  }

  const FULL_DROP_LIMIT = 1100
  let {
    playerPosition = null,
    heightManager = null,
    maxDrops = FULL_DROP_LIMIT,
    enableSplashes = true,
  }: Props = $props()
  const dropLimit = untrack(() => maxDrops)
  const splashLimit = untrack(() => (enableSplashes ? 350 : 0))

  // Cover the visible quarter-view area.
  const SPAWN_RADIUS = 36
  const SPAWN_RATE_AT_FULL = (760 * dropLimit) / FULL_DROP_LIMIT
  const SPAWN_HEIGHT_MIN = 4
  const SPAWN_HEIGHT_MAX = 13
  const FALL_SPEED_MIN = 9
  const FALL_SPEED_MAX = 13
  const SPLASH_LIFE = 0.28
  // Horizontal speed at full wind strength; ~20° tilt against FALL_SPEED.
  const WIND_SPEED_AT_FULL = 4

  interface Drop {
    alive: boolean
    x: number
    y: number
    z: number
    vy: number
    groundY: number
    age: number
    baseOpacity: number
    scale: number
  }

  interface Splash {
    alive: boolean
    x: number
    y: number
    z: number
    age: number
  }

  const drops: Drop[] = Array.from({ length: dropLimit }, () => ({
    alive: false,
    x: 0,
    y: 0,
    z: 0,
    vy: 0,
    groundY: 0,
    age: 0,
    baseOpacity: 0,
    scale: 1,
  }))
  const splashes: Splash[] = Array.from({ length: splashLimit }, () => ({
    alive: false,
    x: 0,
    y: 0,
    z: 0,
    age: 0,
  }))
  let dropCursor = 0
  let splashCursor = 0

  function makeTexture(
    w: number,
    h: number,
    paint: (data: Uint8ClampedArray) => void
  ): THREE.CanvasTexture {
    const canvas = document.createElement('canvas')
    canvas.width = w
    canvas.height = h
    const ctx = canvas.getContext('2d')!
    const img = ctx.createImageData(w, h)
    paint(img.data)
    ctx.putImageData(img, 0, 0)
    const tex = new THREE.CanvasTexture(canvas)
    tex.colorSpace = THREE.SRGBColorSpace
    return tex
  }

  // Bright core with a soft, cool halo.
  function createStreakTexture(): THREE.CanvasTexture {
    const w = 16
    const h = 128
    return makeTexture(w, h, (data) => {
      for (let y = 0; y < h; y++) {
        const v = y / (h - 1)
        const vertical = Math.sin(Math.PI * Math.pow(v, 0.75))
        for (let x = 0; x < w; x++) {
          const dx = (x - (w - 1) / 2) / (w / 2)
          const core = Math.max(0, 1 - dx * dx * 4.5)
          const halo = Math.max(0, 1 - dx * dx * 1.4)
          const i = (y * w + x) * 4
          data[i] = Math.round(96 + core * 132)
          data[i + 1] = Math.round(120 + core * 118)
          data[i + 2] = Math.round(150 + core * 100)
          data[i + 3] = Math.round(vertical * Math.max(core, halo * 0.55) * 255)
        }
      }
    })
  }

  function createSplashTexture(): THREE.CanvasTexture {
    const s = 64
    return makeTexture(s, s, (data) => {
      const c = (s - 1) / 2
      for (let y = 0; y < s; y++) {
        for (let x = 0; x < s; x++) {
          const r = Math.hypot(x - c, y - c) / c
          const bright = Math.max(0, 1 - Math.abs(r - 0.6) * 9)
          const dark = Math.max(0, 1 - Math.abs(r - 0.66) * 6)
          const i = (y * s + x) * 4
          data[i] = Math.round(96 + bright * 128)
          data[i + 1] = Math.round(120 + bright * 116)
          data[i + 2] = Math.round(150 + bright * 100)
          data[i + 3] = Math.round(Math.max(bright, dark * 0.6) ** 2 * 240)
        }
      }
    })
  }

  const rainGroup = new THREE.Group()
  let streakMesh: THREE.InstancedMesh | null = null
  let splashMesh: THREE.InstancedMesh | null = null
  let dropsAlive = 0
  let splashesAlive = 0
  let spawnAccumulator = 0
  const textures: THREE.Texture[] = []

  onDestroy(() => {
    for (const mesh of [streakMesh, splashMesh]) {
      if (!mesh) continue
      mesh.removeFromParent()
      mesh.dispose()
      mesh.geometry.dispose()
      const materials = Array.isArray(mesh.material)
        ? mesh.material
        : [mesh.material]
      for (const material of materials) material.dispose()
    }
    for (const texture of textures) texture.dispose()
  })

  function createPooledMesh(
    tex: THREE.CanvasTexture,
    width: number,
    height: number,
    count: number
  ): THREE.InstancedMesh {
    textures.push(tex)
    return createParticleInstancedMesh(
      createRainParticleMaterial(tex),
      width,
      height,
      count
    )
  }

  function init() {
    if (streakMesh) return
    streakMesh = createPooledMesh(createStreakTexture(), 0.03, 0.42, dropLimit)
    if (splashLimit > 0) {
      splashMesh = createPooledMesh(
        createSplashTexture(),
        0.3,
        0.3,
        splashLimit
      )
      splashMesh.renderOrder = 1
    }
    streakMesh.renderOrder = 2
  }

  export function getGroup(): THREE.Group {
    return rainGroup
  }

  const tmpMatrix = new THREE.Matrix4()
  const tmpPos = new THREE.Vector3()
  const tmpScale = new THREE.Vector3()
  const camRight = new THREE.Vector3()
  const camUp = new THREE.Vector3()
  const camBack = new THREE.Vector3()
  const streakRight = new THREE.Vector3()
  const streakUp = new THREE.Vector3()
  const zeroMatrix = new THREE.Matrix4().makeScale(0, 0, 0)
  const flatQuat = new THREE.Quaternion().setFromEuler(
    new THREE.Euler(-Math.PI / 2, 0, 0)
  )

  function groundHeightAt(x: number, z: number, fallback: number): number {
    if (heightManager?.hasHeightData(x, z)) {
      return heightManager.getHeightAtWorldPosition(x, z)
    }
    return fallback
  }

  function spawnDrop(
    px: number,
    py: number,
    pz: number,
    windVX: number,
    windVZ: number
  ) {
    const angle = Math.random() * Math.PI * 2
    const dist = Math.sqrt(Math.random()) * SPAWN_RADIUS
    const x = px + Math.cos(angle) * dist
    const z = pz + Math.sin(angle) * dist

    const d = drops[dropCursor]
    dropCursor = (dropCursor + 1) % dropLimit
    if (d.alive) return

    // Pick the landing point, then start upwind so the splash lands there.
    const fallHeight =
      SPAWN_HEIGHT_MIN + Math.random() * (SPAWN_HEIGHT_MAX - SPAWN_HEIGHT_MIN)
    d.vy = -(FALL_SPEED_MIN + Math.random() * (FALL_SPEED_MAX - FALL_SPEED_MIN))
    const fallTime = fallHeight / -d.vy
    d.x = x - windVX * fallTime
    d.z = z - windVZ * fallTime
    d.groundY = groundHeightAt(x, z, py)
    d.y = d.groundY + fallHeight
    d.age = 0
    d.baseOpacity = 0.3 + Math.random() * 0.28
    d.scale = 0.85 + Math.random() * 0.4
    d.alive = true
  }

  function spawnSplash(x: number, y: number, z: number) {
    if (splashLimit === 0) return
    const s = splashes[splashCursor]
    splashCursor = (splashCursor + 1) % splashLimit
    if (s.alive) return
    s.x = x
    s.y = y + 0.03
    s.z = z
    s.age = 0
    s.alive = true
  }

  /** `intensity` 0..1 gates spawning. */
  export function update(
    deltaTime: number,
    camera: THREE.Camera | undefined,
    intensity: number,
    wind: WindSample | null
  ) {
    if (!camera) return
    if (intensity <= 0 && dropsAlive === 0 && splashesAlive === 0) {
      spawnAccumulator = 0
      return
    }
    init()

    const dt = Math.min(deltaTime / 1000, 0.1)
    const windSpeed = (wind?.windStrength ?? 0) * WIND_SPEED_AT_FULL
    const windVX = (wind?.windDirX ?? 0) * windSpeed
    const windVZ = (wind?.windDirZ ?? 0) * windSpeed
    camera.matrixWorld.extractBasis(camRight, camUp, camBack)
    const windRight = windVX * camRight.x + windVZ * camRight.z
    const windUp = windVX * camUp.x + windVZ * camUp.z

    if (intensity > 0 && playerPosition) {
      spawnAccumulator += dt
      const spawnInterval = 1.0 / (SPAWN_RATE_AT_FULL * intensity)
      while (spawnAccumulator >= spawnInterval) {
        spawnAccumulator -= spawnInterval
        spawnDrop(
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

    const streakOpacity = streakMesh!.geometry.getAttribute(
      PARTICLE_OPACITY_ATTR
    ) as THREE.InstancedBufferAttribute
    const streakArr = streakOpacity.array as Float32Array
    let aliveD = 0
    for (let i = 0; i < drops.length; i++) {
      const d = drops[i]
      if (!d.alive) continue

      d.age += dt
      d.y += d.vy * dt
      d.x += windVX * dt
      d.z += windVZ * dt

      if (d.y <= d.groundY || d.age > 3) {
        d.alive = false
        streakMesh!.setMatrixAt(i, zeroMatrix)
        streakArr[i] = 0
        if (d.age <= 3) spawnSplash(d.x, d.groundY, d.z)
        continue
      }
      aliveD++

      streakArr[i] =
        d.age < 0.06 ? d.baseOpacity * (d.age / 0.06) : d.baseOpacity
      // Camera-facing streak rolled to its on-screen direction of travel.
      const sx = -(windRight + d.vy * camRight.y)
      const sy = -(windUp + d.vy * camUp.y)
      const len = Math.sqrt(sx * sx + sy * sy)
      const ux = len > 1e-6 ? sx / len : 0
      const uy = len > 1e-6 ? sy / len : 1
      streakRight.copy(camRight).multiplyScalar(uy).addScaledVector(camUp, -ux)
      streakUp.copy(camRight).multiplyScalar(ux).addScaledVector(camUp, uy)
      streakUp.multiplyScalar(d.scale)
      tmpMatrix
        .makeBasis(streakRight, streakUp, camBack)
        .setPosition(d.x, d.y, d.z)
      streakMesh!.setMatrixAt(i, tmpMatrix)
    }

    dropsAlive = aliveD
    syncMesh(streakMesh!, streakOpacity, aliveD)
    updateSplashes(dt)
  }

  function updateSplashes(dt: number) {
    if (!splashMesh) return
    const splashOpacity = splashMesh.geometry.getAttribute(
      PARTICLE_OPACITY_ATTR
    ) as THREE.InstancedBufferAttribute
    const splashArr = splashOpacity.array as Float32Array
    let aliveS = 0
    for (let i = 0; i < splashes.length; i++) {
      const s = splashes[i]
      if (!s.alive) continue
      s.age += dt
      const t = s.age / SPLASH_LIFE
      if (t >= 1) {
        s.alive = false
        splashMesh.setMatrixAt(i, zeroMatrix)
        splashArr[i] = 0
        continue
      }
      aliveS++
      const grow = 0.45 + t * 0.85
      splashArr[i] = 0.65 * (1 - t)
      tmpPos.set(s.x, s.y, s.z)
      tmpScale.set(grow, grow, grow)
      tmpMatrix.compose(tmpPos, flatQuat, tmpScale)
      splashMesh.setMatrixAt(i, tmpMatrix)
    }

    splashesAlive = aliveS
    syncMesh(splashMesh, splashOpacity, aliveS)
  }

  /** An empty pool leaves the group so it costs no draw call. */
  function syncMesh(
    mesh: THREE.InstancedMesh,
    opacity: THREE.InstancedBufferAttribute,
    alive: number
  ) {
    if (alive > 0) {
      mesh.instanceMatrix.needsUpdate = true
      opacity.needsUpdate = true
      if (!mesh.parent) rainGroup.add(mesh)
    } else if (mesh.parent) {
      rainGroup.remove(mesh)
    }
  }
</script>

<T is={rainGroup} />
