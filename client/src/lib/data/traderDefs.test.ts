import { describe, expect, it } from 'vitest'
import { getNpcCapabilities } from './traderDefs'
import { getMerchantByNpcName } from './merchantDefs'

describe('NPC click actions', () => {
  it('opens Grida’s shop and keeps talking available', () => {
    expect(getNpcCapabilities('Grida')).toEqual({
      talk: true,
      trade: true,
      defaultAction: 'trade',
      traderId: 'grida',
    })
  })

  it('keeps merchant and resident click defaults', () => {
    expect(getNpcCapabilities('Rica').defaultAction).toBe('trade')
    expect(getNpcCapabilities('Karl').defaultAction).toBe('talk')
  })

  it('buys player items at the same rate as Rica', () => {
    expect(getMerchantByNpcName('Grida')?.sellRatePercent).toBe(40)
    expect(getMerchantByNpcName('Grida')?.sellRatePercent).toBe(
      getMerchantByNpcName('Rica')?.sellRatePercent
    )
  })

  it('keeps Aldwin focused on land deeds', () => {
    expect(getMerchantByNpcName('Aldwin')?.catalog).toBe('land_deed')
  })
})
