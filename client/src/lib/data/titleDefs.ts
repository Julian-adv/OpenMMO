import { derived, get } from 'svelte/store'
import titlesJson from '../../../../data/titles.json'
import { persistedString } from '../stores/persisted'
import {
  languages,
  locale,
  translate,
  type Locale,
  type MessageKey,
} from '../i18n'
import { isLanguagePreference } from '../i18n/locale'

interface TitleDef {
  id: string
  name: string
  nameKo: string
  order: number
}

const defs = titlesJson as Record<string, TitleDef>

export type TitleLanguage = 'auto' | Locale
export const TITLE_LANGUAGES: { value: TitleLanguage; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  ...languages,
]

export const titleLanguage = persistedString<TitleLanguage>(
  'onlinerpg_titleLanguage',
  'auto',
  isLanguagePreference
)

function nameIn(id: string, language: Locale): string {
  const def = defs[id]
  if (!def) return id
  return translate(
    `title.${id}` as MessageKey,
    { defaultValue: language === 'ko' ? def.nameKo || def.name : def.name },
    language
  )
}

/** Reactive lookup: `$titleName(id)` re-renders when the setting changes. */
export const titleName = derived(
  [titleLanguage, locale],
  ([preference, language]) =>
    (id: string) =>
      nameIn(id, preference === 'auto' ? language : preference)
)

/** Non-reactive lookup for one-off text (chat lines). */
export function titleNameNow(id: string): string {
  return get(titleName)(id)
}
