import { describe, expect, it } from 'vitest'
import { roomOf } from './budget'
import type { DayDto } from '@/api/types'

function day(kcal: number, burn: number | null, target: number | null): DayDto {
  return {
    day: '2026-09-05',
    user_id: 1,
    logged: true,
    meals: [],
    weights: [],
    activities: [],
    totals: {
      kcal,
      protein_g: null,
      meals: 1,
      meals_without_protein: 0,
      sport_minutes: 0,
      sport_kcal: null,
    },
    target:
      target == null
        ? null
        : {
            id: 1,
            user_id: 1,
            valid_from: '2026-09-01',
            kcal: target,
            protein_g: null,
            weight_kg: null,
          },
    balance: target == null ? null : kcal - target,
    expenditure: {
      kcal: burn,
      base_kcal: burn,
      sport_kcal: 0,
      basis: burn == null ? 'none' : 'seed',
      logged_days: 1,
      weight_span_days: 0,
      seed_kcal: burn,
    },
    balance_vs_expenditure: burn == null ? null : kcal - burn,
  } as DayDto
}

describe('roomOf', () => {
  it('prefers the burn, falls back to the target, else nothing', () => {
    expect(roomOf(day(2000, 2400, 2200))).toEqual({ kind: 'burn', against: 2400, kcal: 400 })
    expect(roomOf(day(2000, null, 2200))).toEqual({ kind: 'target', against: 2200, kcal: 200 })
    expect(roomOf(day(2000, null, null))).toBeNull()
    // Over the line is negative room.
    expect(roomOf(day(2600, 2400, null))?.kcal).toBe(-200)
  })
})
