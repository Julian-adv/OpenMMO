import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { PlayerHealthDisplay } from './playerHealthDisplay'

let display: PlayerHealthDisplay
const impacts = vi.fn<(health: number, hit: boolean) => void>()

beforeEach(() => {
  vi.useFakeTimers()
  display = new PlayerHealthDisplay()
  display.sync({ id: 1, health: 100 })
  impacts.mockClear()
})

afterEach(() => vi.useRealTimers())

function attack(health: number, delay: number, hit = true) {
  display.sync({ id: 1, health })
  const apply = display.prepareImpact(1, hit, health)
  setTimeout(() => {
    if (apply()) impacts(health, hit)
  }, delay)
}

describe('player health display', () => {
  it('does not restore health when an older monster impact arrives last', () => {
    attack(80, 1042)
    vi.advanceTimersByTime(10)
    attack(50, 625)
    expect(get(display.displayed)).toBe(100)

    vi.advanceTimersByTime(625)
    expect(get(display.displayed)).toBe(50)
    vi.advanceTimersByTime(407)
    expect(get(display.displayed)).toBe(50)
    expect(impacts.mock.calls).toEqual([
      [50, true],
      [80, true],
    ])
  })

  it('keeps successive damage drops on their own impact frames', () => {
    attack(80, 1042)
    vi.advanceTimersByTime(10)
    attack(50, 1042)
    vi.advanceTimersByTime(1031)
    expect(get(display.displayed)).toBe(100)
    vi.advanceTimersByTime(1)
    expect(get(display.displayed)).toBe(80)
    vi.advanceTimersByTime(10)
    expect(get(display.displayed)).toBe(50)
  })

  it('does not restore health when a delayed miss arrives', () => {
    attack(100, 1042, false)
    vi.advanceTimersByTime(10)
    attack(70, 625)
    vi.advanceTimersByTime(625)
    expect(get(display.displayed)).toBe(70)
    vi.runAllTimers()
    expect(get(display.displayed)).toBe(70)
    expect(impacts).toHaveBeenLastCalledWith(100, false)
  })

  it('does not show a pending hit early when a later miss arrives first', () => {
    attack(80, 1042)
    vi.advanceTimersByTime(10)
    attack(80, 625, false)
    vi.advanceTimersByTime(625)
    expect(get(display.displayed)).toBe(100)
    vi.runAllTimers()
    expect(get(display.displayed)).toBe(80)
  })

  it('shows partial healing immediately while old damage text is pending', () => {
    attack(50, 1042)
    display.sync({ id: 1, health: 70 })
    expect(get(display.displayed)).toBe(70)
    vi.runAllTimers()
    expect(get(display.displayed)).toBe(70)
    expect(impacts).toHaveBeenCalledWith(50, true)
  })

  it('does not apply pre-heal damage during a new attack', () => {
    attack(20, 1042)
    vi.advanceTimersByTime(10)
    display.sync({ id: 1, health: 100 })
    attack(50, 1200)
    vi.advanceTimersByTime(1032)
    expect(get(display.displayed)).toBe(100)
    vi.advanceTimersByTime(168)
    expect(get(display.displayed)).toBe(50)
  })

  it('keeps a dead player at zero when an earlier hit arrives', () => {
    attack(80, 1042)
    attack(0, 625)
    expect(get(display.displayed)).toBe(0)
    vi.runAllTimers()
    expect(get(display.displayed)).toBe(0)
    expect(impacts).toHaveBeenCalledTimes(2)
  })

  it('discards pre-respawn impacts while allowing new combat', () => {
    attack(20, 1042)
    attack(0, 625)
    display.sync({ id: 1, health: 80 })
    attack(50, 1200)
    vi.advanceTimersByTime(1042)
    expect(get(display.displayed)).toBe(80)
    expect(impacts).not.toHaveBeenCalled()
    vi.runAllTimers()
    expect(get(display.displayed)).toBe(50)
    expect(impacts).toHaveBeenCalledTimes(1)
  })

  it.each([1, 2])(
    'discards old impacts after reconnecting as player %i',
    (id) => {
      attack(50, 1042)
      display.sync(null)
      expect(get(display.displayed)).toBeNull()
      display.sync({ id, health: 100 })
      vi.runAllTimers()
      expect(get(display.displayed)).toBe(100)
      expect(impacts).not.toHaveBeenCalled()
    }
  )
})
