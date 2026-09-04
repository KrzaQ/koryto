// What a day left: burn minus intake when there is an estimate, target minus
// intake otherwise. Positive is room left. The day page and the home page
// answer the same question, so they read it the same way.
import type { DayDto } from '@/api/types'

export type Room = { kind: 'burn' | 'target'; against: number; kcal: number }

export function roomOf(day: DayDto): Room | null {
  const eaten = day.totals.kcal
  const burn = day.expenditure.kcal
  if (burn != null) return { kind: 'burn', against: burn, kcal: burn - eaten }
  const target = day.target?.kcal
  if (target != null) return { kind: 'target', against: target, kcal: target - eaten }
  return null
}
