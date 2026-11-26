/** Seasonal event dates, mirroring `shared/src/seasonal.rs`. */

export interface MonthDay {
  month: number
  day: number
}

/** First and last day of a run, both included. It may run past new year. */
export interface Season {
  start: MonthDay
  end: MonthDay
}

export type SeasonSchedule = 'Default' | 'Off' | { Period: Season }

export type SeasonMode = 'default' | 'period' | 'off'

const DAYS_IN_MONTH = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]

export function daysInMonth(month: number): number {
  return DAYS_IN_MONTH[month - 1] ?? 31
}

/** Keep the day inside its month after the month changed. */
export function clampDay({ month, day }: MonthDay): MonthDay {
  return { month, day: Math.min(day, daysInMonth(month)) }
}

export function formatMonthDay({ month, day }: MonthDay): string {
  return `${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`
}

function ordinal({ month, day }: MonthDay): number {
  return month * 100 + day
}

export function seasonContains(season: Season, date: MonthDay): boolean {
  const start = ordinal(season.start)
  const end = ordinal(season.end)
  const at = ordinal(date)
  return start <= end ? start <= at && at <= end : at >= start || at <= end
}

export function scheduleMode(schedule: SeasonSchedule): SeasonMode {
  if (schedule === 'Default') return 'default'
  return schedule === 'Off' ? 'off' : 'period'
}

/** The run a schedule gives an event, or `null` when it is off. */
export function scheduledSeason(
  schedule: SeasonSchedule,
  defaultSeason: Season
): Season | null {
  if (schedule === 'Default') return defaultSeason
  return schedule === 'Off' ? null : schedule.Period
}

export function buildSchedule(
  mode: SeasonMode,
  period: Season
): SeasonSchedule {
  if (mode === 'default') return 'Default'
  return mode === 'off' ? 'Off' : { Period: period }
}

export function sameSchedule(a: SeasonSchedule, b: SeasonSchedule): boolean {
  if (typeof a === 'string' || typeof b === 'string') return a === b
  return (
    ordinal(a.Period.start) === ordinal(b.Period.start) &&
    ordinal(a.Period.end) === ordinal(b.Period.end)
  )
}
