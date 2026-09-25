import {
  moveHorse,
  resolveHorseSteps,
  keyboardRotation,
  KEYBOARD_TURN_RATE,
  BACKWARD_SPEED,
} from '../../../utils/horseMovement'
import type { MovementConfig, Position } from '../../../utils/movementUtils'
import { shortestWrappedDeltaX, wrapWorldX } from '../../../terrain/world-wrap'
import type { InteractionExitKind } from './interaction'
import type { KeyboardMovementMode } from '../../../stores/movementSettings'

export interface KeyboardInput {
  forward: number
  turn: number
}

export interface KeyboardTarget {
  position: Position
  rotation: number
  forward: number
}

export interface KeyboardMoveSender {
  target(
    position: Position,
    rotation: number,
    input: KeyboardInput,
    speed: number,
    dt: number,
    mounted: boolean,
    movementMode: KeyboardMovementMode
  ): KeyboardTarget
  commitTarget(): void
  flush(position: Position, rotation: number): void
  reset(): void
}

export function createKeyboardMoveSender(
  send: (position: Position, rotation: number, forward: number) => void
): KeyboardMoveSender {
  let target: KeyboardTarget | null = null
  let sent: KeyboardTarget | null = null
  let lastInput: KeyboardInput | null = null
  let lastMounted = false
  let lastMovementMode: KeyboardMovementMode | null = null
  let movementRotation = 0
  let lastSpeed = 0
  let elapsed = 0
  return {
    target(position, rotation, input, speed, dt, mounted, movementMode) {
      elapsed += dt
      const lookahead = Math.max(4, speed * 0.5)
      const relativeMounted = mounted && movementMode === 'character'
      const inputChanged =
        lastInput === null ||
        lastMounted !== mounted ||
        lastMovementMode !== movementMode ||
        lastInput.forward !== input.forward ||
        lastInput.turn !== input.turn
      if (!relativeMounted && inputChanged) {
        movementRotation =
          movementMode === 'world'
            ? Math.atan2(input.turn, -input.forward)
            : rotation + Math.atan2(-input.turn, input.forward)
      }
      const forward = relativeMounted ? input.forward : 1
      if (
        target === null ||
        inputChanged ||
        lastSpeed !== speed ||
        (relativeMounted && input.turn !== 0 && elapsed >= 0.1) ||
        (forward !== 0 &&
          Math.hypot(
            shortestWrappedDeltaX(position.x, target.position.x),
            target.position.z - position.z
          ) <=
            lookahead / 2)
      ) {
        const facing = relativeMounted
          ? rotation - input.turn * KEYBOARD_TURN_RATE * 0.15
          : movementRotation
        target = {
          position: {
            x: wrapWorldX(position.x + Math.sin(facing) * lookahead * forward),
            y: position.y,
            z: position.z + Math.cos(facing) * lookahead * forward,
          },
          rotation: facing,
          forward,
        }
        lastInput = { ...input }
        lastMounted = mounted
        lastMovementMode = movementMode
        lastSpeed = speed
        elapsed = 0
      }
      return target
    },
    commitTarget() {
      if (target !== null && target !== sent) {
        send(target.position, target.rotation, target.forward)
        sent = target
      }
    },
    flush(position, rotation) {
      if (sent !== null) send(position, rotation, sent.forward)
      target = null
      sent = null
      elapsed = 0
    },
    reset() {
      target = null
      sent = null
      elapsed = 0
      lastInput = null
    },
  }
}

// Keyboard starts use the click-move acceleration curve.
export interface KeyboardSpeedRamp {
  advance(config: MovementConfig, deltaTimeSeconds: number): number
  reset(): void
}

export function createKeyboardSpeedRamp(): KeyboardSpeedRamp {
  let speed = 0
  return {
    advance(config, deltaTimeSeconds) {
      speed = Math.min(
        speed + config.acceleration * deltaTimeSeconds,
        config.maxSpeed
      )
      return speed
    },
    reset() {
      speed = 0
    },
  }
}

