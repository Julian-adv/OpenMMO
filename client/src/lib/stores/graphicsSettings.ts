import { writable, derived, get } from 'svelte/store'
import { refractionEnabled, reflectionEnabled } from './debugStore'

export type QualityLevel = 'high' | 'medium' | 'low'
export type RenderBudget = 'full' | 'mobile'

export interface GraphicsPreset {
  renderBudget: RenderBudget
  pixelRatioCap: number
  shadowMapSize: number
  torchShadowMapSize: number
  antialias: boolean
  refraction: boolean
  reflection: boolean
  /** Screen-size divisor for the refraction/reflection render targets. */
  waterRtDivisor: number
  /** Allow the per-pixel depth-buffer shoreline. Costs a full-res depth copy
   *  every frame water is on screen; the heightmap-ramp fallback is free. */
  waterPixelDepth: boolean
  grassDensity: number
  enableDirectionalShadows: boolean
  enableWaterLayer: boolean
  enableWaterEffects: boolean
  enableGrassLayer: boolean
  grassCastsShadow: boolean
  enableTreeLayer: boolean
  treeInstanceLimit: number
  treeCastsShadow: boolean
  enableWindParticles: boolean
  enableHousingLayer: boolean
  enableTorchEffects: boolean
  enableTorchShadows: boolean
  terrainQueueDrainTilesBeforeStagger: number
  terrainTileWorkPerFrame: number | undefined
  terrainMaterialPrecompilePoolSize: number
  initialTerrainQueueDrainCount: number
  initialTileWorkDrainCount: number
  warmupScenePipelines: boolean
  worldMapDefaultZoomSpan: number
  worldMapMaxZoomSpan: number
  worldMapImageCacheLimit: number
}

const FULL_RENDER_SETTINGS = {
  renderBudget: 'full',
  enableDirectionalShadows: true,
  enableWaterLayer: true,
  enableWaterEffects: true,
  enableGrassLayer: true,
  enableTreeLayer: true,
  enableHousingLayer: true,
  enableTorchEffects: true,
  enableTorchShadows: true,
  terrainQueueDrainTilesBeforeStagger: Infinity,
  terrainTileWorkPerFrame: undefined,
  terrainMaterialPrecompilePoolSize: 8,
  initialTerrainQueueDrainCount: Infinity,
  initialTileWorkDrainCount: Infinity,
  warmupScenePipelines: true,
  worldMapDefaultZoomSpan: 8,
  worldMapMaxZoomSpan: 32,
} satisfies Omit<
  GraphicsPreset,
  | 'pixelRatioCap'
  | 'shadowMapSize'
  | 'torchShadowMapSize'
  | 'antialias'
  | 'refraction'
  | 'reflection'
  | 'waterRtDivisor'
  | 'waterPixelDepth'
  | 'grassDensity'
  | 'grassCastsShadow'
  | 'treeInstanceLimit'
  | 'treeCastsShadow'
  | 'enableWindParticles'
  | 'worldMapImageCacheLimit'
>

const PRESETS: Record<QualityLevel, GraphicsPreset> = {
  high: {
    ...FULL_RENDER_SETTINGS,
    pixelRatioCap: 2.0,
    shadowMapSize: 4096,
    torchShadowMapSize: 1024,
    antialias: true,
    refraction: true,
    reflection: true,
    waterRtDivisor: 2,
    waterPixelDepth: true,
    grassDensity: 1.0,
    grassCastsShadow: true,
    treeInstanceLimit: 1024,
    treeCastsShadow: true,
    enableWindParticles: true,
    worldMapImageCacheLimit: Infinity,
  },
  medium: {
    ...FULL_RENDER_SETTINGS,
    pixelRatioCap: 1.5,
    shadowMapSize: 2048,
    torchShadowMapSize: 512,
    antialias: false,
    refraction: true,
    reflection: true,
    waterRtDivisor: 2,
    waterPixelDepth: true,
    grassDensity: 0.7,
    grassCastsShadow: false,
    treeInstanceLimit: 768,
    treeCastsShadow: true,
    enableWindParticles: true,
    worldMapImageCacheLimit: 256,
  },
  low: {
    ...FULL_RENDER_SETTINGS,
    pixelRatioCap: 1.0,
    shadowMapSize: 1024,
    torchShadowMapSize: 256,
    antialias: false,
    refraction: false,
    reflection: false,
    waterRtDivisor: 3,
    waterPixelDepth: false,
    grassDensity: 0.4,
    grassCastsShadow: false,
    treeInstanceLimit: 512,
    treeCastsShadow: false,
    enableWindParticles: false,
    worldMapImageCacheLimit: 128,
  },
}

const STORAGE_KEY = 'onlinerpg_graphicsQuality'
const STORAGE_KEY_APPLIED_AA = 'onlinerpg_appliedAA'

// Device render budgets are constant for the session, so compute each once and
// cache it. They're read on hot paths (grass streaming, graphics-quality changes)
// where re-running matchMedia/UA-regex per call would be wasted work.
let _mobileBudgetDetected: boolean | undefined
let _renderBudget: RenderBudget | undefined

