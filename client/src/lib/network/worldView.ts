import { world_constants } from '../wasm/onlinerpg_shared'
import { shortestWrappedDeltaX } from '../terrain/world-wrap'

export type WorldEvent = {
  subject: string
  revision: number
  change: 'Enter' | 'Update' | 'Leave' | 'Delete'
  messages: unknown[]
}

export type WorldUpdate = {
  world_epoch: string
  generation: number
  sequence: number
  position: { x: number; y: number; z: number }
  floor_level: number
  ready: boolean
  reset: boolean
  events: WorldEvent[]
}

export class WorldView {
  epoch = ''
  generation = 0
  sequence = 0
  synchronized = false
  position: WorldUpdate['position'] | null = null
  floorLevel = 0
  subjects = new Map<string, number>()
  pendingTerrain = new Set<string>()
  staticReady = true
  private retiredEpochs = new Set<string>()
  private coverageRadiusSq?: number

  covers(x: number, z: number): boolean {
    if (
      !this.synchronized ||
      !this.staticReady ||
      !this.position ||
      this.pendingTerrain.size > 0
    )
      return false
    const dx = shortestWrappedDeltaX(this.position.x, x)
    this.coverageRadiusSq ??= (world_constants().eventDeliveryRadius - 1) ** 2
    return dx * dx + (z - this.position.z) ** 2 <= this.coverageRadiusSq
  }

  accept(update: WorldUpdate): boolean {
    const sameEpoch = this.epoch === update.world_epoch
    if (
      this.retiredEpochs.has(update.world_epoch) ||
      (sameEpoch &&
        (update.generation < this.generation ||
          (update.generation === this.generation &&
            update.sequence <= this.sequence)))
    )
      return false
    if (update.reset) {
      if (
        update.sequence !== 1 ||
        (sameEpoch && update.generation <= this.generation)
      ) {
        this.synchronized = false
        return false
      }
      if (this.epoch && !sameEpoch) this.retiredEpochs.add(this.epoch)
      this.epoch = update.world_epoch
      this.generation = update.generation
      this.sequence = 0
      this.subjects.clear()
      this.synchronized = true
    }
    if (
      !this.synchronized ||
      update.world_epoch !== this.epoch ||
      update.generation !== this.generation ||
      update.sequence !== this.sequence + 1 ||
      update.events.some(
        (event) => (this.subjects.get(event.subject) ?? 0) > event.revision
      )
    ) {
      this.synchronized = false
      return false
    }
    for (const event of update.events) {
      if (event.change === 'Leave' || event.change === 'Delete')
        this.subjects.delete(event.subject)
      else this.subjects.set(event.subject, event.revision)
    }
    this.sequence = update.sequence
    this.position = update.position
    this.floorLevel = update.floor_level
    this.synchronized = update.ready
    return true
  }
}

export const worldView = new WorldView()
