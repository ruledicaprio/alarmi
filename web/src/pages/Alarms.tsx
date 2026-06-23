import { useEffect, useMemo, useRef, useState } from 'react'
import { ProTable, type ActionType, type ProColumns } from '@ant-design/pro-components'
import { Link, useSearchParams } from 'react-router-dom'
import { Button, Space, Tag, message } from 'antd'
import { ReloadOutlined, DownloadOutlined } from '@ant-design/icons'
import dayjs from 'dayjs'
import { api, qs, ALL_SOURCES, ALL_CLASSES, type RecentEvent, type Severity, type Transition } from '../api'
import { formatTs, tsSorter } from '../utils'
import { SeverityTag, TransitionTag, SourceTag, ClassTag } from '../components/Tags'
import AlarmDrawer from '../components/AlarmDrawer'

const SEVERITIES: Severity[]   = ['critical','major','minor','warning','info']
const TRANSITIONS: Transition[] = ['raise','clear','instant']
const POLL_MS = 30_000

function rowStyle(r: RecentEvent): React.CSSProperties {
  // Cleared events are dimmed; active raises get a severity tint.
  if (r.transition === 'clear') return { opacity: 0.55 }
  switch (r.severity) {
    case 'critical': return { background: '#fff1f0' }
    case 'major':    return { background: '#fff7e6' }
    case 'minor':    return { background: '#feffe6' }
    case 'warning':  return { background: '#f6ffed' }
    default:         return {}
  }
}

// Filter fields we sync to URL (ProTable's search form). Keep the keys mirror
// the column dataIndex so ProTable initialValues + URL stay 1:1.
const FILTER_KEYS = ['site_key','alarm_class','severity','transition','source','raw_alarm','hours']

function csvEscape(v: any): string {
  const s = (v ?? '').toString()
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s
}
function toCsv(rows: RecentEvent[]): string {
  const cols: (keyof RecentEvent)[] = ['event_time','source','site_key','region','alarm_class','severity','transition','raw_alarm','device_ip']
  const head = cols.join(',')
  const body = rows.map(r => cols.map(c => csvEscape(r[c])).join(',')).join('\n')
  return head + '\n' + body
}
function downloadFile(name: string, content: string, mime = 'text/csv') {
  const a = document.createElement('a')
  a.href = URL.createObjectURL(new Blob([content], { type: mime }))
  a.download = name; a.click()
  setTimeout(() => URL.revokeObjectURL(a.href), 1000)
}

