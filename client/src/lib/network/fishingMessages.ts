import { get } from 'svelte/store'
import { locale, translate } from '../i18n'
import { itemText } from '../i18n/items'

export interface CatchDefLike {
  name: string
  category?: string
}

export function catchMessage(
  def: CatchDefLike | undefined,
  fallbackId: string,
  sizeCm: number,
  trophy: boolean
): string {
  const language = get(locale)
  const name = itemText(fallbackId, 'name', def?.name ?? fallbackId, language)
  const an = /^[aeiou]/i.test(name) ? 'an' : 'a'
  if (trophy) return translate('fishing.trophy', { name, size: sizeCm })
  const values = {
    name: language === 'en' ? `${an} ${name}` : name,
    size: sizeCm,
  }
  if (def?.category === 'coin_catch') return translate('fishing.coin', values)
  if (def?.category === 'fish') return translate('fishing.fish', values)
  return translate('fishing.junk', values)
}
