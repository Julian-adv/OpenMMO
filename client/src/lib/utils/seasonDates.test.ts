import { describe, expect, it } from 'vitest'
import {
  buildSchedule,
  clampDay,
  daysInMonth,
  formatMonthDay,
  sameSchedule,
  scheduleMode,
  scheduledSeason,
  seasonContains,
  type Season,
} from './seasonDates'

const day = (month: number, day: number) => ({ month, day })
const winter: Season = { start: day(12, 10), end: day(1, 5) }
const summer: Season = { start: day(6, 1), end: day(8, 31) }

describe('season dates', () => {
  it('covers both ends of a run across new year', () => {
    for (const date of [day(12, 10), day(12, 31), day(1, 1), day(1, 5)]) {
      expect(seasonContains(winter, date)).toBe(true)
    }
    for (const date of [day(12, 9), day(1, 6), day(7, 1)]) {
      expect(seasonContains(winter, date)).toBe(false)
    }
  })

  it('stops a run inside one year at its last day', () => {
    expect(seasonContains(summer, day(6, 1))).toBe(true)
    expect(seasonContains(summer, day(8, 31))).toBe(true)
    expect(seasonContains(summer, day(5, 31))).toBe(false)
    expect(seasonContains(summer, day(9, 1))).toBe(false)
  })

  it('keeps a day inside its month', () => {
    expect(daysInMonth(2)).toBe(29)
    expect(clampDay(day(2, 31))).toEqual(day(2, 29))
    expect(clampDay(day(4, 31))).toEqual(day(4, 30))
    expect(clampDay(day(12, 31))).toEqual(day(12, 31))
  })

  it('prints a day as the season command reads it', () => {
    expect(formatMonthDay(day(1, 5))).toBe('01-05')
    expect(formatMonthDay(day(12, 10))).toBe('12-10')
  })

  it('maps a schedule to its mode and run and back', () => {
    expect(scheduleMode('Default')).toBe('default')
    expect(scheduleMode('Off')).toBe('off')
    expect(scheduleMode({ Period: summer })).toBe('period')

    expect(scheduledSeason('Default', winter)).toBe(winter)
    expect(scheduledSeason({ Period: summer }, winter)).toBe(summer)
    expect(scheduledSeason('Off', winter)).toBeNull()

    expect(buildSchedule('default', summer)).toBe('Default')
    expect(buildSchedule('off', summer)).toBe('Off')
    expect(buildSchedule('period', summer)).toEqual({ Period: summer })
  })

  it('compares schedules by their dates', () => {
    expect(sameSchedule('Default', 'Default')).toBe(true)
    expect(sameSchedule('Default', 'Off')).toBe(false)
    expect(sameSchedule({ Period: summer }, 'Default')).toBe(false)
    expect(
      sameSchedule(
        { Period: summer },
        { Period: { start: day(6, 1), end: day(8, 31) } }
      )
    ).toBe(true)
    expect(sameSchedule({ Period: summer }, { Period: winter })).toBe(false)
  })
})
