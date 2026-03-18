<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import type { WindState } from '../../shaders/grass-material'
  import {
    createWindParticleMaterial,
    loadDandelionSeedTexture,
    loadGrassLeafTexture,
    loadPetalTexture,
    PARTICLE_OPACITY_ATTR,
  } from '../../shaders/wind-particle-material'

  interface Props {
    playerPosition?: THREE.Vector3 | null
  }

  let { playerPosition = null }: Props = $props()

  // ── Configuration ────────────────────────────────────────
  const MAX_PER_TYPE = 60
  const WIND_SPAWN_THRESHOLD = 0.4
  const GRASS_COUNT_THRESHOLD = 20
  const SPAWN_RADIUS = 20
  const SPAWN_HEIGHT_MIN = 1.5
  const SPAWN_HEIGHT_MAX = 5.0

  type ParticleType = 'dandelion' | 'grass' | 'petal'

  // ── Particle data ────────────────────────────────────────
  interface Particle {
    alive: boolean
    age: number
    maxAge: number
    x: number
    y: number
    z: number
    vx: number
    vy: number
    vz: number
    baseScale: number
    phase: number
    /** Age ratio (0~1) at which fade-out begins — randomized per particle */
    fadeStart: number
    /** Billboard-local Z rotation (radians) */
    rot: number
    /** Rotation speed (radians/sec) */
    rotSpeed: number
  }

  function createPool(): Particle[] {
    return Array.from({ length: MAX_PER_TYPE }, () => ({
      alive: false,
      age: 0,
      maxAge: 0,
      x: 0,
      y: 0,
      z: 0,
      vx: 0,
      vy: 0,
      vz: 0,
      baseScale: 0,
      phase: 0,
      fadeStart: 0.7,
      rot: 0,
      rotSpeed: 0,
    }))
  }

  const dandelionPool = createPool()
  const grassLeafPool = createPool()
  const petalPool = createPool()

  // ── Async texture loading ────────────────────────────────
  let dandelionTex: THREE.Texture | null = null
  let grassLeafTex: THREE.Texture | null = null
  let petalTex: THREE.Texture | null = null

  Promise.all([
    loadDandelionSeedTexture(),
    loadGrassLeafTexture(),
    loadPetalTexture(),
  ]).then(([dt, gt, pt]) => {
    dandelionTex = dt
    grassLeafTex = gt
    petalTex = pt
  })

  // ── THREE.js objects ─────────────────────────────────────
  const particleGroup = new THREE.Group()
  let dandelionMesh: THREE.InstancedMesh | null = null
  let grassLeafMesh: THREE.InstancedMesh | null = null
  let petalMesh: THREE.InstancedMesh | null = null
  let initialized = false

  let dandelionAlive = 0
  let grassLeafAlive = 0
  let petalAlive = 0

  function createParticleMesh(
    material: THREE.Material,
    width: number,
    height: number,
  ): THREE.InstancedMesh {
    const geom = new THREE.PlaneGeometry(width, height)
    geom.setAttribute(
      PARTICLE_OPACITY_ATTR,
      new THREE.InstancedBufferAttribute(new Float32Array(MAX_PER_TYPE), 1),
    )

    const mesh = new THREE.InstancedMesh(geom, material, MAX_PER_TYPE)
    mesh.frustumCulled = false
    mesh.castShadow = false
    mesh.receiveShadow = false

    const zeroMat = new THREE.Matrix4().makeScale(0, 0, 0)
    for (let i = 0; i < MAX_PER_TYPE; i++) {
      mesh.setMatrixAt(i, zeroMat)
    }

    return mesh
  }

  /** Lazy init — material + mesh creation deferred to first spawn. */
  function init(): boolean {
    if (initialized) return true
    if (!dandelionTex || !grassLeafTex || !petalTex) return false

    dandelionMesh = createParticleMesh(
      createWindParticleMaterial(dandelionTex),
      1,
      1,
    )
    grassLeafMesh = createParticleMesh(
      createWindParticleMaterial(grassLeafTex),
      0.4,
      1,
    )
    petalMesh = createParticleMesh(
      createWindParticleMaterial(petalTex),
      0.5,
      0.7,
    )

    initialized = true
    return true
  }

  /** Expose group for visibility toggling during render passes. */
  export function getGroup(): THREE.Group {
    return particleGroup
  }

  // ── Reusable temporaries ─────────────────────────────────
  const tmpMatrix = new THREE.Matrix4()
  const tmpQuat = new THREE.Quaternion()
  const tmpSpinQuat = new THREE.Quaternion()
  const tmpFinalQuat = new THREE.Quaternion()
  const Z_AXIS = new THREE.Vector3(0, 0, 1)
  const tmpPos = new THREE.Vector3()
  const tmpScale = new THREE.Vector3()
  const zeroMatrix = new THREE.Matrix4().makeScale(0, 0, 0)
  let spawnAccumulator = 0

  // ── Spawn ────────────────────────────────────────────────
  function spawnParticle(
    pool: Particle[],
    windState: WindState,
    px: number,
    py: number,
    pz: number,
    type: ParticleType,
  ) {
    const slot = pool.findIndex((p) => !p.alive)
    if (slot === -1) return

    const p = pool[slot]
    const angle = Math.random() * Math.PI * 2
    const dist = Math.random() * SPAWN_RADIUS

    p.x = px + Math.cos(angle) * dist
    p.z = pz + Math.sin(angle) * dist
    p.y =
      py +
      SPAWN_HEIGHT_MIN +
      Math.random() * (SPAWN_HEIGHT_MAX - SPAWN_HEIGHT_MIN)

    // Per-particle direction: wind ± wide random spread
    const spread = 0.8 + Math.random() * 0.8
    const deviationAngle = (Math.random() - 0.5) * Math.PI * 0.6
    const cosD = Math.cos(deviationAngle)
    const sinD = Math.sin(deviationAngle)
    const baseVx = windState.windDirX * cosD - windState.windDirZ * sinD
    const baseVz = windState.windDirX * sinD + windState.windDirZ * cosD
    const windSpeed = windState.windStrength * 2.0 * spread
    p.vx = baseVx * windSpeed
    p.vz = baseVz * windSpeed

    switch (type) {
      case 'dandelion': {
        p.vy = 0.02 + Math.random() * 0.08
        p.maxAge = 5 + Math.random() * 4
        p.baseScale = 0.25 + Math.random() * 0.15
        p.rot = 0
        p.rotSpeed = 0
        break
      }
      case 'grass': {
        p.vy = -0.05 - Math.random() * 0.1
        p.maxAge = 3 + Math.random() * 4
        p.baseScale = 0.15 + Math.random() * 0.1
        p.rot = Math.random() * Math.PI * 2
        p.rotSpeed =
          (1.5 + Math.random() * 3.0) * (Math.random() < 0.5 ? 1 : -1)
        break
      }
      case 'petal': {
        p.vy = -0.02 - Math.random() * 0.08
        p.maxAge = 4 + Math.random() * 4
        p.baseScale = 0.18 + Math.random() * 0.12
        p.rot = Math.random() * Math.PI * 2
        p.rotSpeed =
          (1.0 + Math.random() * 2.5) * (Math.random() < 0.5 ? 1 : -1)
        break
      }
    }

    p.age = 0
    p.phase = Math.random() * Math.PI * 2
    p.fadeStart = 0.5 + Math.random() * 0.35
    p.alive = true
  }

  // ── Per-frame update ─────────────────────────────────────
  /** Returns number of alive particles after update. */
  function updatePool(
    pool: Particle[],
    mesh: THREE.InstancedMesh,
    dt: number,
    windState: WindState,
    type: ParticleType,
  ): number {
    const opacityAttr = mesh.geometry.getAttribute(
      PARTICLE_OPACITY_ATTR,
    ) as THREE.InstancedBufferAttribute
    const opacityArr = opacityAttr.array as Float32Array
    let alive = 0

    for (let i = 0; i < pool.length; i++) {
      const p = pool[i]

      // Dead slots were already zeroed when they died — skip entirely
      if (!p.alive) continue

      p.age += dt
      if (p.age >= p.maxAge) {
        p.alive = false
        mesh.setMatrixAt(i, zeroMatrix)
        opacityArr[i] = 0
        continue
      }

      alive++

      // Wind acceleration
      const windForce = windState.windStrength * 1.5
      p.vx += windState.windDirX * windForce * dt
      p.vz += windState.windDirZ * windForce * dt

      switch (type) {
        case 'dandelion': {
          // Gentle vertical bobbing
          p.vy += Math.sin(windState.time * 1.5 + p.phase) * 0.1 * dt
          p.vy = Math.max(-0.15, Math.min(0.2, p.vy))
          p.x += Math.sin(windState.time * 2.5 + p.phase) * 0.2 * dt
          p.z += Math.cos(windState.time * 2 + p.phase * 0.7) * 0.15 * dt
          break
        }
        case 'grass': {
          // Slight gravity + lateral tumble
          p.vy -= 0.3 * dt
          p.vy = Math.max(-0.5, p.vy)
          p.x += Math.sin(windState.time * 4 + p.phase) * 0.4 * dt
          p.z += Math.cos(windState.time * 3 + p.phase * 1.3) * 0.25 * dt
          break
        }
        case 'petal': {
          // Floaty descent with gentle swaying
          p.vy -= 0.15 * dt
          p.vy = Math.max(-0.35, p.vy)
          p.x += Math.sin(windState.time * 2.0 + p.phase) * 0.35 * dt
          p.z += Math.cos(windState.time * 1.8 + p.phase * 0.9) * 0.3 * dt
          break
        }
      }

      // Drag
      p.vx *= 1 - 0.5 * dt
      p.vz *= 1 - 0.5 * dt

      // Integrate
      p.x += p.vx * dt
      p.y += p.vy * dt
      p.z += p.vz * dt

      // Kill if too far below ground
      if (p.y < -10) {
        p.alive = false
        mesh.setMatrixAt(i, zeroMatrix)
        opacityArr[i] = 0
        alive--
        continue
      }

      // Opacity: quick fade in → hold → per-particle fade out
      const t = p.age / p.maxAge
      let opacity = 1.0
      if (t < 0.1) opacity = t / 0.1
      else if (t > p.fadeStart)
        opacity = 1.0 - (t - p.fadeStart) / (1.0 - p.fadeStart)

      opacityArr[i] = opacity

      // Advance rotation
      p.rot += p.rotSpeed * dt

      // Billboard matrix (with local Z spin for leaves/petals)
      const scale = p.baseScale * (1.0 - t * 0.3)
      tmpPos.set(p.x, p.y, p.z)
      tmpScale.set(scale, scale, scale)
      if (p.rotSpeed !== 0) {
        tmpSpinQuat.setFromAxisAngle(Z_AXIS, p.rot)
        tmpFinalQuat.copy(tmpQuat).multiply(tmpSpinQuat)
        tmpMatrix.compose(tmpPos, tmpFinalQuat, tmpScale)
      } else {
        tmpMatrix.compose(tmpPos, tmpQuat, tmpScale)
      }
      mesh.setMatrixAt(i, tmpMatrix)
    }

    if (alive > 0) {
      mesh.instanceMatrix.needsUpdate = true
      opacityAttr.needsUpdate = true
    }
    return alive
  }

  /**
   * Force WebGPU buffer re-upload only for meshes with alive particles.
   * When alive → 0, remove from scene to stop draw calls entirely.
   */
  function syncMeshToScene(
    mesh: THREE.InstancedMesh,
    prevAlive: number,
    nowAlive: number,
  ) {
    if (nowAlive > 0) {
      if (mesh.parent) particleGroup.remove(mesh)
      particleGroup.add(mesh)
    } else if (prevAlive > 0) {
      if (mesh.parent) particleGroup.remove(mesh)
    }
  }

  /** Called from GameScene's game loop each frame. */
  export function update(
    deltaTime: number,
    camera: THREE.Camera | undefined,
    windState: WindState | undefined,
    nearbyGrassCount = 0,
  ) {
    if (!windState || !camera) return

    const canSpawn =
      playerPosition &&
      windState.windStrength > WIND_SPAWN_THRESHOLD &&
      nearbyGrassCount >= GRASS_COUNT_THRESHOLD
    const totalAlive = dandelionAlive + grassLeafAlive + petalAlive

    // Nothing alive and can't spawn → skip entirely
    if (!canSpawn && totalAlive === 0) {
      spawnAccumulator = 0
      return
    }

    // Lazy init on first actual need (not during loading)
    if (!init()) return

    const dt = Math.min(deltaTime / 1000, 0.1)

    // Billboard orientation from camera
    tmpQuat.copy(camera.quaternion)

    // Spawn when wind is strong enough
    if (canSpawn) {
      spawnAccumulator += dt
      const s = windState.windStrength
      const spawnRate =
        s > 0.8
          ? (0.8 - WIND_SPAWN_THRESHOLD) * 8.0 + (s - 0.8) * 32.0
          : (s - WIND_SPAWN_THRESHOLD) * 8.0
      const spawnInterval = 1.0 / Math.max(spawnRate, 0.1)

      while (spawnAccumulator >= spawnInterval) {
        spawnAccumulator -= spawnInterval
        const roll = Math.random()
        const px = playerPosition!.x
        const py = playerPosition!.y
        const pz = playerPosition!.z
        if (roll < 0.3) {
          spawnParticle(dandelionPool, windState, px, py, pz, 'dandelion')
        } else if (roll < 0.65) {
          spawnParticle(grassLeafPool, windState, px, py, pz, 'grass')
        } else {
          spawnParticle(petalPool, windState, px, py, pz, 'petal')
        }
      }
    } else {
      spawnAccumulator = 0
    }

    // Simulate + write instance data
    const prevDandelion = dandelionAlive
    const prevGrass = grassLeafAlive
    const prevPetal = petalAlive
    dandelionAlive = updatePool(
      dandelionPool,
      dandelionMesh!,
      dt,
      windState,
      'dandelion',
    )
    grassLeafAlive = updatePool(
      grassLeafPool,
      grassLeafMesh!,
      dt,
      windState,
      'grass',
    )
    petalAlive = updatePool(
      petalPool,
      petalMesh!,
      dt,
      windState,
      'petal',
    )

    // Only touch scene graph for meshes that need GPU re-upload
    syncMeshToScene(dandelionMesh!, prevDandelion, dandelionAlive)
    syncMeshToScene(grassLeafMesh!, prevGrass, grassLeafAlive)
    syncMeshToScene(petalMesh!, prevPetal, petalAlive)
  }
</script>

<T is={particleGroup} />
