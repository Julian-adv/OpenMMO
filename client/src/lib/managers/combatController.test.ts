import { describe, expect, it, vi } from 'vitest'
import { CombatController, type MonsterInfo } from './combatController'

vi.mock('./bgmManager', () => ({
  startBattleMusic: vi.fn(),
  stopBattleMusic: vi.fn(),
}))

describe('CombatController', () => {
  it('uses a hovered ability target without starting an attack or chase', () => {
    const controller = new CombatController()
    const target = controller.getAbilityTarget('hovered', () => ({
      state: 'idle',
      health: 10,
    }))
    expect(target).toBe('hovered')
    expect(controller.targetMonsterId).toBeNull()
    expect(controller.attackCounter).toBe(0)
    for (const distance of [2, 20]) {
      expect(
        controller.update(
          1500,
          { x: 0, y: 0, z: 0 },
          { state: 'idle' },
          { x: distance, y: 0, z: 0 },
          false,
          1500,
          'idle',
          false,
          10
        )
      ).toEqual({ action: 'none' })
    }
  })

  it('uses the hovered target ahead of the selected target without changing combat', () => {
    const controller = new CombatController()
    controller.beginCombat('selected', true)
    expect(
      controller.getAbilityTarget('hovered', () => ({ state: 'idle' }))
    ).toBe('hovered')
    expect(controller.targetMonsterId).toBe('selected')
    expect(controller.attackCounter).toBe(1)
    expect(controller.getAbilityTarget(null, () => ({ state: 'idle' }))).toBe(
      'selected'
    )
  })

  it.each<MonsterInfo | undefined>([
    undefined,
    { state: 'dead' },
    { state: 'hit', isDeadPending: true },
    { state: 'idle', health: 0 },
  ])('ignores unavailable or dying ability targets: %j', (invalid) => {
    const controller = new CombatController()
    controller.beginCombat('selected', true)
    expect(
      controller.getAbilityTarget('hovered', (id) =>
        id === 'selected' ? invalid : { state: 'idle', health: 10 }
      )
    ).toBe('hovered')
    expect(controller.getAbilityTarget('hovered', () => invalid)).toBeNull()
    expect(
      controller.getAbilityTarget('hovered', (id) =>
        id === 'hovered' ? invalid : { state: 'idle', health: 10 }
      )
    ).toBe('selected')
    controller.cancelCombat()
    expect(
      controller.getAbilityTarget(null, () => ({ state: 'idle' }))
    ).toBeNull()
  })

  it('re-approaches instead of starting a new attack cycle after the target flees out of range', () => {
    const controller = new CombatController()
    controller.beginCombat('m1', true)

    const result = controller.update(
      1500,
      { x: 0, y: 0, z: 0 },
      { state: 'run' },
      { x: 3.5, y: 0, z: 0 },
      false,
      1500,
      'attack',
      false,
      2
    )

    expect(result).toEqual({
      action: 'chasing',
      newTarget: { x: 3.5, y: 0, z: 0 },
    })
    expect(controller.isInCombat).toBe(true)
  })

  it('re-approaches when the target is outside player reach but still near monster reach', () => {
    const controller = new CombatController()
    controller.beginCombat('m1', true)

    const result = controller.update(
      1500,
      { x: 0, y: 0, z: 0 },
      { state: 'attack' },
      { x: 2.5, y: 0, z: 0 },
      false,
      1500,
      'attack',
      false,
      2
    )

    expect(result).toEqual({
      action: 'chasing',
      newTarget: { x: 2.5, y: 0, z: 0 },
    })
    expect(controller.isInCombat).toBe(true)
  })

  it('starts the next attack cycle when the target is still in range', () => {
    const controller = new CombatController()
    controller.beginCombat('m1', true)

    const result = controller.update(
      1500,
      { x: 0, y: 0, z: 0 },
      { state: 'idle' },
      { x: 1.5, y: 0, z: 0 },
      false,
      1500,
      'attack',
      false,
      2
    )

    expect(result).toEqual({
      action: 'attack_cycle',
      monsterId: 'm1',
      rotation: Math.PI / 2,
    })
  })

  it('re-approaches when a wall stands between us and a target in reach', () => {
    const controller = new CombatController()
    controller.beginCombat('m1', true)

    const result = controller.update(
      1500,
      { x: 0, y: 0, z: 0 },
      { state: 'idle' },
      { x: 1.5, y: 0, z: 0 },
      false,
      1500,
      'attack',
      true,
      2
    )

    expect(result).toEqual({
      action: 'chasing',
      newTarget: { x: 1.5, y: 0, z: 0 },
    })
  })
})
