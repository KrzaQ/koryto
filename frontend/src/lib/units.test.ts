import { describe, expect, it } from 'vitest'
import {
  formatKg,
  formatMinutes,
  looksLikeDuration,
  looksLikeKg,
  looksLikePortions,
  signed,
} from './units'

describe('units', () => {
  it('recognises kilograms in range', () => {
    for (const ok of ['82.4', '82,4', '82', ' 82.45 kg', '100'])
      expect(looksLikeKg(ok), ok).toBe(true)
    for (const bad of ['', 'heavy', '10', '500', '82.456'])
      expect(looksLikeKg(bad), bad).toBe(false)
  })
  it('recognises durations', () => {
    for (const ok of ['45', '45m', '1h', '1h30', '1h 30m', '1:30', '1.5h'])
      expect(looksLikeDuration(ok), ok).toBe(true)
    for (const bad of ['', '0', 'abc', '1:60', '1.5'])
      expect(looksLikeDuration(bad), bad).toBe(false)
  })
  it('recognises portions', () => {
    for (const ok of ['1', '0.5', '1,5', '2.25']) expect(looksLikePortions(ok), ok).toBe(true)
    for (const bad of ['', '0', 'lots', '1.234']) expect(looksLikePortions(bad), bad).toBe(false)
  })
  it('formats', () => {
    expect(formatKg(82400)).toBe('82.4')
    expect(formatKg(82000)).toBe('82')
    expect(formatKg(82457)).toBe('82.46')
    expect(formatMinutes(45)).toBe('45m')
    expect(formatMinutes(60)).toBe('1h')
    expect(formatMinutes(90)).toBe('1h30')
    expect(signed(120)).toBe('+120')
    expect(signed(-80)).toBe('−80')
    expect(signed(0)).toBe('0')
  })
})
