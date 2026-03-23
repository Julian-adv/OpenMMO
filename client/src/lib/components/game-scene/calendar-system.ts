import {
  GAME_START_YEAR,
  SUN_DAY_DURATION_SECONDS,
  SUN_START_HOUR,
  type CalendarDate,
  getCalendarDateFromGameDayIndex,
  getGameCalendarDayIndex,
} from '../../utils/celestialSimulation'
import type { ServerGameTime } from '../../stores/timeStore'

export interface CalendarSystemCallbacks {
  onDateChanged(year: number, month: number, day: number): void
  onHourChanged(hour: number): void
}

export interface CalendarSystem {
  getDate(): CalendarDate
  getGameHour(): number
  syncToServer(gameTime: ServerGameTime): void
  advance(deltaSeconds: number): void
  applyServerTimeIfAllowed(
    latestServerTime: ServerGameTime | null,
    sunTimeScale: number
  ): void
}

export function createCalendarSystem(
  callbacks: CalendarSystemCallbacks
): CalendarSystem {
  let calendarDate: CalendarDate = {
    year: GAME_START_YEAR,
    month: 1,
    day: 1,
  }
  let dayElapsedSeconds = (SUN_START_HOUR / 24) * SUN_DAY_DURATION_SECONDS

  function getGameHour() {
    return (dayElapsedSeconds / SUN_DAY_DURATION_SECONDS) * 24
  }

  function syncWidgets() {
    callbacks.onDateChanged(
      calendarDate.year,
      calendarDate.month,
      calendarDate.day
    )
    callbacks.onHourChanged(getGameHour())
  }

  function syncToServer(gameTime: ServerGameTime) {
    calendarDate = {
      year: gameTime.year,
      month: gameTime.month,
      day: gameTime.day,
    }
    dayElapsedSeconds =
      ((gameTime.hour + gameTime.minute / 60) / 24) * SUN_DAY_DURATION_SECONDS
    syncWidgets()
  }

  function addDays(daysToAdd: number) {
    if (daysToAdd === 0) return
    const currentDayIndex = getGameCalendarDayIndex(calendarDate)
    calendarDate = getCalendarDateFromGameDayIndex(currentDayIndex + daysToAdd)
  }

  function advance(deltaSeconds: number) {
    if (deltaSeconds <= 0) return
    dayElapsedSeconds += deltaSeconds
    if (dayElapsedSeconds < SUN_DAY_DURATION_SECONDS) return

    const elapsedDays = Math.floor(dayElapsedSeconds / SUN_DAY_DURATION_SECONDS)
    addDays(elapsedDays)
    dayElapsedSeconds -= elapsedDays * SUN_DAY_DURATION_SECONDS
    callbacks.onDateChanged(
      calendarDate.year,
      calendarDate.month,
      calendarDate.day
    )
  }

  function applyServerTimeIfAllowed(
    latestServerTime: ServerGameTime | null,
    sunTimeScale: number
  ) {
    if (sunTimeScale > 1) return
    if (latestServerTime === null) return
    syncToServer(latestServerTime)
  }

  function getDate(): CalendarDate {
    return calendarDate
  }

  return {
    getDate,
    getGameHour,
    syncToServer,
    advance,
    applyServerTimeIfAllowed,
  }
}
