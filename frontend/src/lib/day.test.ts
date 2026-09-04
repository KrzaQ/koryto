import { describe, expect, it } from 'vitest'
import { dayLabel, isDay, shiftDay } from './day'

describe('day', () => {
  it('validates', () => {
    expect(isDay('2026-09-04')).toBe(true)
    expect(isDay('2026-02-30')).toBe(false)
    expect(isDay('2026-9-4')).toBe(false)
    expect(isDay('yesterday')).toBe(false)
  })
  it('shifts across month and year ends', () => {
    expect(shiftDay('2026-09-30', 1)).toBe('2026-10-01')
    expect(shiftDay('2026-01-01', -1)).toBe('2025-12-31')
    expect(shiftDay('2026-09-04', 0)).toBe('2026-09-04')
  })
  it('labels', () => {
    expect(dayLabel('2026-09-04')).toBe('Fri, 4 Sept 2026')
  })
})
