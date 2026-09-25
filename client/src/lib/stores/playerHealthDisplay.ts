import { writable } from 'svelte/store'

export class PlayerHealthDisplay {
  readonly displayed = writable<number | null>(null)
  private playerId: number | null = null
  private currentHealth: number | null = null
  private generation = 0
  private sequence = 0
  private lastImpactSequence = 0

  sync(player: { id: number; health: number } | null) {
    const playerId = player?.id ?? null
    const health = player?.health ?? null
    if (playerId === this.playerId && health === this.currentHealth) return

    const changedPlayer = playerId !== this.playerId
    const revived = this.currentHealth === 0 && health !== null && health > 0
    if (changedPlayer || revived) this.generation++

    if (
      changedPlayer ||
      health === null ||
      this.currentHealth === null ||
      health > this.currentHealth ||
      health === 0
    ) {
      this.lastImpactSequence = ++this.sequence
      this.displayed.set(health)
    }
    this.playerId = playerId
    this.currentHealth = health
  }

  prepareImpact(playerId: number, hit: boolean, health: number) {
    const generation = this.generation
    const sequence = ++this.sequence
    return () => {
      if (playerId !== this.playerId || generation !== this.generation) {
        return false
      }
      if (hit && sequence > this.lastImpactSequence) {
        this.lastImpactSequence = sequence
        this.displayed.set(health)
      }
      return true
    }
  }
}

export const playerHealthDisplay = new PlayerHealthDisplay()
