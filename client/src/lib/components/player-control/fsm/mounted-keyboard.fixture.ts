import {
  DEFAULT_MOVEMENT_CONFIG,
  scaleMovementConfig,
  SPRINT_SPEED_MULT,
  type Position,
} from '../../../utils/movementUtils'
import { wrapWorldX } from '../../../terrain/world-wrap'
import {
  createKeyboardMoveSender,
  createKeyboardSpeedRamp,
  runKeyboardFrame,
} from './keyboard'

export function recordKeyboardTravel(mounted: boolean, fps = 60) {
  const start = { x: -1497, y: 5, z: 4740 }
  const player = { position: { ...start } }
  let rotation = Math.PI / 2
  let moving = false
  let tick = 0
  let sprinting = false
  const commands: {
    tick: number
    position: Position
    rotation: number
    sprinting: boolean
    forward: number
  }[] = []
  const checkpoints: { tick: number; position: Position; rotation: number }[] =
    []
  const moveSender = createKeyboardMoveSender((position, facing, forward) => {
    commands.push({
      tick,
      position: { ...position },
      rotation: facing,
      sprinting: sprinting && forward > 0,
      forward,
    })
  })
  const speedRamp = createKeyboardSpeedRamp()
  const unexpected = () => {
    throw new Error('Unexpected interruption on flat ground')
  }
  const actions = {
    exitPickupInteraction: unexpected,
    exitObjectInteraction: unexpected,
    clearClickMovement: unexpected,
    cancelCombat: unexpected,
    markMoving: () => {
      moving = true
    },
    setKeyboardIdleRuntime: () => {
      moving = false
    },
    emitKeyboardPlayerState: () => {},
    stopMovement: unexpected,
    triggerJumpFeedback: unexpected,
    setMoved: (_speed: number, facing: number) => {
      rotation = facing
    },
  }
  for (let frame = 0; frame < fps * 62; frame++) {
    const seconds = frame / fps
    tick = Math.floor(frame / (fps / 5)) + 1
    const pressed = seconds < 60
    const forward = seconds < 4 ? 0 : seconds < 12 ? 1 : seconds < 16 ? -1 : 1
    const turn =
      seconds < 12
        ? Math.floor(seconds / 2) % 2
          ? -1
          : 1
        : seconds < 14
          ? -1
          : 0
    sprinting =
      (!mounted || forward > 0) &&
      (seconds >= 20 || Math.floor(seconds) % 2 === 0)
    runKeyboardFrame({
      currentPlayer: player,
      isKeyboardMoving: moving,
      interactionExit: 'none',
      hasMovementTarget: false,
      isInCombat: false,
      input: pressed ? { forward, turn } : null,
      movementMode: 'character',
      rotation,
      config: {
        ...scaleMovementConfig(
          DEFAULT_MOVEMENT_CONFIG,
          (mounted ? 3 : 1) * (sprinting ? SPRINT_SPEED_MULT : 1)
        ),
        ...(mounted ? { mountRotation: rotation } : {}),
      },
      deltaTimeSeconds: 1 / fps,
      sampleHeight: () => 5,
      isMovementBlocked: () => false,
      isUphillTooSteep: () => false,
      writePlayerPosition: (position, facing) => {
        player.position = { ...position, x: wrapWorldX(position.x) }
        rotation = facing
      },
      moveSender,
      speedRamp,
      actions,
    })
    if ((frame + 1) % (fps * 2) === 0) {
      checkpoints.push({ tick, position: { ...player.position }, rotation })
    }
  }
  return {
    mounted,
    start,
    rotation: Math.PI / 2,
    ticks: tick,
    commands,
    checkpoints,
  }
}
