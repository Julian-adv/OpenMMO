import { writable } from 'svelte/store'

export const QUICKSLOT_COUNT = 10

/**
 * Quickslot assignments. Each slot holds an item *definition* id (not an
 * instance id) so the binding survives stacks being consumed and re-created —
 * the bar always points at "the first Healing Potion in my bag", not a specific
 * one. `null` means the slot is empty.
 */
export const quickslots = writable<(string | null)[]>(
  new Array(QUICKSLOT_COUNT).fill(null)
)

/** localStorage key for the active character; null until a character loads. */
let storageKey: string | null = null

function persist(slots: (string | null)[]) {
  if (!storageKey) return
  try {
    localStorage.setItem(storageKey, JSON.stringify(slots))
  } catch {
    /* storage full or unavailable — quickslots just won't persist */
  }
}

/** Load this character's saved quickslots (call when a character is selected). */
export function loadQuickslots(characterId: number) {
  storageKey = `quickslots:${characterId}`
  const next: (string | null)[] = new Array(QUICKSLOT_COUNT).fill(null)
  try {
    const raw = localStorage.getItem(storageKey)
    const parsed = raw ? JSON.parse(raw) : null
    if (Array.isArray(parsed)) {
      for (let i = 0; i < QUICKSLOT_COUNT; i++) {
        if (typeof parsed[i] === 'string') next[i] = parsed[i]
      }
    }
  } catch {
    /* corrupt entry — fall back to empty */
  }
  quickslots.set(next)
}

export function assignQuickslot(index: number, itemDefId: string) {
  if (index < 0 || index >= QUICKSLOT_COUNT) return
  quickslots.update((slots) => {
    const next = [...slots]
    // Avoid the same item occupying two slots: clear any previous binding.
    for (let i = 0; i < next.length; i++) {
      if (next[i] === itemDefId) next[i] = null
    }
    next[index] = itemDefId
    persist(next)
    return next
  })
}

export function clearQuickslot(index: number) {
  if (index < 0 || index >= QUICKSLOT_COUNT) return
  quickslots.update((slots) => {
    const next = [...slots]
    next[index] = null
    persist(next)
    return next
  })
}

export function resetQuickslots() {
  storageKey = null
  quickslots.set(new Array(QUICKSLOT_COUNT).fill(null))
}
