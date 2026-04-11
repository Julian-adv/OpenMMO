import { writable, derived, get } from 'svelte/store'
import { refractionEnabled, reflectionEnabled } from './debugStore'

export type QualityLevel = 'high' | 'medium' | 'low'

export interface GraphicsPreset {
  pixelRatioCap: number
  shadowMapSize: number
  antialias: boolean
  refraction: boolean
  reflection: boolean
  grassDensity: number
}

const PRESETS: Record<QualityLevel, GraphicsPreset> = {
  high: {
    pixelRatioCap: 2.0,
    shadowMapSize: 4096,
    antialias: true,
    refraction: true,
    reflection: true,
    grassDensity: 1.0,
  },
  medium: {
    pixelRatioCap: 1.5,
    shadowMapSize: 2048,
    antialias: false,
    refraction: true,
    reflection: true,
    grassDensity: 1.0,
  },
  low: {
    pixelRatioCap: 1.0,
    shadowMapSize: 1024,
    antialias: false,
    refraction: false,
    reflection: false,
    grassDensity: 0.5,
  },
}

const STORAGE_KEY = 'onlinerpg_graphicsQuality'
const STORAGE_KEY_APPLIED_AA = 'onlinerpg_appliedAA'

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
  const aa = PRESETS[loadQuality()].antialias
  try {
    localStorage.setItem(STORAGE_KEY_APPLIED_AA, String(aa))
  } catch {
    // localStorage unavailable
  }
  return aa
}

export const graphicsQuality = writable<QualityLevel>(loadQuality())

/** True when the current preset's antialias differs from what the renderer was created with. */
export const reloadNeeded = derived(graphicsQuality, (level) => {
  try {
    const appliedAA = localStorage.getItem(STORAGE_KEY_APPLIED_AA) === 'true'
    return PRESETS[level].antialias !== appliedAA
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
  const preset = PRESETS[level]
  refractionEnabled.set(preset.refraction)
  reflectionEnabled.set(preset.reflection)
})

export function getPreset(level: QualityLevel): GraphicsPreset {
  return PRESETS[level]
}

export function getCurrentPreset(): GraphicsPreset {
  return PRESETS[get(graphicsQuality)]
}
