export interface CombatAuditTarget {
  character_id: number
  name: string | null
}

export interface CombatAuditTargets {
  until: number
  entries: CombatAuditTarget[]
}

export function parseCombatAuditTargets(value: unknown): CombatAuditTargets {
  if (!value || typeof value !== 'object') throw new Error('Invalid combat audit targets response')
  const data = value as CombatAuditTargets
  if (!Number.isSafeInteger(data.until) || data.until < 0
    || !Array.isArray(data.entries) || data.entries.length > 128) {
    throw new Error('Invalid combat audit targets response')
  }
  let previousId = 0
  for (const entry of data.entries) {
    if (!entry || typeof entry !== 'object' || !Number.isSafeInteger(entry.character_id)
      || entry.character_id <= previousId
      || (entry.name !== null && (typeof entry.name !== 'string' || !entry.name.trim()))) {
      throw new Error('Invalid combat audit target')
    }
    previousId = entry.character_id
  }
  return data
}
