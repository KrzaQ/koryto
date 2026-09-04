// Days are YYYY-MM-DD strings, the server's accounting days. Arithmetic is
// done in UTC so a browser zone never shifts a date.

const DAY = /^\d{4}-\d{2}-\d{2}$/

export function isDay(s: string): boolean {
  if (!DAY.test(s)) return false
  const d = new Date(`${s}T00:00:00Z`)
  return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === s
}

export function shiftDay(day: string, by: number): string {
  const d = new Date(`${day}T00:00:00Z`)
  d.setUTCDate(d.getUTCDate() + by)
  return d.toISOString().slice(0, 10)
}

/** "Thu 4 Sep 2026". */
export function dayLabel(day: string): string {
  const d = new Date(`${day}T00:00:00Z`)
  return d.toLocaleDateString('en-GB', {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    timeZone: 'UTC',
  })
}

/** "Thu 4 Sep". */
export function shortDayLabel(day: string): string {
  const d = new Date(`${day}T00:00:00Z`)
  return d.toLocaleDateString('en-GB', {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
  })
}
