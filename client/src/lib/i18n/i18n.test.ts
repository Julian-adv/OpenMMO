import { afterEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { browserLocale } from './locale'
import {
  languagePreference,
  locale,
  t,
  translate,
  translateServerMessage,
} from './index'
import { itemText } from './items'
import { titleName, titleNameNow } from '../data/titleDefs'
import { itemDescription, itemDisplayName, getItemDef } from '../data/itemDefs'
import { catchMessage } from '../network/fishingMessages'
import { attackLog } from '../network/combatLog'
import { debuffPresentation, formatRemaining } from '../data/debuffPresentation'
import {
  activeDebuffs,
  resetDebuffStore,
  visibleDebuffs,
} from '../stores/debuffStore'
import en from './locales/en.json'
import ko from './locales/ko.json'
import ja from './locales/ja.json'
import zhHans from './locales/zh-Hans.json'
import zhHant from './locales/zh-Hant.json'
import koItems from './locales/ko.items.json'
import jaItems from './locales/ja.items.json'
import zhItems from './locales/zh-Hans.items.json'
import zhHantItems from './locales/zh-Hant.items.json'
import items from '../../../../data/items.json'

afterEach(() => {
  resetDebuffStore()
  languagePreference.set('en')
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('language selection', () => {
  it.each([
    [['ko-KR'], 'ko'],
    [['ja-JP'], 'ja'],
    [['zh-CN'], 'zh-Hans'],
    [['zh-SG'], 'zh-Hans'],
    [['zh-Hans'], 'zh-Hans'],
    [['zh'], 'zh-Hans'],
    [['zh-TW', 'ja'], 'zh-Hant'],
    [['zh-Hant'], 'zh-Hant'],
    [['zh-Hant-TW'], 'zh-Hant'],
    [['zh-HK'], 'zh-Hant'],
    [['zh_MO'], 'zh-Hant'],
    [['fr-FR', 'ko'], 'ko'],
    [[], 'en'],
  ])('resolves %j to %s', (preferences, expected) => {
    expect(browserLocale(preferences)).toBe(expected)
  })

  it('uses an explicit choice ahead of browser preferences and updates subscribers', () => {
    vi.stubGlobal('navigator', { languages: ['ko-KR'] })
    languagePreference.set('auto')
    expect(get(locale)).toBe('ko')
    const labels: string[] = []
    const unsubscribe = t.subscribe((format) =>
      labels.push(format('settings.title'))
    )
    languagePreference.set('ja')
    languagePreference.set('en')
    unsubscribe()
    expect(labels).toEqual(['설정', '設定', 'Settings'])
  })

  it('persists a choice and changes the document language without navigation', () => {
    const setItem = vi.fn()
    const root = { lang: '' }
    vi.stubGlobal('localStorage', { setItem })
    vi.stubGlobal('document', { documentElement: root })
    languagePreference.set('zh-Hans')
    expect(setItem).toHaveBeenCalledWith('onlinerpg_language', 'zh-Hans')
    expect(root.lang).toBe('zh-Hans')
    expect(get(t)('settings.title')).toBe('设置')
  })

  it('still switches language when browser storage is blocked', () => {
    vi.stubGlobal('localStorage', {
      setItem: () => {
        throw new Error('blocked')
      },
    })
    expect(() => languagePreference.set('ja')).not.toThrow()
    expect(get(t)('settings.title')).toBe('設定')
  })
})

describe('catalog integrity', () => {
  const placeholders = (value: string) =>
    [...value.matchAll(/{{\s*(\w+)\s*}}/g)].map((match) => match[1]).sort()

  it.each([
    ['ko', ko],
    ['ja', ja],
    ['zh-Hans', zhHans],
    ['zh-Hant', zhHant],
  ] as const)(
    '%s has every message and preserves its variables',
    (_language, messages) => {
      expect(Object.keys(messages).sort()).toEqual(Object.keys(en).sort())
      for (const key of Object.keys(en) as (keyof typeof en)[]) {
        expect(messages[key].trim(), key).not.toBe('')
        expect(placeholders(messages[key]), key).toEqual(placeholders(en[key]))
      }
    }
  )

  it.each([
    ['ko', koItems],
    ['ja', jaItems],
    ['zh-Hans', zhItems],
    ['zh-Hant', zhHantItems],
  ] as const)(
    '%s covers every current item without changing the game data',
    (language, messages) => {
      const expectedKeys = Object.keys(items)
        .flatMap((id) => [`${id}.name`, `${id}.description`])
        .sort()
      expect(Object.keys(messages).sort()).toEqual(expectedKeys)
      for (const def of Object.values(items)) {
        expect(itemText(def.id, 'name', '', language), def.id).not.toBe('')
        expect(itemText(def.id, 'description', '', language), def.id).not.toBe(
          ''
        )
      }
      languagePreference.set(language)
      expect(getItemDef('iron_sword')?.name).toBe('Iron Sword')
      expect(itemDisplayName('iron_sword', 7)).toBe(
        `+7 ${messages['iron_sword.name']}`
      )
    }
  )
})

describe('message rendering', () => {
  it('updates character titles and new chat messages with the game language', () => {
    languagePreference.set('en')
    const labels: string[] = []
    const unsubscribe = titleName.subscribe((name) =>
      labels.push(name('goblin_slayer'))
    )
    try {
      for (const language of [
        'ko',
        'ja',
        'zh-Hans',
        'zh-Hant',
        'en',
      ] as const) {
        languagePreference.set(language)
        expect(titleNameNow('goblin_slayer')).toBe(labels.at(-1))
        expect(titleNameNow('future_title')).toBe('future_title')
      }
      expect(labels).toEqual([
        'Slayer of the Goblin Chief',
        '고블린 족장을 쓰러뜨린 자',
        'ゴブリンの族長を討ちし者',
        '哥布林酋长讨伐者',
        '哥布林酋長討伐者',
        'Slayer of the Goblin Chief',
      ])
    } finally {
      unsubscribe()
    }
  })

  it.each([
    ['검', '검을'],
    ['도끼', '도끼를'],
    ['수호의 결계', '수호의 결계를'],
    ['정조준', '정조준을'],
  ])(
    'selects particles after interpolating %s in client and server messages',
    (name, expected) => {
      languagePreference.set('ko')
      expect(get(t)('skillFailure.cooldown', { name })).toBe(
        `${expected} 아직 사용할 수 없습니다.`
      )
      expect(
        translateServerMessage({
          message: `Cannot use ${name}.`,
          localization: { code: 'skillFailure.requirements', params: { name } },
        })
      ).toBe(`${expected} 사용할 수 없습니다.`)
    }
  )

  it('selects particles after all item, quantity, and XP values are interpolated', () => {
    languagePreference.set('ko')
    expect(
      translate('system.pickedUpItem', { name: '미루', item: '검', amount: '' })
    ).toBe('미루: 검을 주웠습니다.')
    expect(
      translate('system.pickedUpItem', {
        name: '미루',
        item: '검',
        amount: ' x2',
      })
    ).toBe('미루: 검 x2를 주웠습니다.')
    expect(translate('combat.xpGained', { amount: 10 })).toBe(
      '경험치 10을 얻었습니다.'
    )
    expect(translate('combat.xpLost', { amount: 12 })).toBe(
      '사망 페널티: 경험치 12를 잃었습니다.'
    )
  })

  it('leaves other languages and unknown server messages untouched', () => {
    expect(translate('skillFailure.cooldown', { name: '검$을' }, 'en')).toBe(
      '검$을 is not ready yet.'
    )
    const message = '검$을 {{name}}'
    expect(translateServerMessage({ message }, 'ko')).toBe(message)
    expect(
      translateServerMessage(
        { message, localization: { code: 'future.message', params: {} } },
        'ko'
      )
    ).toBe(message)
  })

  it('updates active status effects on language changes without resetting their duration', () => {
    vi.useFakeTimers()
    vi.setSystemTime(1_000)
    languagePreference.set('en')
    activeDebuffs.set([{ id: 'food_poisoning', until: 62_000 }])
    const unsubscribe = visibleDebuffs.subscribe(() => {})
    try {
      expect(get(visibleDebuffs)[0]).toMatchObject({
        label: 'Food Poisoning',
        remaining: '2m',
      })
      languagePreference.set('ko')
      expect(get(visibleDebuffs)[0]).toMatchObject({
        label: '식중독',
        note: '능력치 크게 감소',
        remaining: '2분',
      })
      vi.advanceTimersByTime(1_000)
      expect(get(visibleDebuffs)[0].remaining).toBe('1분')
      languagePreference.set('ja')
      expect(get(visibleDebuffs)[0].label).toBe('食中毒')
      languagePreference.set('zh-Hans')
      expect(get(visibleDebuffs)[0]).toMatchObject({
        label: '食物中毒',
        remaining: '1分钟',
      })
      languagePreference.set('zh-Hant')
      expect(get(visibleDebuffs)[0].remaining).toBe('1分鐘')
      expect(get(activeDebuffs)).toEqual([
        { id: 'food_poisoning', until: 62_000 },
      ])
      vi.advanceTimersByTime(60_000)
      expect(get(visibleDebuffs)).toEqual([])
    } finally {
      unsubscribe()
    }
    expect(vi.getTimerCount()).toBe(0)
  })

  it('preserves unknown status effects and rounds localized countdowns up', () => {
    expect(debuffPresentation('future_effect', 'ko')).toMatchObject({
      label: 'future_effect',
      icon: '⚠️',
      note: '',
    })
    expect(formatRemaining(-1, 'ko')).toBe('0초')
    expect(formatRemaining(1, 'ko')).toBe('1초')
    expect(formatRemaining(59_000, 'ja')).toBe('59秒')
    expect(formatRemaining(60_000, 'zh-Hans')).toBe('1分钟')
    expect(formatRemaining(60_000, 'zh-Hant')).toBe('1分鐘')
    expect(formatRemaining(60_001, 'en')).toBe('2m')
  })

  it('falls back to the supplied English content for new items and message codes', () => {
    languagePreference.set('ko')
    expect(itemText('future_item', 'name', 'Future Item')).toBe('Future Item')
    expect(itemDisplayName('unknown_item')).toBe('unknown_item')
    expect(
      translateServerMessage({
        message: 'New server message',
        localization: { code: 'server.future', params: {} },
      })
    ).toBe('New server message')
    expect(translateServerMessage({ message: 'Legacy message' })).toBe(
      'Legacy message'
    )
  })

  it('renders structured server variables and preserves names as text', () => {
    languagePreference.set('ko')
    expect(
      translateServerMessage({
        message: 'You picked up 12 copper.',
        localization: { code: 'server.goldPickedUp', params: { amount: '12' } },
      })
    ).toBe('동화 12개를 주웠습니다.')
    expect(translate('system.playerRevived', { name: '<b>A&B</b>' })).toBe(
      '<b>A&B</b> 님이 부활했습니다.'
    )
  })

  it('translates item tooltips and newly received gameplay logs', () => {
    languagePreference.set('ko')
    expect(itemDescription(getItemDef('iron_sword')!)).toBe('튼튼한 철제 장검.')
    expect(catchMessage(getItemDef('raw_trout'), 'raw_trout', 34, false)).toBe(
      '생송어 (34 cm) 낚시에 성공했습니다.'
    )
    expect(attackLog(20, true, 9)).toBe('주사위 20: 명중! 피해 9!')
    languagePreference.set('en')
    expect(catchMessage(getItemDef('raw_trout'), 'raw_trout', 34, false)).toBe(
      'You caught a Raw Trout (34 cm).'
    )
  })
})
