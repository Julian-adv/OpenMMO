import { get } from 'svelte/store'
import { locale, type Locale } from './locale'
import ko from './locales/ko.places.json'
import ja from './locales/ja.places.json'
import zhHans from './locales/zh-Hans.places.json'
import zhHant from './locales/zh-Hant.places.json'

const translations: Record<Locale, Record<string, string>> = {
  en: {},
  ko,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
}

export function placeName(
  id: string,
  fallback: string,
  language: Locale = get(locale)
): string {
  return translations[language][id] || fallback
}