export default function Alarms() {
  const ref = useRef<ActionType>()
  const [searchParams, setSearchParams] = useSearchParams()
  const [hours, setHours] = useState<number>(() => Number(searchParams.get('hours')) || 24)
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [selected, setSelected] = useState<RecentEvent | null>(null)
  const [selectedKeys, setSelectedKeys] = useState<React.Key[]>([])
  const [pageData, setPageData] = useState<RecentEvent[]>([])

  // Initial filter values from URL → ProTable form
  const initial = useMemo(() => {
    const o: Record<string, any> = {}
    for (const k of FILTER_KEYS) if (k !== 'hours' && searchParams.get(k)) o[k] = searchParams.get(k)
    return o
  }, [])

  // Live polling
  useEffect(() => {
    const t = setInterval(() => ref.current?.reload(true), POLL_MS)
    return () => clearInterval(t)
  }, [])

  const columns: ProColumns<RecentEvent>[] = [
    { title: 'Time', dataIndex: 'event_time', width: 175, hideInSearch: true, copyable: true,
      sorter: (a, b) => tsSorter<RecentEvent>('event_time')(a, b),
      defaultSortOrder: 'descend',
      render: (_, r) => formatTs(r.event_time) },
    {
      title: 'Site', dataIndex: 'site_key', width: 180,
      render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`} onClick={e => e.stopPropagation()}>{r.site_key}</Link>,
      fieldProps: { placeholder: 'Search site…' },
    },
    {
      title: 'Raw alarm', dataIndex: 'raw_alarm', width: 220, ellipsis: true,
      fieldProps: { placeholder: 'contains…' },
    },
    {
      title: 'Class', dataIndex: 'alarm_class', width: 180,
      render: (v) => <ClassTag v={v as string} />,
      valueType: 'select', valueEnum: Object.fromEntries(ALL_CLASSES.map(c => [c, { text: c }])),
    },
    {
      title: 'Severity', dataIndex: 'severity', width: 100,
      render: v => <SeverityTag v={v as Severity} />,
      valueType: 'select', valueEnum: Object.fromEntries(SEVERITIES.map(s => [s, { text: s }])),
    },
    {
      title: 'Transition', dataIndex: 'transition', width: 110,
      render: v => <TransitionTag v={v as Transition} />,
      valueType: 'select', valueEnum: Object.fromEntries(TRANSITIONS.map(t => [t, { text: t }])),
    },
    {
      title: 'Source', dataIndex: 'source', width: 130,
      render: v => <SourceTag v={v as any} />,
      valueType: 'select', valueEnum: Object.fromEntries(ALL_SOURCES.map(s => [s, { text: s }])),
    },
    { title: 'Region', dataIndex: 'region', width: 100, hideInSearch: true,
      render: v => v ? <Tag>{v as string}</Tag> : '—' },
    { title: 'IP', dataIndex: 'device_ip', width: 130, hideInSearch: true, copyable: true },
  ]

  const activeFilters = Object.entries(Object.fromEntries(searchParams)).filter(([k]) => FILTER_KEYS.includes(k))

  return (
    <>
      <ProTable<RecentEvent>
        actionRef={ref}
        columns={columns}
        rowKey={(r) => r.event_time + r.site_key + r.alarm_class + r.transition}
        form={{ initialValues: initial }}
        rowSelection={{
          selectedRowKeys: selectedKeys,
          onChange: (keys) => setSelectedKeys(keys),
        }}
        tableAlertRender={({ selectedRowKeys, onCleanSelected }) => (
          <Space size="middle">
            <span>{selectedRowKeys.length} selected</span>
            <a onClick={onCleanSelected}>clear</a>
          </Space>
        )}
        tableAlertOptionRender={() => (
          <Space>
            <Button size="small" onClick={() => {
              const picked = pageData.filter((r, _i, _arr) =>
                selectedKeys.includes(r.event_time + r.site_key + r.alarm_class + r.transition))
              downloadFile(`alarms-selected-${dayjs().format('YYYYMMDD-HHmmss')}.csv`, toCsv(picked))
            }}>Export selected CSV</Button>
          </Space>
        )}
        request={async (params) => {
          // Sync filters into URL (so view is bookmarkable)
          const next = new URLSearchParams()
          for (const k of FILTER_KEYS) {
            const v = k === 'hours' ? String(hours) : (params as any)[k]
            if (v !== undefined && v !== '' && v !== null) next.set(k, String(v))
          }
          setSearchParams(next, { replace: true })

          const offset = ((params.current ?? 1) - 1) * (params.pageSize ?? 50)
          const d = await api<{items: RecentEvent[]; total: number}>(
            `/api/alarms/recent${qs({
              hours,
              limit: params.pageSize ?? 50,
              offset,
              site: (params as any).site_key,
              class: (params as any).alarm_class,
              severity: (params as any).severity,
              transition: (params as any).transition,
              source: (params as any).source,
              raw_alarm_like: (params as any).raw_alarm,
            })}`)
          setPageData(d.items)
          return { data: d.items, success: true, total: d.total }
        }}
        onRow={(record) => ({
          onClick: () => { setSelected(record); setDrawerOpen(true) },
          style: { cursor: 'pointer', ...rowStyle(record) },
        })}
        pagination={{ defaultPageSize: 50, pageSizeOptions: ['20','50','100','200'], showSizeChanger: true, showTotal: (t) => `${t} events match` }}
        search={{ labelWidth: 'auto', filterType: 'light', collapsed: false }}
        dateFormatter="string"
        headerTitle={
          <Space>
            <span>Recent alarms</span>
            {activeFilters.length > 0 && <Tag color="processing">{activeFilters.length} active filter{activeFilters.length>1?'s':''}</Tag>}
          </Space>
        }
        toolBarRender={() => [
          <Space key="h">
            <span>Window:</span>
            <select value={hours} onChange={(e) => { setHours(+e.target.value); ref.current?.reload() }}
              style={{ padding: '4px 8px', borderRadius: 4 }}>
              <option value={1}>1h</option><option value={6}>6h</option>
              <option value={24}>24h</option><option value={72}>3d</option>
              <option value={168}>7d</option><option value={720}>30d</option>
              <option value={100000}>All</option>
            </select>
          </Space>,
          <Button key="e" icon={<DownloadOutlined />}
            onClick={() => {
              if (pageData.length === 0) { message.info('No rows on current page'); return }
              downloadFile(`alarms-${dayjs().format('YYYYMMDD-HHmmss')}.csv`, toCsv(pageData))
            }}>
            Export CSV ({pageData.length})
          </Button>,
          <Button key="r" icon={<ReloadOutlined />} onClick={() => ref.current?.reload()}>Refresh</Button>,
        ]}
        scroll={{ x: 1200 }}
        sticky
      />
      <AlarmDrawer
        open={drawerOpen} alarm={selected}
        onClose={() => setDrawerOpen(false)}
      />
    </>
  )
}
