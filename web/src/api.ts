// Same-origin in production (served by bht-api); dev uses Vite proxy.

export async function api<T = any>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(path, init)
  if (!r.ok) throw new Error(`${r.status} ${r.statusText} — ${path}`)
  return r.json()
}

export function qs(params: Record<string, string | number | undefined | null>): string {
  const u = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === '') continue
    u.set(k, String(v))
  }
  const s = u.toString()
  return s ? `?${s}` : ''
}

// -------------------- shared types

export type Severity   = 'critical' | 'major' | 'minor' | 'warning' | 'info'
export type Transition = 'raise' | 'clear' | 'instant'
export type Source =
  | 'ignition' | 'net_eco' | 'u2020' | 'rps_sc200' | 'rps_sc300'
  | 'dse74xx'  | 'benning' | 'baran' | 'modbus_eaton' | 'html_oos'

export interface RecentEvent {
  event_time: string
  source: Source
  site_key: string
  alarm_class: string
  severity: Severity
  transition: Transition
  raw_alarm: string
  device_ip: string
  region: string
}

export interface Site {
  site_key: string
  name: string
  region: string
  municipality: string
  open_alarms: number
  last_event: string
}

export interface SiteGeo {
  site_key: string
  display_name: string
  lat: number
  lon: number
  region: string
  municipality: string
  technologies: string[]
  has_genset: boolean
  has_battery: boolean
  has_solar: boolean
  open_alarms: number
  worst_severity: Severity | null
}

export interface Episode {
  raised_at: string
  cleared_at: string
  duration_seconds: number
  is_open: boolean
  source: Source
  alarm_class: string
  severity: Severity
}

export const ALL_SOURCES: Source[] = [
  'ignition','net_eco','u2020','rps_sc200','rps_sc300',
  'dse74xx','benning','baran','modbus_eaton','html_oos',
]

export const ALL_CLASSES = [
  'MAINS_FAILURE','RECTIFIER_FAILURE','RECTIFIER_COMMS','BATTERY_LOW','BATTERY_FAULT',
  'HIGH_VOLTAGE','COMMS_LOST','NE_DISCONNECTED','COOLING_FAULT','GENSET_EVENT',
  'SOLAR_FAULT','UPS_MODULE','FUSE_LOAD','DOOR_OPEN','SERVICE_OUTAGE',
  'GENERIC_ERROR','UNCLASSIFIED',
]
