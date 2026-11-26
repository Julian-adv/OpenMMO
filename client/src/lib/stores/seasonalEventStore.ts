import { writable } from 'svelte/store'
import type { MonthDay, Season, SeasonSchedule } from '../utils/seasonDates'

export interface SeasonalEvent {
  id: string
  default_season: Season
  schedule: SeasonSchedule
  props: number
  /** Whether its props stand today. */
  active: boolean
}

export interface SeasonalEventsPanel {
  /** The Korean (UTC+9) date the server reads schedules against; `null` until it answers. */
  today: MonthDay | null
  events: SeasonalEvent[]
  error: string | null
  pending: boolean
}

/** The operator's seasonal event panel; `null` while it is closed. */
export const seasonalEventsPanel = writable<SeasonalEventsPanel | null>(null)

export function openSeasonalEvents() {
  seasonalEventsPanel.set({
    today: null,
    events: [],
    error: null,
    pending: true,
  })
}

export function applySeasonalEvents(data: {
  today: MonthDay
  events: SeasonalEvent[]
  error?: string | null
}) {
  seasonalEventsPanel.update(
    (panel) =>
      panel && {
        today: data.today,
        events: data.events,
        error: data.error ?? null,
        pending: false,
      }
  )
}

export function markSeasonalEventsPending() {
  seasonalEventsPanel.update(
    (panel) => panel && { ...panel, error: null, pending: true }
  )
}

export function closeSeasonalEvents() {
  seasonalEventsPanel.set(null)
}
