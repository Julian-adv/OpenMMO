import { describe, expect, it } from 'vitest'
import { modularOutfitForArmor } from './modularEquipment'
import { DEFAULT_MODULAR_OUTFIT } from './modularCharacter'

describe('modularOutfitForArmor', () => {
  it('dresses a man in default clothes for a female-only outfit', () => {
    expect(
      modularOutfitForArmor({
        chest: 'shopkeeper_top',
        pants: 'shopkeeper_pants',
        boots: 'shopkeeper_boots',
      })
    ).toEqual({ ...DEFAULT_MODULAR_OUTFIT, pants: 'cloth' })
  })
})
