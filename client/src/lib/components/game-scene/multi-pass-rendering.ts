import type * as THREE from 'three'
import type { RefractionRenderManager } from '../../managers/refractionRenderManager'
import type { ReflectionRenderManager } from '../../managers/reflectionRenderManager'
import type { LoopProfiler } from './loop-profiler'

export interface MultiPassRefractionDeps {
  camera: THREE.OrthographicCamera | undefined
  refractionManager: RefractionRenderManager | null
  refractionEnabled: boolean
  hasWater: boolean
  waterGroup: THREE.Group | undefined
  terrainMeshes: (THREE.Mesh | undefined)[]
  hiddenGroups: (THREE.Group | undefined)[]
}

export interface MultiPassReflectionDeps {
  camera: THREE.OrthographicCamera | undefined
  reflectionManager: ReflectionRenderManager | null
  reflectionEnabled: boolean
  hasWater: boolean
  waterGroup: THREE.Group | undefined
  terrainGroup: THREE.Group | undefined
  housingGroup: THREE.Group | undefined
  hiddenGroups: (THREE.Group | undefined)[]
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
  tickWarmup(): void
  isReady(): boolean
}

const MULTI_PASS_WARMUP_FRAMES = 5
/** Renders each pass must complete before the no-water gate may skip it, so
 *  its pipelines are compiled during the loading dialog even when the player
 *  spawns inland. */
const MULTI_PASS_WARMUP_RENDERS = 2

export function createMultiPassRenderer(): MultiPassRenderer {
  let ready = false
  let warmupFrames = 0
  let frameCount = 0
  let refractionRenders = 0
  let reflectionRenders = 0
  // `clear()` is itself a render pass, so only issue it on the transition
  // into the inactive state rather than every frame we stay there.
  let refractionCleared = false
  let reflectionCleared = false

  // Render refraction/reflection from the first frame so their WebGPU
  // pipelines compile while the loading dialog is still visible. Otherwise
  // the first refraction/reflection render happens after the dialog is gone
  // and stalls the main thread for hundreds of ms when the player moves.
  function tickWarmup() {
    if (ready) {
      frameCount++
      return
    }
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

    const gated =
      !deps.hasWater && refractionRenders >= MULTI_PASS_WARMUP_RENDERS

    if (deps.refractionManager && deps.refractionEnabled && ready && !gated) {
      // Alternate-frame: render refraction on even frames.
      // First frame (frameCount <= 1) always renders to initialize the texture.
      if (frameCount <= 1 || frameCount % 2 === 0) {
        refractionRenders++
        refractionCleared = false
        if (deps.camera) deps.refractionManager.setCamera(deps.camera)
        if (deps.waterGroup)
          deps.refractionManager.setWaterGroup(deps.waterGroup)

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

        renderWithHiddenGroups(deps.hiddenGroups, () =>
          deps.refractionManager!.render()
        )

        if (brushUniforms?.brushActive) {
          brushUniforms.brushActive.value = savedBrushActive
          brushUniforms.gridVisible.value = savedGridVisible
        }
      }
      // else: skip this frame, keep previous refraction texture
    } else if (deps.refractionManager && !refractionCleared) {
      deps.refractionManager.clear()
      refractionCleared = true
    }

    loopProfiler.record('refractionPass', performance.now() - start)
  }

  function renderReflection(
    deps: MultiPassReflectionDeps,
    loopProfiler: LoopProfiler
  ) {
    const start = performance.now()

    const gated =
      !deps.hasWater && reflectionRenders >= MULTI_PASS_WARMUP_RENDERS

    if (deps.reflectionManager && deps.reflectionEnabled && ready && !gated) {
      // Alternate-frame: render reflection on odd frames.
      // First frame (frameCount <= 1) always renders to initialize the texture.
      if (frameCount <= 1 || frameCount % 2 === 1) {
        reflectionRenders++
        reflectionCleared = false
        if (deps.camera) deps.reflectionManager.setCamera(deps.camera)
        deps.reflectionManager.setTerrainGroup(deps.terrainGroup ?? null)
        if (deps.waterGroup)
          deps.reflectionManager.setWaterGroup(deps.waterGroup)
        deps.reflectionManager.setHousingGroup(deps.housingGroup ?? null)
        // Hide nametags/HP bars during reflection render
        const nametagGroups = deps.getNametagGroups()
        for (const nt of nametagGroups) nt.visible = false

        // Hide groups that would trigger first-use pipeline compiles
        // (~100–150ms stall per new material).
        renderWithHiddenGroups(deps.hiddenGroups, () =>
          deps.reflectionManager!.render()
        )

        for (const nt of nametagGroups) nt.visible = true
      }
      // else: skip this frame, keep previous reflection texture
    } else if (deps.reflectionManager && !reflectionCleared) {
      deps.reflectionManager.clear()
      reflectionCleared = true
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
