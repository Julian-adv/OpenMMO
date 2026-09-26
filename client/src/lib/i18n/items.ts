import { get } from 'svelte/store'
import { locale, type Locale } from './locale'
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
  return translations[language][`${id}.${field}`] || fallback
}
