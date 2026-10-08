import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import {
  acknowledgeDaggerSkill,
  daggerSkillState,
  queueDaggerSkill,
  resetDaggerSkill,
} from './daggerSkillStore'

beforeEach(resetDaggerSkill)

describe('Double Slash queue', () => {
  it('stays locked until the acknowledged cooldown ends', () => {
    expect(queueDaggerSkill(100)).toBe(true)
    acknowledgeDaggerSkill(10000, 200)
    expect(get(daggerSkillState).queued).toBe(false)
    expect(queueDaggerSkill(10199)).toBe(false)
    expect(queueDaggerSkill(10200)).toBe(true)
  })

  it('a second press cancels the queued skill without starting cooldown', () => {
    queueDaggerSkill(100)
    queueDaggerSkill(101)
    expect(get(daggerSkillState).queued).toBe(false)
    expect(get(daggerSkillState).cooldownUntil).toBe(0)
  })
})