function detectMobileRenderBudget(): boolean {
  if (_mobileBudgetDetected !== undefined) return _mobileBudgetDetected
  if (typeof window === 'undefined') return false

  const coarsePointer =
    window.matchMedia?.('(pointer: coarse)').matches ?? false
  const narrowViewport = Math.min(window.innerWidth, window.innerHeight) <= 600
  const touchDevice = navigator.maxTouchPoints > 0

  _mobileBudgetDetected = touchDevice && (coarsePointer || narrowViewport)
  return _mobileBudgetDetected
}

export function getDeviceRenderBudget(): RenderBudget {
  if (_renderBudget !== undefined) return _renderBudget
  if (typeof window === 'undefined') return 'full'

  const ua = navigator.userAgent
  const explicitIphone = /\biPhone\b/.test(ua)
  const tinyTouchViewport =
    navigator.maxTouchPoints > 0 &&
    Math.min(window.innerWidth, window.innerHeight) <= 430

  _renderBudget =
    detectMobileRenderBudget() || explicitIphone || tinyTouchViewport
      ? 'mobile'
      : 'full'
  return _renderBudget
}

function getMobileSafePreset(preset: GraphicsPreset): GraphicsPreset {
  return {
    ...preset,
    renderBudget: 'mobile',
    pixelRatioCap: Math.min(preset.pixelRatioCap, 0.75),
    shadowMapSize: Math.min(preset.shadowMapSize, 512),
    torchShadowMapSize: 256,
    antialias: false,
    refraction: false,
    reflection: false,
    waterRtDivisor: 3,
    waterPixelDepth: false,
    grassDensity: 0,
    enableDirectionalShadows: true,
    enableWaterLayer: true,
    enableWaterEffects: false,
    enableGrassLayer: false,
    grassCastsShadow: false,
    enableTreeLayer: true,
    treeInstanceLimit: 384,
    treeCastsShadow: false,
    enableWindParticles: false,
    enableHousingLayer: true,
    enableTorchEffects: true,
    enableTorchShadows: false,
    terrainQueueDrainTilesBeforeStagger: Infinity,
    terrainTileWorkPerFrame: 1,
    terrainMaterialPrecompilePoolSize: 1,
    initialTerrainQueueDrainCount: 1,
    initialTileWorkDrainCount: 2,
    warmupScenePipelines: false,
    worldMapDefaultZoomSpan: 2,
    worldMapMaxZoomSpan: 4,
    worldMapImageCacheLimit: 32,
  }
}

function loadQuality(): QualityLevel {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'high' || stored === 'medium' || stored === 'low')
      return stored
  } catch {
    // localStorage unavailable
  }
  return 'medium'
}

/**
 * Called once at renderer creation time.
 * Returns the antialias flag and records what was applied
 * so `reloadNeeded` can detect mismatches later.
 */
export function applyInitialAntialias(): boolean {
  const aa = getEffectivePreset(loadQuality()).antialias
  try {
    localStorage.setItem(STORAGE_KEY_APPLIED_AA, String(aa))
  } catch {
    // localStorage unavailable
  }
  return aa
}

/** Antialias flag the renderer was actually created with this session.
 *  Falls back to the current preset when the applied record is missing
 *  (first launch — `applyInitialAntialias` hasn't stored it yet). */
export function getAppliedAntialias(): boolean {
  try {
    const stored = localStorage.getItem(STORAGE_KEY_APPLIED_AA)
    if (stored !== null) return stored === 'true'
  } catch {
    // localStorage unavailable
  }
  return getEffectivePreset(loadQuality()).antialias
}

export const graphicsQuality = writable<QualityLevel>(loadQuality())

// Device budget and base presets are constant for the session, so the effective
// preset is a pure function of `level`. Cache it — `getCurrentPreset()` is read
// on hot paths (grass streaming) where re-spreading the mobile-safe object per
// call would be wasted allocation.
const _effectivePresetCache: Partial<Record<QualityLevel, GraphicsPreset>> = {}

/** True when the current preset's antialias differs from what the renderer was created with. */
export const reloadNeeded = derived(graphicsQuality, (level) => {
  try {
    const appliedAA = localStorage.getItem(STORAGE_KEY_APPLIED_AA) === 'true'
    return getEffectivePreset(level).antialias !== appliedAA
  } catch {
    return false
  }
})

// Sync to localStorage and debugStore on change
graphicsQuality.subscribe((level) => {
  try {
    localStorage.setItem(STORAGE_KEY, level)
  } catch {
    // localStorage unavailable
  }
  const preset = getEffectivePreset(level)
  refractionEnabled.set(preset.refraction)
  reflectionEnabled.set(preset.reflection)
})

export function getEffectivePreset(level: QualityLevel): GraphicsPreset {
  return (_effectivePresetCache[level] ??=
    getDeviceRenderBudget() === 'mobile'
      ? getMobileSafePreset(PRESETS[level])
      : PRESETS[level])
}

export function getCurrentPreset(): GraphicsPreset {
  return getEffectivePreset(get(graphicsQuality))
}
