<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import {
    createWindParticleMaterial,
    PARTICLE_OPACITY_ATTR,
  } from '../../shaders/wind-particle-material'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'

  interface Props {
    playerPosition?: THREE.Vector3 | null
    heightManager?: TerrainHeightManager | null
  }

  let { playerPosition = null, heightManager = null }: Props = $props()

  // Covers the whole visible quarter-view area, not just the player's surroundings.
  const SPAWN_RADIUS = 36
  const MAX_DROPS = 1100
  const SPAWN_RATE_AT_FULL = 760
  const MAX_SPLASHES = 350
  const SPAWN_HEIGHT_MIN = 4
  const SPAWN_HEIGHT_MAX = 13
  const FALL_SPEED_MIN = 9
  const FALL_SPEED_MAX = 13
  const SPLASH_LIFE = 0.28
  const WIND_DRIFT_X = 0.4
  const WIND_DRIFT_Z = 0.2

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

  const drops: Drop[] = Array.from({ length: MAX_DROPS }, () => ({
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
  const splashes: Splash[] = Array.from({ length: MAX_SPLASHES }, () => ({
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

  // Bright core + darker cool halo so streaks read on bright grass and at night
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
  let initialized = false
  let dropsAlive = 0
  let splashesAlive = 0
  let spawnAccumulator = 0

  function createPooledMesh(
    tex: THREE.CanvasTexture,
    width: number,
    height: number,
    count: number
  ): THREE.InstancedMesh {
    const geom = new THREE.PlaneGeometry(width, height)
    geom.setAttribute(
      PARTICLE_OPACITY_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(count), 1)
    )
    const mesh = new THREE.InstancedMesh(
      geom,
      createWindParticleMaterial(tex),
      count
    )
    mesh.frustumCulled = false
    mesh.castShadow = false
    mesh.receiveShadow = false
    const zeroMat = new THREE.Matrix4().makeScale(0, 0, 0)
    for (let i = 0; i < count; i++) mesh.setMatrixAt(i, zeroMat)
    return mesh
  }

  function init(): boolean {
    if (initialized) return true
    streakMesh = createPooledMesh(createStreakTexture(), 0.03, 0.42, MAX_DROPS)
    splashMesh = createPooledMesh(createSplashTexture(), 0.3, 0.3, MAX_SPLASHES)
    splashMesh.renderOrder = 1
    streakMesh.renderOrder = 2
    initialized = true
    return true
  }

  export function getGroup(): THREE.Group {
    return rainGroup
  }

  const tmpMatrix = new THREE.Matrix4()
  const tmpQuat = new THREE.Quaternion()
  const tmpPos = new THREE.Vector3()
  const tmpScale = new THREE.Vector3()
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

  function spawnDrop(px: number, py: number, pz: number) {
    const angle = Math.random() * Math.PI * 2
    const dist = Math.sqrt(Math.random()) * SPAWN_RADIUS
    const x = px + Math.cos(angle) * dist
    const z = pz + Math.sin(angle) * dist

    const d = drops[dropCursor]
    dropCursor = (dropCursor + 1) % MAX_DROPS
    if (d.alive) return

    d.x = x
    d.z = z
    d.groundY = groundHeightAt(x, z, py)
    d.y =
      d.groundY +
      SPAWN_HEIGHT_MIN +
      Math.random() * (SPAWN_HEIGHT_MAX - SPAWN_HEIGHT_MIN)
    d.vy = -(FALL_SPEED_MIN + Math.random() * (FALL_SPEED_MAX - FALL_SPEED_MIN))
    d.age = 0
    d.baseOpacity = 0.3 + Math.random() * 0.28
    d.scale = 0.85 + Math.random() * 0.4
    d.alive = true
  }

  function spawnSplash(x: number, y: number, z: number) {
    const s = splashes[splashCursor]
    splashCursor = (splashCursor + 1) % MAX_SPLASHES
    if (s.alive) return
    s.x = x
    s.y = y + 0.03
    s.z = z
    s.age = 0
    s.alive = true
  }

  /** Called from GameScene's game loop. `intensity` 0..1 gates spawning. */
  export function update(
    deltaTime: number,
    camera: THREE.Camera | undefined,
    intensity: number
  ) {
    if (!camera) return
    if (intensity <= 0 && dropsAlive === 0 && splashesAlive === 0) {
      spawnAccumulator = 0
      if (streakMesh?.parent) rainGroup.remove(streakMesh)
      if (splashMesh?.parent) rainGroup.remove(splashMesh)
      return
    }
    if (!init()) return

    const dt = Math.min(deltaTime / 1000, 0.1)
    tmpQuat.copy(camera.quaternion)

    if (intensity > 0 && playerPosition) {
      spawnAccumulator += dt
      const spawnInterval = 1.0 / (SPAWN_RATE_AT_FULL * intensity)
      while (spawnAccumulator >= spawnInterval) {
        spawnAccumulator -= spawnInterval
        spawnDrop(playerPosition.x, playerPosition.y, playerPosition.z)
      }
    } else {
      spawnAccumulator = 0
    }

    // Streaks
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
      d.x += WIND_DRIFT_X * dt
      d.z += WIND_DRIFT_Z * dt

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
      tmpPos.set(d.x, d.y, d.z)
      tmpScale.set(1, d.scale, 1)
      tmpMatrix.compose(tmpPos, tmpQuat, tmpScale)
      streakMesh!.setMatrixAt(i, tmpMatrix)
    }

    // Splashes: quick expanding ring lying flat on the ground
    const splashOpacity = splashMesh!.geometry.getAttribute(
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
        splashMesh!.setMatrixAt(i, zeroMatrix)
        splashArr[i] = 0
        continue
      }
      aliveS++
      const grow = 0.45 + t * 0.85
      splashArr[i] = 0.65 * (1 - t)
      tmpPos.set(s.x, s.y, s.z)
      tmpScale.set(grow, grow, grow)
      tmpMatrix.compose(tmpPos, flatQuat, tmpScale)
      splashMesh!.setMatrixAt(i, tmpMatrix)
    }

    const prevD = dropsAlive
    const prevS = splashesAlive
    dropsAlive = aliveD
    splashesAlive = aliveS

    if (aliveD > 0) {
      streakMesh!.instanceMatrix.needsUpdate = true
      streakOpacity.needsUpdate = true
      if (streakMesh!.parent) rainGroup.remove(streakMesh!)
      rainGroup.add(streakMesh!)
    } else if (prevD > 0 && streakMesh!.parent) {
      rainGroup.remove(streakMesh!)
    }
    if (aliveS > 0) {
      splashMesh!.instanceMatrix.needsUpdate = true
      splashOpacity.needsUpdate = true
      if (splashMesh!.parent) rainGroup.remove(splashMesh!)
      rainGroup.add(splashMesh!)
    } else if (prevS > 0 && splashMesh!.parent) {
      rainGroup.remove(splashMesh!)
    }
  }
</script>

<T is={rainGroup} />
