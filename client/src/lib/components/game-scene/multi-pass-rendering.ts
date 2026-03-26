import type * as THREE from 'three'
import type { ClippingGroup } from 'three/webgpu'
import type { RefractionRenderManager } from '../../managers/refractionRenderManager'
import type { ReflectionRenderManager } from '../../managers/reflectionRenderManager'
import type { LoopProfiler } from './loop-profiler'

export interface MultiPassRefractionDeps {
  camera: THREE.OrthographicCamera | undefined
  refractionManager: RefractionRenderManager | null
  refractionEnabled: boolean
  waterGroup: THREE.Group | undefined
  terrainMeshes: (THREE.Mesh | undefined)[]
  entityClipGroup: ClippingGroup | undefined
  grassGroup: THREE.Group | undefined
  windParticlesGroup: THREE.Group | undefined
}

export interface MultiPassReflectionDeps {
  camera: THREE.OrthographicCamera | undefined
  reflectionManager: ReflectionRenderManager | null
  reflectionEnabled: boolean
  waterGroup: THREE.Group | undefined
  terrainGroup: THREE.Group | undefined
  housingGroup: THREE.Group | undefined
  entityClipGroup: ClippingGroup | undefined
  grassGroup: THREE.Group | undefined
  windParticlesGroup: THREE.Group | undefined
  getNametagGroups: () => THREE.Group[]
}

export interface MultiPassRenderer {
  renderRefraction(
    deps: MultiPassRefractionDeps,
    loopProfiler: LoopProfiler
  ): void
  renderReflection(
    deps: MultiPassReflectionDeps,
    loopProfiler: LoopProfiler
  ): void
  tickWarmup(isSceneCompiling: boolean): void
  isReady(): boolean
}

const MULTI_PASS_WARMUP_FRAMES = 5

export function createMultiPassRenderer(): MultiPassRenderer {
  let ready = false
  let warmupFrames = 0

  function tickWarmup(isSceneCompiling: boolean) {
    if (ready || isSceneCompiling) return
    warmupFrames++
    if (warmupFrames >= MULTI_PASS_WARMUP_FRAMES) {
      ready = true
    }
  }

  function renderRefraction(
    deps: MultiPassRefractionDeps,
    loopProfiler: LoopProfiler
  ) {
    const start = performance.now()

    if (deps.refractionManager && deps.refractionEnabled && ready) {
      if (deps.camera) deps.refractionManager.setCamera(deps.camera)
      if (deps.waterGroup) deps.refractionManager.setWaterGroup(deps.waterGroup)

      // Hide brush/grid overlay during refraction so it doesn't show through water
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const brushUniforms = (deps.terrainMeshes[0]?.material as any)?.userData
        ?.uniforms
      let savedBrushActive: number | undefined
      let savedGridVisible: number | undefined
      if (brushUniforms?.brushActive) {
        savedBrushActive = brushUniforms.brushActive.value
        savedGridVisible = brushUniforms.gridVisible.value
        brushUniforms.brushActive.value = 0.0
        brushUniforms.gridVisible.value = 0.0
      }

      // Hide entities, grass, and particles during refraction
      renderWithHiddenGroups(
        [
          deps.entityClipGroup as THREE.Group | undefined,
          deps.grassGroup,
          deps.windParticlesGroup,
        ],
        () => deps.refractionManager!.render()
      )

      if (brushUniforms?.brushActive) {
        brushUniforms.brushActive.value = savedBrushActive
        brushUniforms.gridVisible.value = savedGridVisible
      }
    } else if (deps.refractionManager) {
      deps.refractionManager.clear()
    }

    loopProfiler.record('refractionPass', performance.now() - start)
  }

  function renderReflection(
    deps: MultiPassReflectionDeps,
    loopProfiler: LoopProfiler
  ) {
    const start = performance.now()

    if (deps.reflectionManager && deps.reflectionEnabled && ready) {
      if (deps.camera) deps.reflectionManager.setCamera(deps.camera)
      deps.reflectionManager.setTerrainGroup(deps.terrainGroup ?? null)
      if (deps.waterGroup) deps.reflectionManager.setWaterGroup(deps.waterGroup)
      deps.reflectionManager.setHousingGroup(deps.housingGroup ?? null)
      if (deps.entityClipGroup)
        deps.reflectionManager.setEntityClipGroup(deps.entityClipGroup)

      // Hide nametags/HP bars during reflection render
      const nametagGroups = deps.getNametagGroups()
      for (const nt of nametagGroups) nt.visible = false

      // Hide grass + particles during reflection
      renderWithHiddenGroups([deps.grassGroup, deps.windParticlesGroup], () =>
        deps.reflectionManager!.render()
      )

      for (const nt of nametagGroups) nt.visible = true
    } else if (deps.reflectionManager) {
      deps.reflectionManager.clear()
    }

    loopProfiler.record('reflectionPass', performance.now() - start)
  }

  return {
    renderRefraction,
    renderReflection,
    tickWarmup,
    isReady: () => ready,
  }
}

/** Hide a list of groups, run a callback, then restore visibility. */
export function renderWithHiddenGroups(
  groups: (THREE.Group | undefined)[],
  renderFn: () => void
) {
  const saved = groups.map((g) => g?.visible)
  for (const g of groups) {
    if (g) g.visible = false
  }
  renderFn()
  for (let i = 0; i < groups.length; i++) {
    if (groups[i]) groups[i]!.visible = saved[i] ?? true
  }
}
