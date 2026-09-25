import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  SKILL_FAILURE_DURATION_MS,
  clearSkillFailure,
  showSkillFailure,
  skillFailure,
} from './skillFailureStore'

beforeEach(() => {
  vi.useFakeTimers()
  clearSkillFailure()
})

afterEach(() => {
  clearSkillFailure()
  vi.useRealTimers()
})

it('shows the failure for the full duration and then removes it', () => {
  showSkillFailure('Target is too far away.')
  vi.advanceTimersByTime(SKILL_FAILURE_DURATION_MS - 1)
  expect(get(skillFailure)?.text).toBe('Target is too far away.')
  vi.advanceTimersByTime(1)
  expect(get(skillFailure)).toBeNull()
  expect(vi.getTimerCount()).toBe(0)
})

it('refreshes repeated failures without accumulating messages or timers', () => {
  showSkillFailure('Not enough mana.')
  const firstId = get(skillFailure)?.id
  vi.advanceTimersByTime(2000)
  showSkillFailure('Not enough mana.')
  expect(get(skillFailure)?.id).not.toBe(firstId)
  expect(vi.getTimerCount()).toBe(1)
  vi.advanceTimersByTime(500)
  expect(get(skillFailure)?.text).toBe('Not enough mana.')
  vi.advanceTimersByTime(2000)
  expect(get(skillFailure)).toBeNull()
})

it('lets a newer reason outlive the previous expiration', () => {
  showSkillFailure('Target is too far away.')
  vi.advanceTimersByTime(1000)
  showSkillFailure('Cannot use Auscultation.')
  vi.advanceTimersByTime(1500)
  expect(get(skillFailure)?.text).toBe('Cannot use Auscultation.')
  vi.advanceTimersByTime(1000)
  expect(get(skillFailure)).toBeNull()
})

it('clears both the visible failure and its timer when the session ends', () => {
  showSkillFailure('Auscultation is not ready yet.')
  clearSkillFailure()
  expect(get(skillFailure)).toBeNull()
  expect(vi.getTimerCount()).toBe(0)
  showSkillFailure('New session')
  vi.advanceTimersByTime(SKILL_FAILURE_DURATION_MS)
  expect(get(skillFailure)).toBeNull()
})
