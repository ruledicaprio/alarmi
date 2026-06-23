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
/** Coerce any value to a string for safe .replace() calls. */
function toStr(s: any): string {
  if (s === null || s === undefined) return ''
  return typeof s === 'string' ? s : String(s)
}

export function formatTs(s: any, withSeconds = true): string {
  const str = toStr(s)
  if (!str) return '—'
  // Normalize: " " -> "T", "+00" -> "+00:00" (single-digit offset PG quirk)
  let iso = str.replace(' ', 'T')
  iso = iso.replace(/([+-]\d\d)$/, '$1:00')
  const d = dayjs(iso)
  if (!d.isValid()) return str
  return d.format(withSeconds ? 'DD.MM.YYYY HH:mm:ss' : 'DD.MM.YYYY HH:mm')
}

/** Short variant for chart axes — "dd.MM HH:mm". */
export function formatTsShort(s: any): string {
  const str = toStr(s)
  if (!str) return ''
  const iso = str.replace(' ', 'T').replace(/([+-]\d\d)$/, '$1:00')
  const d = dayjs(iso)
  return d.isValid() ? d.format('DD.MM HH:mm') : str
}

/** Sorter helper for ProTable columns whose value is an ISO/PG timestamp string. */
export function tsSorter<T>(field: keyof T) {
  return (a: T, b: T) => {
    const ta = Date.parse(toStr(a[field]).replace(' ', 'T')) || 0
    const tb = Date.parse(toStr(b[field]).replace(' ', 'T')) || 0
    return ta - tb
  }
}
