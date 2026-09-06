import { describe, expect, it } from 'vitest'
import {
  dayIn,
  formatDateTime,
  fromLocalInput,
  nowLocal,
  toLocalInput,
  zoneAbbr,
  zoneCity,
} from './time'

describe('time', () => {
  it('shows an instant on the chosen clock', () => {
    expect(formatDateTime('2026-08-05T08:00:00Z', 'Europe/Warsaw')).toBe('2026-08-05 10:00')
    expect(formatDateTime('2026-08-05T08:00:00Z', 'Europe/London')).toBe('2026-08-05 09:00')
    expect(formatDateTime('2026-08-05T08:00:00Z', 'UTC')).toBe('2026-08-05 08:00')
    expect(toLocalInput('2026-08-05T08:00:00Z', 'Europe/Warsaw')).toBe('2026-08-05T10:00')
  })

  it('reads a wall-clock time on the chosen clock, DST included', () => {
    expect(fromLocalInput('2026-08-05T10:00', 'Europe/Warsaw')).toBe('2026-08-05T08:00:00Z')
    expect(fromLocalInput('2026-01-05T10:00', 'Europe/Warsaw')).toBe('2026-01-05T09:00:00Z')
    expect(fromLocalInput('2026-08-05T10:00', 'UTC')).toBe('2026-08-05T10:00:00Z')
    // A time in the spring gap moves forward rather than vanishing.
    expect(fromLocalInput('2026-03-29T02:30', 'Europe/Warsaw')).toBe('2026-03-29T01:30:00Z')
  })

  it('round-trips through the input value', () => {
    const iso = '2026-10-25T00:30:00Z'
    expect(fromLocalInput(toLocalInput(iso, 'Europe/Warsaw'), 'Europe/Warsaw')).toBe(iso)
  })

  it('labels zones', () => {
    expect(nowLocal('UTC', new Date(Date.UTC(2026, 8, 3, 7, 5)))).toBe('2026-09-03T07:05')
    expect(zoneAbbr('Europe/Warsaw', new Date(Date.UTC(2026, 7, 1)))).toBe('CEST')
    expect(zoneCity('Europe/Warsaw')).toBe('Warsaw')
    expect(zoneCity('America/New_York')).toBe('New York')
  })
})

describe('dayIn', () => {
  it('follows the zone and the day boundary, not UTC', () => {
    // 01:30 in Warsaw on the 5th, with the day starting at 04:00, is the 4th.
    expect(dayIn('Europe/Warsaw', 240, new Date('2026-09-04T23:30:00Z'))).toBe('2026-09-04')
    // 04:00 sharp is the new day; 03:59 is not.
    expect(dayIn('Europe/Warsaw', 240, new Date('2026-09-05T02:00:00Z'))).toBe('2026-09-05')
    expect(dayIn('Europe/Warsaw', 240, new Date('2026-09-05T01:59:00Z'))).toBe('2026-09-04')
    // The same instant is a different day on the other side of the world.
    expect(dayIn('Pacific/Auckland', 240, new Date('2026-09-04T23:30:00Z'))).toBe('2026-09-05')
    // Without a boundary it is just the date on that clock.
    expect(dayIn('UTC', 0, new Date('2026-09-04T23:30:00Z'))).toBe('2026-09-04')
  })
})
