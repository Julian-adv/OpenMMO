import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import {
  acknowledgeDaggerSkill,
  consumeDaggerSkill,
  daggerSkillState,
  queueDaggerSkill,
  resetDaggerSkill,
} from './daggerSkillStore'

beforeEach(resetDaggerSkill)

describe('Double Slash queue', () => {
  it('replaces just one attack and waits for the server acknowledgement', () => {
    expect(queueDaggerSkill(100)).toBe(true)
    expect(consumeDaggerSkill(150)).toBe(true)
    expect(consumeDaggerSkill(151)).toBe(false)
    expect(queueDaggerSkill(152)).toBe(false)
    acknowledgeDaggerSkill(10000, 200)
    expect(queueDaggerSkill(10199)).toBe(false)
    expect(queueDaggerSkill(10200)).toBe(true)
    expect(consumeDaggerSkill(10200)).toBe(true)
  })

  it('a second press cancels the queued skill without starting cooldown', () => {
    queueDaggerSkill(100)
    queueDaggerSkill(101)
    expect(consumeDaggerSkill(200)).toBe(false)
    expect(get(daggerSkillState).cooldownUntil).toBe(0)
  })
})
