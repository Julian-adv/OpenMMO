import debuffsJson from '../../../../data/debuffs.json'
import { translate, type Locale, type MessageKey } from '../i18n'

export interface DebuffPresentation {
  label: string
  icon: string
  note: string
  applied: string
  expired: string
}

export interface PresentedDebuff extends DebuffPresentation {
  id: string
  /** Localized remaining time. */
  remaining: string
}

const DEFS: Record<
  string,
  {
    name?: string
    durationSecs?: number
    armorWeightMult?: number
    staggerM?: number
  }
> = debuffsJson

const PRESENTATION: Record<
  string,
  {
    labelKey: MessageKey
    noteKey: MessageKey
    icon: string
    applied: string
    expired: string
  }
> = {
  food_poisoning: {
    labelKey: 'debuff.food_poisoning.name',
    noteKey: 'debuff.food_poisoning.note',
    icon: '☠️',
    applied: 'Your stomach churns — food poisoning! Cooked food next time.',
    expired: 'The sickness passes. You feel yourself again.',
  },
  bleed: {
    labelKey: 'debuff.bleed.name',
    noteKey: 'debuff.bleed.note',
    icon: '🩸',
    applied: 'You are bleeding!',
    expired: 'The bleeding stops.',
  },
  wet: {
    labelKey: 'debuff.wet.name',
    noteKey: 'debuff.wet.note',
    icon: '💧',
    applied: 'You are soaked through — heavy going until you dry off.',
    expired: 'Your clothes are dry again.',
  },
  tipsy: {
    labelKey: 'debuff.tipsy.name',
    noteKey: 'debuff.tipsy.note',
    icon: '🍺',
    applied: 'A warm glow spreads through you. Just the one, mind.',
    expired: 'The glow fades.',
  },
  drunk: {
    labelKey: 'debuff.drunk.name',
    noteKey: 'debuff.drunk.note',
    icon: '🍻',
    applied: 'The room tilts a little. Maybe that was one too many.',
    expired: 'Your head clears.',
  },
  wasted: {
    labelKey: 'debuff.wasted.name',
    noteKey: 'debuff.wasted.note',
    icon: '🥴',
    applied: 'The floor keeps moving. Walking is going to be an adventure.',
    expired: 'You sober up, more or less.',
  },
}

export function debuffPresentation(
  id: string,
  language?: Locale
): DebuffPresentation {
  const presentation = PRESENTATION[id]
  const label = presentation
    ? translate(presentation.labelKey, {}, language)
    : (DEFS[id]?.name ?? id)
  return {
    label,
    icon: presentation?.icon ?? '⚠️',
    note: presentation ? translate(presentation.noteKey, {}, language) : '',
    applied: presentation?.applied ?? `You are afflicted: ${label}.`,
    expired: presentation?.expired ?? `${label} wears off.`,
  }
}

/** How far off a click a walker under this debuff may land (doc/DEBUFF.md). */
export function debuffStaggerM(id: string) {
  return DEFS[id]?.staggerM ?? 0
}

/** A debuff's full duration in ms, for effects that fade with what's left. */
export function debuffDurationMs(id: string) {
  return (DEFS[id]?.durationSecs ?? 0) * 1_000
}

/** Combined `armor` weight factor of the debuffs currently up (doc/DEBUFF.md). */
export function armorWeightMult(ids: string[]) {
  return ids.reduce((mult, id) => mult * (DEFS[id]?.armorWeightMult ?? 1), 1)
}

export function formatRemaining(ms: number, language?: Locale) {
  const seconds = Math.max(0, Math.ceil(ms / 1_000))
  return seconds >= 60
    ? translate(
        'duration.minutes',
        { count: Math.ceil(seconds / 60) },
        language
      )
    : translate('duration.seconds', { count: seconds }, language)
}
