import { describe, expect, it } from 'vitest'
import { recordKeyboardTravel } from './mounted-keyboard.fixture'
import { shortestWrappedDeltaX } from '../../../terrain/world-wrap'
import replay from './__fixtures__/mounted-keyboard.json'

describe('keyboard travel replay', () => {
  it('keeps the Rust replay tied to actual keyboard prediction and messages', () => {
    expect(
      [false, true].map((mounted) => recordKeyboardTravel(mounted))
    ).toEqual(replay)
  })

  for (const mounted of [false, true]) {
    it.each([30, 60, 120])(
      `steers, reverses and sprints for 40 seconds at %i fps (mounted=${mounted})`,
      (fps) => {
        const { commands, checkpoints } = recordKeyboardTravel(mounted, fps)
        expect(commands.length).toBeLessThan(330)
        expect(commands.some((command) => command.forward === 0)).toBe(mounted)
        expect(commands.some((command) => command.forward === -1)).toBe(mounted)
        expect(
          commands.every(
            (command) => !command.sprinting || command.forward === 1
          )
        ).toBe(true)
        const straight = checkpoints.find(
          (checkpoint) => checkpoint.tick === 100
        )!
        const stop = checkpoints.find((checkpoint) => checkpoint.tick === 300)!
        const distance = Math.hypot(
          shortestWrappedDeltaX(straight.position.x, stop.position.x),
          stop.position.z - straight.position.z
        )
        const speed = mounted ? 13.5 : 4.5
        expect(distance).toBeGreaterThan(40 * speed - 0.2)
        expect(distance).toBeLessThanOrEqual(40 * speed + 0.001)
        expect(checkpoints.at(-1)!.position).toEqual(stop.position)
        expect(commands.at(-1)!.position).toEqual(stop.position)
      }
    )
  }
})
