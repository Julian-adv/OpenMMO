import timing from '../../../../data/player_anim_timing.json'

export const DAGGER_SKILL = {
  clip: 'dagger_double_slash',
  icon: '/icons/skills/dagger-double-slash-v2.png',
  weaponType: 'dagger',
  pack: '/models/animations/dagger_preview.glb',
  duration: timing.dagger_skill_duration.delayMs / 1000,
  hits: [
    timing.dagger_skill_first_hit.delayMs / 1000,
    timing.dagger_skill_second_hit.delayMs / 1000,
  ],
  cooldownMs: timing.dagger_skill_cooldown.delayMs,
} as const
