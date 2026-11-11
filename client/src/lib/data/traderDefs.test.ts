import { describe, expect, it } from 'vitest'
import { getNpcCapabilities } from './traderDefs'
import { exchangePrice, getMerchantByNpcName } from './merchantDefs'

describe('NPC click actions', () => {
  it('opens Ysolde’s shard exchange instead of a gold shop', () => {
    const merchant = getMerchantByNpcName('Ysolde')!
    expect(getNpcCapabilities('Ysolde').defaultAction).toBe('trade')
    expect(merchant.currencyItem).toBe('terraclash_token')
    expect(merchant.sellRatePercent).toBe(0)
    expect(exchangePrice(merchant, 'earth_shield')).toBe(30)
    expect(exchangePrice(merchant, 'iron_sword')).toBeUndefined()
  })
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
    expect(getMerchantByNpcName('Aldwin')?.catalog).toBe(
      'land_deed;notice_paper;letter;revenue_stamp;family_chest;family_board'
    )
  })
})
