import dayjs from 'dayjs'
import utc from 'dayjs/plugin/utc'
import customParseFormat from 'dayjs/plugin/customParseFormat'

dayjs.extend(utc)
dayjs.extend(customParseFormat)

// PostgreSQL ::text on a timestamptz returns e.g.
//   "2026-06-19 09:23:35.112456+00"
// JS Date can't parse that reliably (space vs T, fractional seconds, +00 short
// offset). dayjs handles most of it once we normalize.
// Always renders in browser-LOCAL time, as dd.MM.yyyy HH:mm:ss per operator ask.
export function formatTs(s: string | null | undefined, withSeconds = true): string {
  if (!s) return '—'
  // Normalize: " " -> "T", "+00" -> "+00:00" (single-digit offset PG quirk)
  let iso = s.replace(' ', 'T')
  iso = iso.replace(/([+-]\d\d)$/, '$1:00')
  const d = dayjs(iso)
  if (!d.isValid()) return s
  return d.format(withSeconds ? 'DD.MM.YYYY HH:mm:ss' : 'DD.MM.YYYY HH:mm')
}

/** Short variant for chart axes — "dd.MM HH:mm". */
export function formatTsShort(s: string | null | undefined): string {
  if (!s) return ''
  const iso = s.replace(' ', 'T').replace(/([+-]\d\d)$/, '$1:00')
  const d = dayjs(iso)
  return d.isValid() ? d.format('DD.MM HH:mm') : s
}
