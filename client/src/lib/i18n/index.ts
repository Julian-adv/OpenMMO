import { createInstance } from 'i18next'
import { derived, get } from 'svelte/store'
import en from './locales/en.json'
import ko from './locales/ko.json'
import ja from './locales/ja.json'
import zhHans from './locales/zh-Hans.json'
import { locale, type Locale } from './locale'
import { resolveKoreanParticles } from './koreanParticles'

export { languages, languagePreference, locale } from './locale'
export type { Locale, LanguagePreference } from './locale'
export type MessageKey = keyof typeof en
export type MessageValues = Record<string, string | number>

const i18n = createInstance()
void i18n.init({
  initAsync: false,
  lng: 'en',
  fallbackLng: 'en',
  keySeparator: false,
  returnEmptyString: false,
  interpolation: { escapeValue: false },
  resources: {
    en: { translation: en },
    ko: { translation: ko },
    ja: { translation: ja },
    'zh-Hans': { translation: zhHans },
  },
})

export function translate(
  key: MessageKey,
  values: MessageValues = {},
  language: Locale = get(locale)
): string {
  return renderMessage(key, values, language)
}

function renderMessage(
  key: string,
  values: MessageValues,
  language: Locale,
  fallback?: string
): string {
  const message = i18n.t(key, {
    ...values,
    lng: language,
    defaultValue: fallback,
  })
  return language === 'ko' && i18n.exists(key, { lng: language })
    ? resolveKoreanParticles(message)
    : message
}

export const t = derived(
  locale,
  (language) => (key: MessageKey, values?: MessageValues) =>
    translate(key, values, language)
)

export interface LocalizedMessage {
  code: string
  params: MessageValues
}

export function translateServerMessage(
  data: {
    message: string
    localization?: LocalizedMessage | null
  },
  language: Locale = get(locale)
): string {
  if (!data.localization) return data.message
  return renderMessage(
    data.localization.code,
    data.localization.params,
    language,
    data.message
  )
}

locale.subscribe((language) => {
  if (typeof document !== 'undefined') document.documentElement.lang = language
})
