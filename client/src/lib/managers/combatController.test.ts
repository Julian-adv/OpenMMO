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
    expect(controller.attackCounter).toBe(0)
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

  it('plays a confirmed server attack when the target is still in range', () => {
    const controller = new CombatController()
    controller.beginCombat('m1', true)
    controller.attackConfirmed('m1')

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

    expect(result).toEqual({ action: 'attack_cycle', rotation: Math.PI / 2 })
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

  describe('server-driven attacks', () => {
    const player = { x: 0, y: 0, z: 0 }
    const monster = { x: 1.5, y: 0, z: 0 }
    const attacking = () => {
      const controller = new CombatController()
      const stop = vi.fn()
      controller.attackStopRequested.on(stop)
      controller.beginCombat('m1', true)
      return { controller, stop }
    }
    const tick = (
      controller: CombatController,
      deltaTime = 16,
      monsterPos = monster
    ) =>
      controller.update(
        deltaTime,
        player,
        { state: 'idle' },
        monsterPos,
        false,
        1533,
        'attack',
        false,
        2
      )

    it('sends one start request under repeated clicks and never schedules its own swings', () => {
      const controller = new CombatController()
      let starts = 0
      for (let frame = 0; frame < 400; frame++) {
        if (controller.beginCombat('m1', true).startRequested) starts++
        expect(tick(controller).action).toBe('attacking')
      }
      expect(starts).toBe(1)
      expect(controller.attackCounter).toBe(0)
    })

    it('advances the animation only for confirmed server attacks', () => {
      const controller = new CombatController()
      controller.beginCombat('m1', true)
      controller.attackConfirmed('m1')
      expect(tick(controller).action).toBe('attack_cycle')
      expect(controller.attackCounter).toBe(1)
      expect(tick(controller, 5000).action).toBe('attacking')
      controller.attackConfirmed('m1')
      expect(tick(controller).action).toBe('attack_cycle')
      expect(controller.attackCounter).toBe(2)
    })

    it('stops the server attack once when combat is cancelled', () => {
      const { controller, stop } = attacking()
      const requestId = controller.attackRequestId
      controller.cancelCombat()
      controller.cancelCombat()
      expect(stop).toHaveBeenCalledExactlyOnceWith(requestId)
      controller.attackConfirmed('m1')
      expect(tick(controller).action).toBe('none')
    })

    it('clears combat for movement without a stop request or a late idle transition', () => {
      const { controller, stop } = attacking()
      const requestId = controller.attackRequestId
      controller.attackConfirmed('m1')
      controller.cancelCombat({ notifyServer: false })
      controller.attackStopped('m1', requestId)
      controller.attackConfirmed('m1')
      expect(stop).not.toHaveBeenCalled()
      expect(controller.targetMonsterId).toBeNull()
      expect(controller.attackCounter).toBe(0)
      expect(tick(controller).action).toBe('none')
    })

    it('keeps the chase target and lets movement stop the server attack', () => {
      const { controller, stop } = attacking()
      expect(tick(controller, 16, { x: 4, y: 0, z: 0 }).action).toBe('chasing')
      expect(stop).not.toHaveBeenCalled()
      expect(controller.attackRequested).toBe(false)
      expect(controller.targetMonsterId).toBe('m1')
      controller.attackStopped('m1', controller.attackRequestId)
      expect(controller.targetMonsterId).toBe('m1')
      expect(controller.beginCombat('m1', true).startRequested).toBe(true)
    })

    it('lets movement stop the old attack when chasing a different monster', () => {
      const { controller, stop } = attacking()
      const requestId = controller.attackRequestId
      expect(controller.beginCombat('m2', false).startRequested).toBe(false)
      controller.attackStopped('m1', requestId)
      expect(stop).not.toHaveBeenCalled()
      expect(controller.targetMonsterId).toBe('m2')
      expect(controller.beginCombat('m2', true).startRequested).toBe(true)
    })

    it('returns to idle after the server stops the current request', () => {
      const { controller, stop } = attacking()
      controller.attackStopped('m1', controller.attackRequestId)
      expect(tick(controller).action).toBe('idle')
      expect(tick(controller).action).toBe('none')
      expect(stop).not.toHaveBeenCalled()
    })

    it('ignores an old stop after restarting combat with the same monster', () => {
      const controller = new CombatController()
      controller.beginCombat('m1', true)
      const oldRequest = controller.attackRequestId
      controller.cancelCombat()
      controller.beginCombat('m1', true)
      controller.attackStopped('m1', oldRequest)
      expect(controller.attackRequested).toBe(true)
    })

    it('ignores confirmations and stops for a previous target', () => {
      const controller = new CombatController()
      controller.beginCombat('m1', true)
      const requestId = controller.attackRequestId
      controller.beginCombat('m2', true)
      controller.attackConfirmed('m1')
      controller.attackStopped('m1', requestId)
      expect(controller.attackCounter).toBe(0)
      expect(controller.targetMonsterId).toBe('m2')
    })
  })
})
