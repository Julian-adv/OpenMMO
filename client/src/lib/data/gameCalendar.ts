import type { MessageKey, MessageValues } from '../i18n'

export const MONTH_NAME_KEYS = [
  'calendar.month.dawnmere',
  'calendar.month.reson',
  'calendar.month.verdant',
  'calendar.month.highsun',
  'calendar.month.emberfall',
  'calendar.month.redrain',
  'calendar.month.harvestwind',
  'calendar.month.gloam',
  'calendar.month.riftwane',
  'calendar.month.mistveil',
  'calendar.month.frostrest',
  'calendar.month.afterglow',
] as const satisfies readonly MessageKey[]

/** "{year} {month name} {day}" in the viewer's language. */
export function formatCalendarDate(
  t: (key: MessageKey, values?: MessageValues) => string,
  { year, month, day }: { year: number; month: number; day: number }
): string {
  return t('calendar.date', { year, monthName: monthName(t, month), day })
}

/** "{month name} {day}" in the viewer's language. */
export function formatCalendarMonthDay(
  t: (key: MessageKey, values?: MessageValues) => string,
  { month, day }: { month: number; day: number }
): string {
  return t('calendar.monthDay', { monthName: monthName(t, month), day })
}

function monthName(
  t: (key: MessageKey, values?: MessageValues) => string,
  month: number
): string {
  const monthKey = MONTH_NAME_KEYS[month - 1]
  return monthKey ? t(monthKey) : t('calendar.monthNumber', { month })
}

const pad2 = (n: number) => String(n).padStart(2, '0')

/** Calendar date plus HH:MM. */
export function formatGameDateTime(
  t: (key: MessageKey, values?: MessageValues) => string,
  at: { year: number; month: number; day: number; hour: number; minute: number }
): string {
  return `${formatCalendarDate(t, at)} ${pad2(at.hour)}:${pad2(at.minute)}`
}
