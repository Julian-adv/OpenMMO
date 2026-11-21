import { beforeEach, describe, expect, it } from 'vitest'
import { HEALING_POTION_COOLDOWN_MS } from '../data/combatTiming'
import { claimPotionCooldown, resetPotionCooldown } from './potionCooldownStore'

beforeEach(resetPotionCooldown)

describe('healing potion cooldown', () => {
  it('holds a second drink until the cooldown and send margin pass', () => {
    expect(claimPotionCooldown(1000)).toBe(true)
    expect(claimPotionCooldown(1000 + HEALING_POTION_COOLDOWN_MS)).toBe(false)
    expect(claimPotionCooldown(1000 + HEALING_POTION_COOLDOWN_MS + 100)).toBe(
      true
    )
  })
})
