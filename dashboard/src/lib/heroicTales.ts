export interface HeroicTale {
  line: number
  date: string
  hero: string
  brief: string
}

export interface HeroicTales {
  until: number
  available: boolean
  skipped_lines: number
  entries: HeroicTale[]
}

const nonnegativeInteger = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0
const nonemptyString = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0

export function parseHeroicTales(value: unknown): HeroicTales {
  if (!value || typeof value !== 'object') throw new Error('Invalid heroic tales response')
  const data = value as HeroicTales
  if (!nonnegativeInteger(data.until) || typeof data.available !== 'boolean'
    || !nonnegativeInteger(data.skipped_lines) || !Array.isArray(data.entries)
    || (!data.available && (data.entries.length > 0 || data.skipped_lines > 0))) {
    throw new Error('Invalid heroic tales response')
  }
  let previousLine = Infinity
  for (const entry of data.entries) {
    if (!entry || typeof entry !== 'object' || !nonnegativeInteger(entry.line)
      || entry.line === 0 || entry.line >= previousLine || !nonemptyString(entry.date)
      || !nonemptyString(entry.hero) || !nonemptyString(entry.brief)) {
      throw new Error('Invalid heroic tale entry')
    }
    previousLine = entry.line
  }
  return data
}
