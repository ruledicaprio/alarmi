import { useState } from 'react'
import { Tabs, Alert, Select, Input, Tag } from 'antd'
import { ProTable, ProCard } from '@ant-design/pro-components'
import { Link } from 'react-router-dom'
import { api, qs } from '../api'
import { formatTs } from '../utils'

interface OrphanRow { site_key: string; events: number; last_seen: string }
interface StaleRow  { site_key: string; name: string; region: string; last_event: string }
interface CovRow    { region: string; sites: number; sites_with_events: number; sites_with_open_alarms: number }

export default function Inventory() {
  const [staleDays, setStaleDays] = useState(30)
  const [orphanQ, setOrphanQ] = useState('')
  const [staleQ,  setStaleQ]  = useState('')

  return (
    <Tabs
      defaultActiveKey="orphans"
      items={[
        {
          key: 'orphans', label: 'Orphan events',
          children: (
            <ProCard>
              <Alert type="warning" showIcon style={{ marginBottom: 12 }}
                message="Site keys appearing in events but missing from dim_site. These need a row in the inventory to be classified by region/municipality." />
              <Input.Search allowClear placeholder="filter by site_key…"
                value={orphanQ} onChange={e => setOrphanQ(e.target.value)}
                style={{ width: 280, marginBottom: 8 }} />
              <ProTable<OrphanRow>
                rowKey="site_key" search={false} options={false}
                params={{ q: orphanQ }}
                request={async (params) => {
                  const d = await api<{items: OrphanRow[]}>(`/api/inventory/orphans`)
                  const q = ((params as any).q || '').toLowerCase()
                  const rows = q ? d.items.filter(r => r.site_key.toLowerCase().includes(q)) : d.items
                  return { data: rows, success: true, total: rows.length }
                }}
                pagination={{ defaultPageSize: 25 }}
                columns={[
                  { title: 'Site key', dataIndex: 'site_key', copyable: true },
                  { title: 'Events',   dataIndex: 'events',   align: 'right',
                    sorter: (a,b) => a.events - b.events, defaultSortOrder: 'descend' },
                  { title: 'Last seen', dataIndex: 'last_seen', render: v => formatTs(v as string) },
                ]}
              />
            </ProCard>
          ),
        },
        {
          key: 'stale', label: 'Silent sites',
          children: (
            <ProCard
              title={`Sites in inventory with no events in the last ${staleDays} days`}
              extra={
                <Select value={staleDays} onChange={setStaleDays} style={{ width: 140 }}
                  options={[7,14,30,60,90].map(d => ({ value: d, label: `${d} days` }))} />
              }>
              <Input.Search allowClear placeholder="filter by site_key, name or region…"
                value={staleQ} onChange={e => setStaleQ(e.target.value)}
                style={{ width: 360, marginBottom: 8 }} />
              <ProTable<StaleRow>
                rowKey="site_key" search={false} options={false}
                params={{ days: staleDays, q: staleQ }}
                request={async (params) => {
                  const days = (params as any).days ?? staleDays
                  const d = await api<{items: StaleRow[]}>(`/api/inventory/stale${qs({ days })}`)
                  const q = ((params as any).q || '').toLowerCase()
                  const rows = q
                    ? d.items.filter(r =>
                        r.site_key.toLowerCase().includes(q) ||
                        (r.name || '').toLowerCase().includes(q) ||
                        (r.region || '').toLowerCase().includes(q))
                    : d.items
                  return { data: rows, success: true, total: rows.length }
                }}
                pagination={{ defaultPageSize: 25 }}
                columns={[
                  { title: 'Site key', dataIndex: 'site_key',
                    render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link> },
                  { title: 'Name', dataIndex: 'name', ellipsis: true },
                  { title: 'Region', dataIndex: 'region', render: v => v ? <Tag>{v as string}</Tag> : '—' },
                  { title: 'Last event ever', dataIndex: 'last_event',
                    render: v => v ? formatTs(v as string) : <i style={{ color:'#999' }}>never</i> },
                ]}
              />
            </ProCard>
          ),
        },
        {
          key: 'coverage', label: 'Region coverage',
          children: (
            <ProCard>
              <Alert type="info" showIcon style={{ marginBottom: 12 }}
                message="Per-region rollup: inventoried sites vs. sites that have actually emitted events vs. sites with active open alarms." />
              <ProTable<CovRow>
                rowKey="region" search={false} options={false}
                request={async () => {
                  const d = await api<{items: CovRow[]}>(`/api/inventory/coverage`)
                  return { data: d.items, success: true, total: d.items.length }
                }}
                pagination={false}
                columns={[
                  { title: 'Region', dataIndex: 'region' },
                  { title: 'Sites',                  dataIndex: 'sites',                  align: 'right',
                    sorter: (a,b) => a.sites - b.sites },
                  { title: 'Sites with events',      dataIndex: 'sites_with_events',      align: 'right',
                    sorter: (a,b) => a.sites_with_events - b.sites_with_events },
                  { title: 'Sites with open alarms', dataIndex: 'sites_with_open_alarms', align: 'right',
                    sorter: (a,b) => a.sites_with_open_alarms - b.sites_with_open_alarms,
                    render: v => (v as number) > 0
                      ? <Tag color="red">{v as number}</Tag>
                      : <Tag color="green">0</Tag> },
                ]}
              />
            </ProCard>
          ),
        },
      ]}
    />
  )
}
