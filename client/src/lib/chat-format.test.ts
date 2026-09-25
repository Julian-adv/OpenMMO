import { describe, it, expect } from 'vitest'
import {
  isPartyTabLine,
  unreadChatCount,
  unreadPartyCount,
  whisperChatEntry,
  chatEntryName,
  chatEntryText,
  shouldTranslateChatEntry,
} from './chat-format'
import type { ChatSender } from './stores/gameStore'

describe('automatic chat translation', () => {
  it('translates player chat and legacy server prose while leaving localized notices alone', () => {
    expect(shouldTranslateChatEntry({ sender: 'remote', text: 'Hello' })).toBe(
      true
    )
    expect(
      shouldTranslateChatEntry({ sender: 'system', text: '/help — 도움말' })
    ).toBe(false)
    const legacy = {
      sender: 'system' as const,
      text: 'New server notice',
      autoTranslate: true,
    }
    expect(shouldTranslateChatEntry(legacy)).toBe(true)
    expect(
      shouldTranslateChatEntry({
        ...legacy,
        localization: { code: 'server.partyNotInParty', params: {} },
      })
    ).toBe(false)
  })
})

describe('whisperChatEntry', () => {
  it('labels the echo of an own whisper as outgoing', () => {
    expect(whisperChatEntry('Miru', 'Rica', 'psst', 'Miru')).toEqual({
      text: 'psst',
      sender: 'whisper',
      name: 'Rica',
      whisperDirection: 'outgoing',
    })
  })

  it('labels a received whisper with the sender', () => {
    expect(whisperChatEntry('Miru', 'Rica', 'psst', 'Rica')).toEqual({
      text: 'psst',
      sender: 'whisper',
      name: 'Miru',
      whisperDirection: 'incoming',
    })
  })

  it('treats an unknown own name as incoming', () => {
    expect(
      whisperChatEntry('Miru', 'Rica', 'psst', undefined).whisperDirection
    ).toBe('incoming')
  })

  it('localizes direction without changing names, message text, or unread counts', () => {
    const outgoing = {
      ...whisperChatEntry('Miru', 'To <b>Rica</b>', 'hello', 'Miru'),
      id: 1,
    }
    const incoming = {
      ...whisperChatEntry('To <b>Rica</b>', 'Miru', 'hello', 'Miru'),
      id: 2,
    }
    expect(chatEntryName(outgoing, 'en')).toBe('To To <b>Rica</b>')
    expect(chatEntryName(outgoing, 'ko')).toBe('To <b>Rica</b>에게')
    expect(chatEntryName(incoming, 'ja')).toBe('To <b>Rica</b>から')
    expect(chatEntryName(incoming, 'zh-Hans')).toBe('来自To <b>Rica</b>')
    expect(chatEntryText(incoming, 'ko')).toBe('hello')
    expect(unreadChatCount([outgoing, incoming], 0, 'Miru')).toBe(1)
  })
})

describe('unreadPartyCount', () => {
  const TRANSCRIPT = [
    { sender: 'party' as const, name: 'Rica', id: 1 },
    { sender: 'local' as const, name: 'Miru', id: 2 },
    { sender: 'party' as const, name: 'Rica', id: 3 },
    { sender: 'system' as const, id: 4 },
    { sender: 'party' as const, name: 'Rica', id: 5 },
  ]

  it('counts only the party lines newer than the last one seen', () => {
    expect(unreadPartyCount(TRANSCRIPT, 0, 'Miru')).toBe(3)
    expect(unreadPartyCount(TRANSCRIPT, 3, 'Miru')).toBe(1)
    expect(unreadPartyCount(TRANSCRIPT, 5, 'Miru')).toBe(0)
  })

  it('never counts the echo of an own party line', () => {
    expect(unreadPartyCount(TRANSCRIPT, 0, 'Rica')).toBe(0)
  })

  it('counts everything when the own name is not known yet', () => {
    expect(unreadPartyCount(TRANSCRIPT, 0, undefined)).toBe(3)
  })

  it('survives the ring buffer dropping the entries it counted', () => {
    // Ids only grow, so evicting 1 and 3 leaves the unread 5 counted once.
    expect(unreadPartyCount(TRANSCRIPT.slice(3), 4, 'Miru')).toBe(1)
    expect(unreadPartyCount([], 4, 'Miru')).toBe(0)
  })
})

describe('unreadChatCount', () => {
  const TRANSCRIPT = [
    { sender: 'remote' as const, name: 'Rica', id: 1 },
    { sender: 'local' as const, name: 'Miru', id: 2 },
    { sender: 'party' as const, name: 'Miru', id: 3 },
    {
      sender: 'whisper' as const,
      name: 'Rica',
      whisperDirection: 'outgoing' as const,
      id: 4,
    },
    {
      sender: 'whisper' as const,
      name: 'Rica',
      whisperDirection: 'incoming' as const,
      id: 5,
    },
    { sender: 'system' as const, id: 6 },
  ]

  it('counts incoming and system lines newer than the collapsed baseline', () => {
    expect(unreadChatCount(TRANSCRIPT, 0, 'Miru')).toBe(3)
    expect(unreadChatCount(TRANSCRIPT, 5, 'Miru')).toBe(1)
    expect(unreadChatCount(TRANSCRIPT, 6, 'Miru')).toBe(0)
  })

  it('does not count messages sent by the current player', () => {
    expect(unreadChatCount(TRANSCRIPT.slice(1, 4), 0, 'Miru')).toBe(0)
  })
})

describe('isPartyTabLine', () => {
  const line = (sender: ChatSender, text: string) => ({ sender, text })

  it('takes the party channel and its system notices', () => {
    expect(isPartyTabLine(line('party', 'on my way'))).toBe(true)
    expect(isPartyTabLine(line('system', 'Party: Rica joined.'))).toBe(true)
    expect(isPartyTabLine(line('system', 'Summon: Rica calls you.'))).toBe(true)
  })

  it('keeps localized notices in the party tab across language changes', () => {
    const entry = {
      sender: 'system' as const,
      text: 'Party: Rica was removed.',
      localization: { code: 'server.partyRemoved', params: { name: 'Rica' } },
    }
    expect(chatEntryText(entry, 'en')).toBe(entry.text)
    expect(chatEntryText(entry, 'ko')).toBe('파티: Rica 님이 추방되었습니다.')
    expect(isPartyTabLine(entry)).toBe(true)
    expect(isPartyTabLine({ ...entry, text: chatEntryText(entry, 'ja') })).toBe(
      true
    )
    expect(entry.text).toBe('Party: Rica was removed.')
    expect(
      chatEntryText(
        { ...entry, localization: { code: 'future.message', params: {} } },
        'ko'
      )
    ).toBe(entry.text)
  })

  it('leaves every other line to the All tab', () => {
    expect(isPartyTabLine(line('local', 'Party: hi'))).toBe(false)
    expect(isPartyTabLine(line('whisper', 'psst'))).toBe(false)
    expect(isPartyTabLine(line('system', 'Muted: 10 minutes left.'))).toBe(
      false
    )
  })
})
