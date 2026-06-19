import type { Severity, Transition, Source } from './api'

// Severity tokens — used by SeverityTag, Statistic valueStyle, chart fills.
export const SEV_COLOR: Record<Severity, string> = {
  critical: '#cf1322',
  major:    '#fa541c',
  minor:    '#faad14',
  warning:  '#fadb14',
  info:     '#1677ff',
}

export const TRANSITION_COLOR: Record<Transition, string> = {
  raise:   'red',
  clear:   'green',
  instant: 'blue',
}

// Source colors — calm, monochrome-ish so they don't fight severity in tables.
export const SOURCE_COLOR: Record<Source, string> = {
  ignition:     'geekblue',
  net_eco:      'cyan',
  u2020:        'purple',
  rps_sc200:    'magenta',
  rps_sc300:    'magenta',
  dse74xx:      'orange',
  benning:      'lime',
  baran:        'volcano',
  modbus_eaton: 'gold',
  html_oos:     'default',
}
