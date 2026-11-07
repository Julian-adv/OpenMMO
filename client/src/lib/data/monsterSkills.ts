export const BARKAN_RUNE_SLASH = {
  windup: 1.6,
  slashEntry: 0.3,
  transitionEnd: 2.15,
  swingEase: 0.16,
  strikeStart: 2.25,
  strikeEnd: 2.78,
  sweepStart: 2.55,
  sweepEnd: 2.73,
  sweepBearings: [-81, 89],
  recoveryEnd: 3.9,
  duration: 4.2,
} as const

export const EARTH_ROAR_STRIKE_MS = 4000
export const EARTH_ROAR_CAST_MS = 6200
export const FORMATION_BREAK_STRIKE_MS = 2800
export const FORMATION_BREAK_CAST_MS = 4400
export const FORMATION_BREAK_POOL_MS = 10_000
export const EARTHQUAKE_STRIKE_MS = 3500
export const EARTHQUAKE_CAST_MS = 5000
export const CRYSTAL_ERUPTION_STRIKE_MS = 2400
export const CRYSTAL_ERUPTION_CAST_MS = 3367

export interface MonsterSkillDefinition {
  clip: string
  durationMs: number
  sound?: { url: string; atMs: number }
  marks?: {
    impactMs: number
    lingerMs?: number
    sound?: string
    crystal?: boolean
  }
}

export const MONSTER_SKILL_DEFS: Record<string, MonsterSkillDefinition> = {
  rune_slash: {
    clip: 'RuneSlash',
    durationMs: BARKAN_RUNE_SLASH.duration * 1000,
  },
  earth_roar: {
    clip: 'Roar',
    durationMs: EARTH_ROAR_CAST_MS,
    sound: {
      url: '/sounds/terraclash-roar.ogg',
      atMs: EARTH_ROAR_STRIKE_MS - 300,
    },
  },
  formation_break: {
    clip: 'Smash',
    durationMs: FORMATION_BREAK_CAST_MS,
    sound: {
      url: '/sounds/terraclash-smash.ogg',
      atMs: FORMATION_BREAK_STRIKE_MS,
    },
    marks: {
      impactMs: FORMATION_BREAK_STRIKE_MS,
      lingerMs: FORMATION_BREAK_POOL_MS,
    },
  },
  earthquake: {
    clip: 'Quake',
    durationMs: EARTHQUAKE_CAST_MS,
    sound: {
      url: '/sounds/terraclash-quake.ogg',
      atMs: EARTHQUAKE_STRIKE_MS - 3030,
    },
    marks: {
      impactMs: EARTHQUAKE_STRIKE_MS,
      sound: '/sounds/terraclash-eruption.ogg',
    },
  },
  crystal_eruption: {
    clip: 'slash3',
    durationMs: CRYSTAL_ERUPTION_CAST_MS,
    marks: {
      impactMs: CRYSTAL_ERUPTION_STRIKE_MS,
      sound: '/sounds/terraclash-eruption.ogg',
      crystal: true,
    },
  },
}
