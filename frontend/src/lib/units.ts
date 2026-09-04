// The grammars are enforced by the server; the client only needs to know
// whether a value looks acceptable, and how to show what comes back.

const KG = /^\d{1,3}([.,]\d{1,2})?(\s*kg)?$/i
const DURATION = [
  /^\d+$/,
  /^\d+m$/i,
  /^\d+h(\s*\d{1,2}m?)?$/i,
  /^\d+([.,]\d+)?h$/i,
  /^\d+:[0-5]\d$/,
]
const PORTIONS = /^\d{1,4}([.,]\d{1,2})?$/

export function looksLikeKg(input: string): boolean {
  const s = input.trim()
  if (!KG.test(s)) return false
  const n = Number(s.replace(/kg/i, '').replace(',', '.'))
  return n >= 20 && n <= 400
}

export function looksLikeDuration(input: string): boolean {
  const s = input.trim().replace(/\s+/g, '')
  return s.length > 0 && s !== '0' && DURATION.some((re) => re.test(s))
}

export function looksLikePortions(input: string): boolean {
  const s = input.trim()
  return PORTIONS.test(s) && Number(s.replace(',', '.')) > 0
}

/** Grams -> "82.4". */
export function formatKg(grams: number): string {
  return String(Math.round(grams / 10) / 100)
}

/** Whole minutes -> "45m", "1h", "1h30". */
export function formatMinutes(minutes: number): string {
  const h = Math.floor(minutes / 60)
  const m = minutes % 60
  if (h === 0) return `${m}m`
  if (m === 0) return `${h}h`
  return `${h}h${String(m).padStart(2, '0')}`
}

/** "+120" / "−80" / "0". */
export function signed(n: number): string {
  if (n > 0) return `+${n}`
  if (n < 0) return `−${-n}`
  return '0'
}
