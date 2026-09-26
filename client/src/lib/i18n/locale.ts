import { derived } from 'svelte/store'
import { persistedString } from '../stores/persisted'

export const languages = [
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
  { value: 'ja', label: '日本語' },
  { value: 'zh-Hans', label: '简体中文' },
  { value: 'zh-Hant', label: '繁體中文' },
] as const

export type Locale = (typeof languages)[number]['value']
export type LanguagePreference = Locale | 'auto'

export function isLanguagePreference(
  value: string
): value is LanguagePreference {
  return (
    value === 'auto' || languages.some((language) => language.value === value)
  )
}

export function browserLocale(preferences: readonly string[]): Locale {
  for (const preference of preferences) {
    const tag = preference.toLowerCase().replaceAll('_', '-')
    const language = tag.split('-')[0]
    if (language === 'en' || language === 'ko' || language === 'ja')
      return language
    if (language === 'zh')
      return /-(hant|tw|hk|mo)(-|$)/.test(tag) ? 'zh-Hant' : 'zh-Hans'
  }
  return 'en'
}

export const languagePreference = persistedString<LanguagePreference>(
  'onlinerpg_language',
  'auto',
  isLanguagePreference
)

export const locale = derived(
  languagePreference,
  (preference): Locale =>
    preference === 'auto'
      ? browserLocale(
          typeof navigator === 'undefined'
            ? []
            : (navigator.languages ?? [navigator.language])
        )
      : preference
)
