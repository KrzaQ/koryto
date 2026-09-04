// Instants come from the API as RFC 3339 UTC. Everything shown or typed is a
// wall-clock time in one zone: the house zone ("as recorded") or the
// browser's. Conversion both ways goes through Intl, so there is no offset
// table to keep and DST is the platform's problem.

export const DEFAULT_HOUSE_ZONE = 'Europe/Warsaw'

export function systemZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
  } catch {
    return 'UTC'
  }
}

type Parts = { y: number; m: number; d: number; hh: number; mm: number; ss: number }

const formatters = new Map<string, Intl.DateTimeFormat>()
function formatter(zone: string): Intl.DateTimeFormat {
  let f = formatters.get(zone)
  if (!f) {
    f = new Intl.DateTimeFormat('en-GB', {
      timeZone: zone,
      hourCycle: 'h23',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
    formatters.set(zone, f)
  }
  return f
}

function partsIn(date: Date, zone: string): Parts {
  const p = formatter(zone).formatToParts(date)
  const get = (t: Intl.DateTimeFormatPartTypes) => Number(p.find((x) => x.type === t)?.value ?? 0)
  return {
    y: get('year'),
    m: get('month'),
    d: get('day'),
    hh: get('hour'),
    mm: get('minute'),
    ss: get('second'),
  }
}

const pad = (n: number) => String(n).padStart(2, '0')

/** "YYYY-MM-DD HH:MM" on a clock in `zone`. */
export function formatDateTime(iso: string, zone: string): string {
  const p = partsIn(new Date(iso), zone)
  return `${p.y}-${pad(p.m)}-${pad(p.d)} ${pad(p.hh)}:${pad(p.mm)}`
}

/** Instant -> the value a datetime-local input wants, on a clock in `zone`. */
export function toLocalInput(iso: string, zone: string): string {
  const p = partsIn(new Date(iso), zone)
  return `${p.y}-${pad(p.m)}-${pad(p.d)}T${pad(p.hh)}:${pad(p.mm)}`
}

/** The current time as a datetime-local value on a clock in `zone`. */
export function nowLocal(zone: string, now = new Date()): string {
  return toLocalInput(now.toISOString(), zone)
}

/**
 * datetime-local value (a wall-clock time in `zone`) -> RFC 3339 UTC. Two
 * rounds of "what does this instant look like in the zone, and how far off is
 * it" settle the offset, DST edges included.
 */
export function fromLocalInput(value: string, zone: string): string {
  const [date, time = '00:00'] = value.split('T')
  const [y, m, d] = (date ?? '').split('-').map(Number)
  const [hh, mm, ss = 0] = time.split(':').map(Number)
  const target = Date.UTC(y!, m! - 1, d!, hh!, mm!, ss)
  let guess = target
  const wallOf = (t: number) => {
    const p = partsIn(new Date(t), zone)
    return Date.UTC(p.y, p.m - 1, p.d, p.hh, p.mm, p.ss)
  }
  for (let i = 0; i < 2; i++) guess += target - wallOf(guess)
  // A repeated hour (autumn) has two instants with this wall time; take the
  // first, as the server does.
  if (wallOf(guess - 3600_000) === target) guess -= 3600_000
  return new Date(guess).toISOString().slice(0, 19) + 'Z'
}

/** Short zone name at an instant, e.g. "CEST", "BST", "GMT+3". */
export function zoneAbbr(zone: string, at = new Date()): string {
  try {
    const p = new Intl.DateTimeFormat('en-GB', { timeZone: zone, timeZoneName: 'short' })
      .formatToParts(at)
      .find((x) => x.type === 'timeZoneName')
    return p?.value ?? zone
  } catch {
    return zone
  }
}

/** "Europe/Warsaw" -> "Warsaw". */
export function zoneCity(zone: string): string {
  return (zone.split('/').pop() ?? zone).replace(/_/g, ' ')
}
