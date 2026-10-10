import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  ANNOUNCEMENT_DURATION_MS,
  announcements,
  clearAnnouncements,
  dismissAnnouncement,
  showAnnouncement,
} from './announcementStore'

beforeEach(() => {
  vi.useFakeTimers()
  clearAnnouncements()
})

afterEach(() => {
  clearAnnouncements()
  vi.useRealTimers()
})

it('gives successive announcements their own full display duration', () => {
  showAnnouncement('전부 재접속 부탁드립니다')
  vi.advanceTimersByTime(2000)
  showAnnouncement('Maintenance soon')
  vi.advanceTimersByTime(ANNOUNCEMENT_DURATION_MS - 2001)
  expect(get(announcements)).toHaveLength(2)
  vi.advanceTimersByTime(1)
  expect(get(announcements).map((entry) => entry.text)).toEqual([
    'Maintenance soon',
  ])
  vi.advanceTimersByTime(2000)
  expect(get(announcements)).toEqual([])
  expect(vi.getTimerCount()).toBe(0)
})

it('dismisses a toast without changing the lifetime of other announcements', () => {
  showAnnouncement('First')
  showAnnouncement('Second')
  dismissAnnouncement(get(announcements)[0].id)
  expect(get(announcements).map((entry) => entry.text)).toEqual(['Second'])
  expect(vi.getTimerCount()).toBe(1)
})

it('bounds visible announcements and removes timers for evicted entries', () => {
  for (let i = 0; i < 10; i++) showAnnouncement(String(i))
  expect(get(announcements).map((entry) => entry.text)).toEqual(['7', '8', '9'])
  expect(vi.getTimerCount()).toBe(3)
})

it('clears announcements and timers when leaving the game', () => {
  showAnnouncement('Old session')
  clearAnnouncements()
  expect(get(announcements)).toEqual([])
  expect(vi.getTimerCount()).toBe(0)
  showAnnouncement('New session')
  vi.advanceTimersByTime(ANNOUNCEMENT_DURATION_MS)
  expect(get(announcements)).toEqual([])
})
