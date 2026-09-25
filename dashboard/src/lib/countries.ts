const UNKNOWN_COUNTRY = 'ZZ'

export interface CountryEntry {
  country: string
  accounts: number
  sessions: number
  current_accounts: number
}

export interface CountryStats {
  from: number
  until: number
  collection_started_at: number
  last_aggregated_at: number | null
  accounts: number
  current_accounts: number
  countries: CountryEntry[]
}

const isCount = (value: unknown) => Number.isSafeInteger(value) && (value as number) >= 0

export function parseCountryStats(value: unknown): CountryStats {
  if (!value || typeof value !== 'object') throw new Error('Invalid country stats response')
  const data = value as CountryStats
  if (!isCount(data.from) || !isCount(data.until) || data.from > data.until
    || !isCount(data.collection_started_at)
    || (data.last_aggregated_at !== null && (!isCount(data.last_aggregated_at) || data.last_aggregated_at > data.until))
    || !isCount(data.accounts) || !isCount(data.current_accounts) || !Array.isArray(data.countries)) {
    throw new Error('Invalid country stats response')
  }
  const seen = new Set<string>()
  for (const entry of data.countries) {
    if (!entry || typeof entry !== 'object' || typeof entry.country !== 'string'
      || !/^[A-Z]{2}$/.test(entry.country) || seen.has(entry.country)
      || !isCount(entry.accounts) || !isCount(entry.sessions) || !isCount(entry.current_accounts)
      || entry.accounts > entry.sessions) {
      throw new Error('Invalid country stats entry')
    }
    seen.add(entry.country)
  }
  return data
}

const regionNames = new Intl.DisplayNames('ko-KR', { type: 'region' })

export function countryName(code: string): string {
  if (code === UNKNOWN_COUNTRY) return '미분류'
  try {
    return regionNames.of(code) ?? code
  } catch {
    return code
  }
}

export const share = (accounts: number, total: number) => total > 0 ? accounts / total * 100 : 0
