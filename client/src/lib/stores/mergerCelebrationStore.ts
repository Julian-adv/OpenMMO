export type MergerCelebration = {
  x: number
  y: number
  z: number
  startedAt: number
}
let pending: MergerCelebration | null = null

export function queueMergerCelebration(position: {
  x: number
  y: number
  z: number
}) {
  pending = { ...position, startedAt: Date.now() }
}

export function takeMergerCelebration(): MergerCelebration | null {
  const event = pending
  pending = null
  return event
}
