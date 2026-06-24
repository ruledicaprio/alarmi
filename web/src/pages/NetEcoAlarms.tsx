import { useEffect, useRef, useState } from 'react'
import { ProTable, type ActionType, type ProColumns } from '@ant-design/pro-components'
import { Badge, Button, Space, Tag, Tooltip, message } from 'antd'
import { ReloadOutlined, DownloadOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'
import { api, qs, type NetEcoAlarm } from '../api'
import { formatTs } from '../utils'

const POLL_MS = 30_000

const SEV_MAP: Record<number, { label: string; color: string }> = {
  1: { label: 'CRITICAL', color: '#cf1322' },
  2: { label: 'MAJOR',    color: '#fa541c' },
  3: { label: 'MINOR',    color: '#faad14' },
  4: { label: 'WARNING',  color: '#d4b106' },
}

const STATUS_MAP: Record<number, { label: string; color: 'error' | 'warning' | 'default' | 'success' | 'processing' }> = {
  1: { label: 'Active',      color: 'error'      },
  2: { label: 'Acked',       color: 'warning'    },
  4: { label: 'Handled',     color: 'processing' },
  5: { label: 'User-clear',  color: 'success'    },
  6: { label: 'Auto-clear',  color: 'success'    },
}

function SevTag({ v }: { v: number | null }) {
  if (v == null) return <Tag>—</Tag>
  const s = SEV_MAP[v] ?? { label: `SEV${v}`, color: '#aaa' }
  return <Tag color={s.color} style={{ fontWeight: 600, fontSize: 11 }}>{s.label}</Tag>
}

function StatusBadge({ v }: { v: number | null }) {
  if (v == null) return <Badge status="default" text="—" />
  const s = STATUS_MAP[v] ?? { label: `S${v}`, color: 'default' as const }
  return <Badge status={s.color} text={s.label} />
}

function duration(raise: string | null, repair: string | null): string {
  if (!raise) return '—'
  const start = dayjs(raise)
  const end   = repair ? dayjs(repair) : dayjs()
  const mins  = end.diff(start, 'minute')
  if (mins < 60)   return `${mins}m`
  if (mins < 1440) return `${Math.floor(mins / 60)}h ${mins % 60}m`
  return `${Math.floor(mins / 1440)}d ${Math.floor((mins % 1440) / 60)}h`
}

function toCsv(rows: NetEcoAlarm[]): string {
  const cols: (keyof NetEcoAlarm)[] = [
    'raise_time', 'station_code', 'station_name', 'dev_name', 'std_type_name',
    'alarm_name', 'alarm_cause', 'severity', 'status', 'repair_time', 'source',
  ]
  const esc = (v: unknown) => {
    const s = (v ?? '').toString()
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
  }
  return cols.join(',') + '\n' + rows.map(r => cols.map(c => esc(r[c])).join(',')).join('\n')
}

export default function NetEcoAlarms() {
  const ref    = useRef<ActionType>()
  const [rows, setRows] = useState<NetEcoAlarm[]>([])

  useEffect(() => {
    const t = setInterval(() => ref.current?.reload(true), POLL_MS)
    return () => clearInterval(t)
  }, [])

  const columns: ProColumns<NetEcoAlarm>[] = [
    {
      title: 'Raised', dataIndex: 'raise_time', width: 155, hideInSearch: true,
      defaultSortOrder: 'descend',
      sorter: (a, b) =>
        new Date(a.raise_time ?? 0).getTime() - new Date(b.raise_time ?? 0).getTime(),
      render: (_, r) => formatTs(r.raise_time ?? ''),
    },
    {
      title: 'Station', dataIndex: 'station_code', width: 150,
      fieldProps: { placeholder: 'station code' },
      render: (_, r) => (
        <Tooltip title={r.station_name || undefined}>
          <span style={{ fontFamily: 'monospace', fontSize: 12 }}>{r.station_code || '—'}</span>
        </Tooltip>
      ),
    },
    {
      title: 'Device', dataIndex: 'dev_name', width: 180, hideInSearch: true,
      render: (_, r) => (
        <>
          {r.std_type_name && (
            <div style={{ fontSize: 10, opacity: 0.55 }}>{r.std_type_name}</div>
          )}
          <div>{r.dev_name || '—'}</div>
        </>
      ),
    },
    {
      title: 'Alarm', dataIndex: 'alarm_name', ellipsis: true, hideInSearch: true,
      render: (_, r) => (
        <Tooltip title={r.alarm_cause || undefined}>
          {r.alarm_name || '—'}
        </Tooltip>
      ),
    },
    {
      title: 'Severity', dataIndex: 'severity', width: 100,
      valueType: 'select',
      valueEnum: {
        1: { text: 'Critical' },
        2: { text: 'Major'    },
        3: { text: 'Minor'    },
        4: { text: 'Warning'  },
      },
      render: (_, r) => <SevTag v={r.severity} />,
    },
    {
      title: 'Status', dataIndex: 'status', width: 110,
      valueType: 'select',
      valueEnum: {
        1: { text: 'Active'     },
        2: { text: 'Acked'      },
        4: { text: 'Handled'    },
        5: { text: 'User-clear' },
        6: { text: 'Auto-clear' },
      },
      render: (_, r) => <StatusBadge v={r.status} />,
    },
    {
      title: 'Duration', dataIndex: 'raise_time', key: 'duration', width: 90,
      hideInSearch: true,
      render: (_, r) => duration(r.raise_time, r.repair_time),
    },
    {
      title: 'Source', dataIndex: 'source', width: 90, hideInSearch: true,
      render: (_, r) => <Tag style={{ fontSize: 10 }}>{r.source}</Tag>,
    },
  ]

  return (
    <ProTable<NetEcoAlarm>
      actionRef={ref}
      rowKey="alarm_id"
      columns={columns}
      request={async (params) => {
        const p      = params as Record<string, any>
        const limit  = params.pageSize ?? 200
        const offset = ((params.current ?? 1) - 1) * limit
        try {
          const res = await api<{ total: number; items: NetEcoAlarm[] }>(
            `/api/neteco/alarms${qs({
              station:  p.station_code ?? '',
              severity: p.severity    ?? '',
              status:   p.status      ?? '',
              limit,
              offset,
            })}`
          )
          setRows(res.items ?? [])
          return { data: res.items, success: true, total: res.total }
        } catch (e) {
          message.error(String(e))
          return { data: [], success: false, total: 0 }
        }
      }}
      rowClassName={(r) => {
        if (r.status !== 1) return ''
        if (r.severity === 1) return 'row-critical'
        if (r.severity === 2) return 'row-major'
        return ''
      }}
      toolBarRender={() => [
        <Button
          key="export"
          icon={<DownloadOutlined />}
          size="small"
          onClick={() => {
            if (!rows.length) { message.warning('No data to export'); return }
            const url = URL.createObjectURL(
              new Blob([toCsv(rows)], { type: 'text/csv' })
            )
            const a = document.createElement('a')
            a.href     = url
            a.download = `neteco_alarms_${dayjs().format('YYYYMMDD_HHmm')}.csv`
            a.click()
            setTimeout(() => URL.revokeObjectURL(url), 10_000)
          }}
        >
          Export
        </Button>,
        <Button
          key="reload"
          icon={<ReloadOutlined />}
          size="small"
          onClick={() => ref.current?.reload()}
        >
          Refresh
        </Button>,
      ]}
      search={{ labelWidth: 'auto', defaultCollapsed: false }}
      pagination={{
        defaultPageSize: 200,
        showSizeChanger: true,
        pageSizeOptions: ['100', '200', '500'],
      }}
      cardBordered
      headerTitle={
        <Space>
          <span>NetEco NBI Alarms</span>
          <Tag color="cyan">live · 30s</Tag>
        </Space>
      }
      scroll={{ x: 980 }}
    />
  )
}
