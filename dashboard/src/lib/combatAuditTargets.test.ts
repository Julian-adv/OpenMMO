import { describe, expect, it } from 'vitest'
import { parseCombatAuditTargets } from './combatAuditTargets'

describe('combat audit targets', () => {
  it('preserves unknown targets alongside resolved names and accepts an empty list', () => {
    const data = { until: 1_800_000_000, entries: [{ character_id: 1, name: '용사A' }, { character_id: 99, name: null }] }
    expect(parseCombatAuditTargets(data)).toEqual(data)
    expect(parseCombatAuditTargets({ ...data, entries: [] }).entries).toEqual([])
  })

  it('rejects malformed data instead of reporting a misleading target list', () => {
    const entry = { character_id: 1, name: '용사A' }
    for (const entries of [null, [null], [entry, entry], [{ ...entry, character_id: 0 }],
      [{ ...entry, character_id: 1.5 }], [{ ...entry, name: undefined }], [{ ...entry, name: '' }]]) {
      expect(() => parseCombatAuditTargets({ until: 1_800_000_000, entries })).toThrow()
    }
    expect(() => parseCombatAuditTargets(null)).toThrow()
    expect(() => parseCombatAuditTargets({ until: -1, entries: [] })).toThrow()
  })
})