interface KeyboardMovementInput {
  currentPos: Position
  input: KeyboardInput
  movementMode: KeyboardMovementMode
  rotation: number
  backwardSpeed?: number
  config: MovementConfig
  deltaTimeSeconds: number
  speedRamp: KeyboardSpeedRamp
  sampleHeight: (x: number, z: number) => number
  isMovementBlocked: (
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ) => boolean
  isUphillTooSteep: (
    x: number,
    z: number,
    y: number,
    dirX: number,
    dirZ: number
  ) => boolean
  writePlayerPosition: (position: Position, rotation: number) => void
  moveSender: KeyboardMoveSender
}

export type KeyboardMovementOutcome =
  | { kind: 'blocked' }
  | { kind: 'slope_blocked' }
  | {
      kind: 'moved'
      currentSpeed: number
      playerRotation: number
    }

export function applyKeyboardMovement({
  currentPos,
  input,
  movementMode,
  rotation,
  backwardSpeed = BACKWARD_SPEED,
  config,
  deltaTimeSeconds,
  speedRamp,
  sampleHeight,
  isMovementBlocked,
  isUphillTooSteep,
  writePlayerPosition,
  moveSender,
}: KeyboardMovementInput): KeyboardMovementOutcome {
  const dt = Math.max(0, Math.min(deltaTimeSeconds, 0.1))
  const mounted = config.mountRotation !== undefined
  const speed =
    mounted && movementMode === 'character' && input.forward < 0
      ? backwardSpeed
      : config.maxSpeed
  const target = moveSender.target(
    currentPos,
    rotation,
    input,
    speed,
    dt,
    mounted,
    movementMode
  )
  if (mounted && target.forward === 0) {
    speedRamp.reset()
    const facing = keyboardRotation(rotation, target.rotation, dt)
    writePlayerPosition(currentPos, facing)
    moveSender.commitTarget()
    return { kind: 'moved', currentSpeed: 0, playerRotation: facing }
  }
  if (mounted) {
    const reverseAngle = target.forward < 0 ? Math.PI : 0
    const result = moveHorse(
      currentPos,
      rotation + reverseAngle,
      speed,
      dt,
      target.position,
      config.mountTurnRadius
    )
    const steps = result.mountSteps ?? []
    for (const step of steps) step.rotation -= reverseAngle
    const path = resolveHorseSteps(steps, currentPos, rotation, {
      sampleHeight,
      isMovementBlocked,
      isUphillTooSteep,
    })
    writePlayerPosition(path.position, path.rotation)
    if (path.blocked) return { kind: path.blocked }
    moveSender.commitTarget()
    return {
      kind: 'moved',
      currentSpeed: result.newSpeed,
      playerRotation: path.rotation,
    }
  }
  const currentSpeed = speedRamp.advance(config, dt)
  const dx = shortestWrappedDeltaX(currentPos.x, target.position.x)
  const dz = target.position.z - currentPos.z
  const distance = Math.hypot(dx, dz)
  const fraction =
    distance > 0 ? Math.min(1, (currentSpeed * dt) / distance) : 0
  const newX = currentPos.x + dx * fraction
  const newZ = currentPos.z + dz * fraction
  if (isMovementBlocked(currentPos.x, currentPos.z, newX, newZ, currentPos.y)) {
    return { kind: 'blocked' }
  }
  if (
    distance > 0 &&
    isUphillTooSteep(
      currentPos.x,
      currentPos.z,
      currentPos.y,
      dx / distance,
      dz / distance
    )
  ) {
    return { kind: 'slope_blocked' }
  }
  const facing = target.rotation
  writePlayerPosition({ x: newX, y: sampleHeight(newX, newZ), z: newZ }, facing)
  moveSender.commitTarget()
  return { kind: 'moved', currentSpeed, playerRotation: facing }
}

export interface KeyboardMovementOutcomeActions {
  stopMovement: () => void
  triggerJumpFeedback: () => void
  setMoved: (currentSpeed: number, playerRotation: number) => void
}

