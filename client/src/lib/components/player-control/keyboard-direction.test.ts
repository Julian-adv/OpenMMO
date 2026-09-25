import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { KeyboardDirectionSender } from './keyboard-direction'

beforeEach(() => vi.useFakeTimers({ toFake: ['performance'] }))
afterEach(() => vi.useRealTimers())

it('renews held input once per second while sending changes and release immediately', () => {
  const send = vi.fn()
  const stop = vi.fn()
  const sender = new KeyboardDirectionSender(send, stop)
  for (let frame = 0; frame < 60; frame++) {
    sender.update({ forward: 1, turn: 0 }, frame, false, 'world', false)
    vi.advanceTimersByTime(16)
  }
  expect(send).toHaveBeenCalledTimes(1)
  expect(send.mock.calls[0][0]).toEqual({
    rotation: Math.PI,
    forward: 1,
    turn: 0,
    sprinting: false,
  })
  vi.advanceTimersByTime(40)
  sender.update({ forward: 1, turn: 0 }, 0, false, 'world', false)
  expect(send).toHaveBeenCalledTimes(2)
  expect(send.mock.calls[1][0]).toEqual(send.mock.calls[0][0])
  vi.advanceTimersByTime(10)
  sender.update({ forward: 0, turn: 1 }, 0, false, 'world', true)
  expect(send).toHaveBeenCalledTimes(3)
  expect(send.mock.lastCall?.[0]).toMatchObject({
    rotation: Math.PI / 2,
    sprinting: true,
  })
  sender.update({ forward: 0, turn: 1 }, 0, false, 'world', false)
  expect(send).toHaveBeenCalledTimes(4)
  expect(send.mock.lastCall?.[0].sprinting).toBe(false)
  vi.advanceTimersByTime(999)
  sender.update({ forward: 0, turn: 1 }, 0, false, 'world', false)
  expect(send).toHaveBeenCalledTimes(4)
  sender.update(null, 0, false, 'world', false)
  vi.advanceTimersByTime(2000)
  sender.update(null, 0, false, 'world', false)
  sender.clear()
  expect(stop).toHaveBeenCalledTimes(1)
  expect(send).toHaveBeenCalledTimes(4)
})

it('keeps a mounted turn relative and does not feed display rotation back into input', () => {
  const send = vi.fn()
  const sender = new KeyboardDirectionSender(send, vi.fn())
  sender.update({ forward: -1, turn: 1 }, 0.5, true, 'character', true)
  vi.advanceTimersByTime(1000)
  sender.update({ forward: -1, turn: 1 }, 1, true, 'character', true)
  expect(send.mock.calls[0][0]).toEqual({
    rotation: 0.5,
    forward: -1,
    turn: 1,
    sprinting: true,
  })
  expect(send.mock.calls[1][0]).toEqual(send.mock.calls[0][0])
})
