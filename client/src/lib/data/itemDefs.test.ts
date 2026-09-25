import { describe, expect, it } from 'vitest'
import itemDefs, {
  armorTypeLabel,
  effectiveGuard,
  isRangedWeapon,
  isTwoHanded,
  weaponRangeMeters,
  WEAPON_TYPE_LABELS,
  weaponTypeLabel,
  type WeaponType,
} from './itemDefs'
import { PLAYER_ATTACK_RANGE_METERS } from './combatTiming'

describe('weapon types', () => {
  it.each(Object.entries(WEAPON_TYPE_LABELS))(
    'labels %s as %s',
    (weaponType, label) => {
      expect(weaponTypeLabel(weaponType as WeaponType)).toBe(label)
    }
  )

  it('classifies every weapon and only weapons', () => {
    for (const def of Object.values(itemDefs)) {
      expect(def.weaponType !== undefined, def.id).toBe(
        def.category === 'weapon'
      )
      if (def.weaponType) {
        expect(WEAPON_TYPE_LABELS[def.weaponType], def.id).toBeDefined()
      }
    }
  })

  it('groups short swords with swords while keeping daggers separate', () => {
    expect(itemDefs.iron_sword.weaponType).toBe('sword')
    expect(itemDefs.steel_longsword.weaponType).toBe('sword')
    expect(itemDefs.goblin_sword.weaponType).toBe('sword')
    expect(itemDefs.small_sword.weaponType).toBe('sword')
    expect(itemDefs.dagger.weaponType).toBe('dagger')
  })
})

describe('shield armor type', () => {
  it.each([
    ['wooden_shield', 1],
    ['raven_shield', 2],
  ] as const)(
    'classifies %s as a shield without changing its guard',
    (id, guard) => {
      const def = itemDefs[id]
      expect(def.armorType).toBe('shield')
      expect(armorTypeLabel(def.armorType!)).toBe('Shield')
      expect(def.category).toBe('armor')
      expect(def.equipSlot).toBe('off_hand')
      expect(def.weaponType).toBeUndefined()
      expect(effectiveGuard(def, 9)).toBe(guard + 9)
    }
  )

  it.each(['torch', 'worn_torch', 'iron_helmet', 'ring_of_protection'])(
    'does not classify %s as a shield',
    (id) => {
      expect(itemDefs[id].armorType).toBeUndefined()
    }
  )
})

describe('weapon reach from items.json', () => {
  it('classifies the great sword as two-handed melee', () => {
    expect(isTwoHanded('great_sword')).toBe(true)
    expect(isRangedWeapon('great_sword')).toBe(false)
    expect(itemDefs.great_sword.weaponType).toBe('great_sword')
    expect(weaponRangeMeters('great_sword')).toBe(PLAYER_ATTACK_RANGE_METERS)
  })
  it('reads a ranged weapon its declared range', () => {
    expect(weaponRangeMeters('bow')).toBe(10)
    expect(isRangedWeapon('bow')).toBe(true)
    expect(isTwoHanded('bow')).toBe(true)
  })

  it('leaves a weapon with no range at the melee reach', () => {
    expect(weaponRangeMeters('iron_sword')).toBe(PLAYER_ATTACK_RANGE_METERS)
    expect(isRangedWeapon('iron_sword')).toBe(false)
    expect(isTwoHanded('iron_sword')).toBe(false)
  })

  it('falls back to melee for an empty hand or an unknown item', () => {
    expect(weaponRangeMeters(null)).toBe(PLAYER_ATTACK_RANGE_METERS)
    expect(weaponRangeMeters('no_such_item')).toBe(PLAYER_ATTACK_RANGE_METERS)
  })
})