export type KeyboardMovementOutcomeApplication =
  | { kind: 'handled' }
  | { kind: 'moved' }

export function applyKeyboardMovementOutcome(
  outcome: KeyboardMovementOutcome,
  actions: KeyboardMovementOutcomeActions
): KeyboardMovementOutcomeApplication {
  switch (outcome.kind) {
    case 'blocked':
      actions.stopMovement()
      return { kind: 'handled' }

    case 'slope_blocked':
      actions.stopMovement()
      actions.triggerJumpFeedback()
      return { kind: 'handled' }

    case 'moved':
      actions.setMoved(outcome.currentSpeed, outcome.playerRotation)
      return { kind: 'moved' }

    default: {
      const _exhaustive: never = outcome
      return _exhaustive
    }
  }
}

interface KeyboardFramePlayer {
  position: Position
}

export interface KeyboardFrameActions extends KeyboardMovementOutcomeActions {
  exitPickupInteraction: () => void
  exitObjectInteraction: () => void
  clearClickMovement: () => void
  cancelCombat: () => void
  markMoving: () => void
  setKeyboardIdleRuntime: () => void
  emitKeyboardPlayerState: () => void
}

interface RunKeyboardFrameInput {
  currentPlayer: KeyboardFramePlayer | null
  isKeyboardMoving: boolean
  interactionExit: InteractionExitKind
  hasMovementTarget: boolean
  isInCombat: boolean
  input: KeyboardInput | null
  movementMode: KeyboardMovementMode
  rotation: number
  backwardSpeed?: number
  config: MovementConfig
  deltaTimeSeconds: number
  sampleHeight: (x: number, z: number) => number
  isMovementBlocked: (
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ) => boolean
  isUphillTooSteep: (
    x: number,
    z: number,
    y: number,
    dirX: number,
    dirZ: number
  ) => boolean
  writePlayerPosition: (position: Position, rotation: number) => void
  moveSender: KeyboardMoveSender
  speedRamp: KeyboardSpeedRamp
  actions: KeyboardFrameActions
}

export function runKeyboardFrame({
  currentPlayer,
  isKeyboardMoving,
  interactionExit,
  hasMovementTarget,
  isInCombat,
  input,
  movementMode,
  rotation,
  backwardSpeed,
  config,
  deltaTimeSeconds,
  sampleHeight,
  isMovementBlocked,
  isUphillTooSteep,
  writePlayerPosition,
  moveSender,
  speedRamp,
  actions,
}: RunKeyboardFrameInput) {
  if (!currentPlayer || !input) {
    speedRamp.reset()
    if (!currentPlayer || hasMovementTarget || isInCombat) {
      moveSender.reset()
      return
    }
    moveSender.flush(currentPlayer.position, rotation)
    moveSender.reset()
    if (isKeyboardMoving) {
      actions.setKeyboardIdleRuntime()
      actions.emitKeyboardPlayerState()
    }
    return
  }

  if (interactionExit !== 'none') {
    if (interactionExit === 'pickup') {
      actions.exitPickupInteraction()
    } else {
      actions.exitObjectInteraction()
    }
  }

  if (hasMovementTarget) {
    actions.clearClickMovement()
    actions.cancelCombat()
  }

  if (isInCombat) {
    actions.cancelCombat()
  }

  const outcome = applyKeyboardMovement({
    currentPos: {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    },
    input,
    movementMode,
    rotation,
    backwardSpeed,
    config,
    deltaTimeSeconds,
    speedRamp,
    sampleHeight,
    isMovementBlocked,
    isUphillTooSteep,
    writePlayerPosition: (position, facing) => {
      rotation = facing
      writePlayerPosition(position, facing)
      actions.markMoving()
    },
    moveSender,
  })

  const keyboardApplication = applyKeyboardMovementOutcome(outcome, actions)
  if (keyboardApplication.kind === 'handled') {
    // Stop at the last safe point and discard acceleration into the obstacle.
    speedRamp.reset()
    moveSender.flush(currentPlayer.position, rotation)
    return
  }

  actions.emitKeyboardPlayerState()
}
