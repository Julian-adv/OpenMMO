import { get } from 'svelte/store'
import { locale, type Locale } from './locale'
import ko from './locales/ko.places.json'
import ja from './locales/ja.places.json'
import zhHans from './locales/zh-Hans.places.json'

const translations: Partial<Record<Locale, Record<string, string>>> = {
  ko,
  ja,
  'zh-Hans': zhHans,
}

export function placeName(
  id: string,
  fallback: string,
  language: Locale = get(locale)
): string {
  return translations[language]?.[id] || fallback
}
