import { derived, writable } from 'svelte/store'
import { locale } from '../i18n'
import {
  debuffPresentation,
  formatRemaining,
  type PresentedDebuff,
} from '../data/debuffPresentation'

/** Server-supplied debuffs; `until` is a Date.now timestamp. */
export interface ActiveDebuff {
  id: string
  until: number
}

export const activeDebuffs = writable<ActiveDebuff[]>([])

/** Tick once a second while debuffs are active. */
export const visibleDebuffs = derived<
  [typeof activeDebuffs, typeof locale],
  PresentedDebuff[]
>(
  [activeDebuffs, locale],
  ([$active, language], set) => {
    const update = () => {
      const now = Date.now()
      set(
        $active
          .filter((d) => d.until > now)
          .map((d) => ({
            ...debuffPresentation(d.id, language),
            id: d.id,
            remaining: formatRemaining(d.until - now, language),
          }))
      )
    }
    update()
    if ($active.length === 0) return
    const timer = setInterval(update, 1_000)
    return () => clearInterval(timer)
  },
  []
)

export function resetDebuffStore() {
  activeDebuffs.set([])
}
