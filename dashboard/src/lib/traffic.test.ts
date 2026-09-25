import { describe, expect, it } from 'vitest'
import { formatBytes, formatRate, parseAssetTraffic, parseNetworkHistory, perAccountRate, type AssetTraffic, type NetworkHistory } from './traffic'

const network = (): NetworkHistory => ({
  from: 1000, until: 4600, collection_started_at: 4500, collection_interval_seconds: 60,
  sample_interval_seconds: 60, available: true,
  latest: { timestamp: 4500, seconds: 120, rx_bytes: 1200, tx_bytes: 2400, accounts: 3, interface: 'eth0' },
  samples: [{ timestamp: 4500, seconds: 120, rx_bytes: 1200, tx_bytes: 2400 }], rx_bytes: 1200, tx_bytes: 2400,
})

const assets = (): AssetTraffic => ({
  from: 1000, until: 4600, configured: true, available: true,
  status: { started_at: 1000, updated_at: 4700, skipped_lines: 0, gaps: 0, pending_bytes: 0 },
  total_bytes: 300,
  categories: [{ category: 'model', path: '', bytes: 300, requests: 3, revalidations: 1 }],
  files: [{ category: 'model', path: '/models/tree.glb', bytes: 300, requests: 3, revalidations: 1 }],
})

describe('traffic metrics', () => {
  it('uses actual elapsed seconds and handles zero connected accounts', () => {
    const sample = network().latest!
    expect(perAccountRate(sample)).toBe(10)
    expect(perAccountRate({ ...sample, accounts: 0 })).toBeNull()
    expect(perAccountRate(null)).toBeNull()
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1024 * 1024)).toBe('1 MiB')
    expect(formatRate(1536)).toBe('1.5 KiB/s')
  })

  it('validates histories without fabricating unavailable samples', () => {
    expect(parseNetworkHistory(network(), 1).samples).toHaveLength(1)
    expect(parseNetworkHistory({ ...network(), latest: null, samples: [], rx_bytes: 0, tx_bytes: 0, available: false }, 1).available).toBe(false)
  })

  it('rejects invalid network durations, totals, and sample order', () => {
    const data = network()
    for (const invalid of [
      { ...data, until: 4700 },
      { ...data, rx_bytes: 1 },
      { ...data, latest: { ...data.latest, seconds: 0 } },
      { ...data, samples: [{ ...data.samples[0], timestamp: 900 }] },
      { ...data, samples: [data.samples[0], data.samples[0]], rx_bytes: 2400, tx_bytes: 4800 },
      { ...data, samples: [{ ...data.samples[0], seconds: Infinity }] },
    ]) expect(() => parseNetworkHistory(invalid, 1)).toThrow()
  })

  it('validates asset shares against the full category total, including 304 responses', () => {
    expect(parseAssetTraffic(assets(), 1).total_bytes).toBe(300)
    for (const invalid of [
      { ...assets(), total_bytes: 100 },
      { ...assets(), files: [{ ...assets().files[0], bytes: 999 }] },
      { ...assets(), files: [{ ...assets().files[0], category: 'unknown' }] },
      { ...assets(), files: [{ ...assets().files[0], revalidations: 4 }] },
      { ...assets(), files: [assets().files[0], assets().files[0]] },
    ]) expect(() => parseAssetTraffic(invalid, 1)).toThrow()
  })
})
