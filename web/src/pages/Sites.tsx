import { useEffect, useRef, useState } from 'react'
import { ProTable, type ActionType, type ProColumns } from '@ant-design/pro-components'
import { Tag } from 'antd'
import { Link } from 'react-router-dom'
import { api, qs, type Site } from '../api'
import { formatTs, tsSorter } from '../utils'
import { SeverityTag } from '../components/Tags'

export default function Sites() {
  const ref = useRef<ActionType>()
  const [regions, setRegions] = useState<{region: string; label: string}[]>([])
  useEffect(() => {
    api<{items: {region: string; label: string}[]}>('/api/regions').then(d => setRegions(d.items || [])).catch(() => setRegions([]))
  }, [])

  const columns: ProColumns<Site>[] = [
    {
      title: 'Site key', dataIndex: 'site_key', width: 240,
      render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link>,
      fieldProps: { placeholder: 'search site_key or name…' },
    },
    { title: 'Name', dataIndex: 'name', hideInSearch: true, ellipsis: true },
    {
      title: 'Region', dataIndex: 'region', width: 140,
      valueType: 'select',
      valueEnum: Object.fromEntries(regions.map(r => [r.region, { text: r.label || r.region }])),
      render: v => v ? <Tag>{v as string}</Tag> : '—',
    },
    { title: 'Municipality', dataIndex: 'municipality', hideInSearch: true, ellipsis: true, width: 160 },
    {
      title: 'Open alarms', dataIndex: 'open_alarms', width: 160, sorter: true, align: 'right',
      hideInSearch: true,
      render: (v, r) => {
        const n = v as number
        if (n === 0) return <Tag color="green">0</Tag>
        return <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
          <Tag color="red" style={{ margin: 0 }}>{n}</Tag>
          {r.worst_severity && <SeverityTag v={r.worst_severity as any} />}
        </span>
      },
    },
    {
      title: 'Min open', dataIndex: 'min_open', hideInTable: true,
      valueType: 'digit', fieldProps: { min: 0 },
    },
    { title: 'Last event', dataIndex: 'last_event', width: 180, hideInSearch: true,
      sorter: tsSorter<Site>('last_event'), defaultSortOrder: 'descend',
      render: v => v ? formatTs(v as string) : '—' },
  ]

  return (
    <ProTable<Site>
      actionRef={ref}
      columns={columns}
      rowKey="site_key"
      request={async (params) => {
        const offset = ((params.current ?? 1) - 1) * (params.pageSize ?? 50)
        const d = await api<{items: Site[]; total: number}>(
          `/api/sites${qs({
            limit: params.pageSize ?? 50,
            offset,
            q: params.site_key,
            region: params.region,
            min_open: params.min_open ?? 0,
          })}`)
        return { data: d.items, success: true, total: d.total }
      }}
      pagination={{ defaultPageSize: 50, pageSizeOptions: ['20','50','100','200'], showSizeChanger: true }}
      search={{ labelWidth: 'auto', filterType: 'light' }}
      headerTitle="Sites"
      scroll={{ x: 1000 }}
      sticky
    />
  )
}
