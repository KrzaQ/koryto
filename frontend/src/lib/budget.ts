// What a day left: burn minus intake when there is an estimate, target minus
// intake otherwise. Positive is room left. The day page and the home page
// answer the same question, so they read it the same way.
import type { DayDto, Summary } from '@/api/types'

export type Room = { kind: 'burn' | 'target'; against: number; kcal: number }

/**
 * The same question over a range: the mean room per logged day. Positive is
 * a deficit, so it reads like the day figures rather than against them.
 */
export function weekRoomOf(week: Summary | null): Omit<Room, 'against'> | null {
  if (!week) return null
  if (week.mean_balance_vs_expenditure != null)
    return { kind: 'burn', kcal: -week.mean_balance_vs_expenditure }
  if (week.mean_balance != null) return { kind: 'target', kcal: -week.mean_balance }
  return null
}

export function roomOf(day: DayDto): Room | null {
  const eaten = day.totals.kcal
  const burn = day.expenditure.kcal
  if (burn != null) return { kind: 'burn', against: burn, kcal: burn - eaten }
  const target = day.target?.kcal
  if (target != null) return { kind: 'target', against: target, kcal: target - eaten }
  return null
}
