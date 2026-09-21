import type { HungerBand, HungerSnapshot } from '../stores/hungerStore'
import type { MessageKey } from '../i18n'

export const HUNGER_BAND_INFO = {
  Normal: {
    labelKey: 'hunger.normal',
    icon: '🍞',
    descriptionKey: 'hunger.normalDescription',
    noteKey: undefined,
  },
  Hungry: {
    labelKey: 'hunger.hungry',
    icon: '🍽️',
    descriptionKey: 'hunger.hungryDescription',
    noteKey: 'hunger.hungryNote',
  },
  Weak: {
    labelKey: 'hunger.weak',
    icon: '🦴',
    descriptionKey: 'hunger.weakDescription',
    noteKey: 'hunger.weakNote',
  },
} satisfies Record<
  HungerBand,
  {
    labelKey: MessageKey
    icon: string
    descriptionKey: MessageKey
    noteKey: MessageKey | undefined
  }
>

export function hungerModifiers(hunger: HungerSnapshot) {
  return [
    { labelKey: 'hunger.movement', mult: hunger.moveMult },
    { labelKey: 'hunger.attack', mult: hunger.attackMult },
    { labelKey: 'hunger.carry', mult: hunger.carryMult },
  ] satisfies { labelKey: MessageKey; mult: number }[]
}

export function formatHungerModifier(multiplier: number) {
  return `${multiplier > 1 ? '+' : ''}${Math.round((multiplier - 1) * 100)}%`
}
