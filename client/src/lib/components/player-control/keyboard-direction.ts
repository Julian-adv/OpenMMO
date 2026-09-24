import type { MoveDirection } from '../../network/networkTypes'
import type { KeyboardMovementMode } from '../../stores/movementSettings'

type Input = { forward: number; turn: number }

export class KeyboardDirectionSender {
  private key: string | null = null
  private input: Omit<MoveDirection, 'request_id'> | null = null
  private nextSendAt = 0

  constructor(
    private readonly send: (input: Omit<MoveDirection, 'request_id'>) => void,
    private readonly stop: () => void
  ) {}

  update(
    input: Input | null,
    rotation: number,
    mounted: boolean,
    mode: KeyboardMovementMode,
    sprinting: boolean
  ) {
    if (!input) {
      this.clear()
      return
    }
    const key = `${input.forward}:${input.turn}:${mounted}:${mode}:${sprinting}`
    if (key !== this.key) {
      const relative = mounted && mode === 'character'
      this.input = {
        rotation: relative
          ? rotation
          : mode === 'world'
            ? Math.atan2(input.turn, -input.forward)
            : rotation + Math.atan2(-input.turn, input.forward),
        forward: relative ? input.forward : 1,
        turn: relative ? input.turn : 0,
        sprinting,
      }
      this.key = key
      this.nextSendAt = 0
    }
    if (performance.now() < this.nextSendAt || !this.input) return
    this.send(this.input)
    this.nextSendAt = performance.now() + 100
  }

  clear() {
    if (this.key !== null) this.stop()
    this.reset()
  }

  reset() {
    this.key = null
    this.input = null
    this.nextSendAt = 0
  }
}
