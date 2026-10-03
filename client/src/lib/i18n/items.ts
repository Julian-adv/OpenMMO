import { get } from 'svelte/store'
import { locale, type Locale } from './locale'
import { land_claim_min_level } from '../wasm/onlinerpg_shared'
import ko from './locales/ko.items.json'
import ja from './locales/ja.items.json'
import zhHans from './locales/zh-Hans.items.json'
import zhHant from './locales/zh-Hant.items.json'

const translations: Record<Locale, Record<string, string>> = {
  en: {},
  ko,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
}

export function itemText(
  id: string,
  field: 'name' | 'description',
  fallback: string,
  language: Locale = get(locale)
): string {
  const text = translations[language][`${id}.${field}`] || fallback
  return id === 'land_deed' && field === 'description'
    ? text.replaceAll('{{landClaimMinLevel}}', String(land_claim_min_level()))
    : text
}
