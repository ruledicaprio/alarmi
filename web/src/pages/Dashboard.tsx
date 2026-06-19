import { useEffect, useState } from 'react'
import { Row, Select, Spin, message, Tag } from 'antd'
import { StatisticCard, ProCard, ProTable } from '@ant-design/pro-components'
import { AreaChart, Area, BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend } from 'recharts'
import { Link } from 'react-router-dom'
import { api, qs } from '../api'
import { SEV_COLOR } from '../colors'
import { SeverityTag, ClassTag, SourceTag } from '../components/Tags'
import { formatTs, formatTsShort } from '../utils'

interface BucketRow { bucket: string; n: number; critical: number; major: number; other: number }
interface ClassRow  { alarm_class: string; count: number }
interface RegionRow { region: string; count: number }
interface SourceRow { source: string; events_24h: number; last_event: string; last_ingest: string }
interface ActiveRow { site_key: string; source: string; alarm_class: string; severity: any; raised_at: string; open_minutes: number }

const POLL_MS = 30_000

export default function Dashboard() {
  const [hours, setHours] = useState(168)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [lastRefresh, setLastRefresh] = useState<string>('')
  const [series, setSeries] = useState<BucketRow[]>([])
  const [byClass, setByClass] = useState<ClassRow[]>([])
  const [byRegion, setByRegion] = useState<RegionRow[]>([])
  const [sources, setSources] = useState<SourceRow[]>([])
  const [active, setActive] = useState<ActiveRow[]>([])
  const [sitesTotal, setSitesTotal] = useState(0)

  const load = (showSpinner: boolean) => {
    if (showSpinner) setLoading(true); else setRefreshing(true)
    const bucket = hours <= 168 ? 'hour' : 'day'
    Promise.all([
      api<{items: BucketRow[]}>(`/api/stats/timeseries${qs({ hours, bucket })}`),
      api<{items: ClassRow[]}>(`/api/stats/by-class${qs({ hours })}`),
      api<{items: RegionRow[]}>(`/api/stats/by-region${qs({ hours })}`),
      api<{items: SourceRow[]}>(`/api/stats/sources`),
      api<{items: ActiveRow[]}>(`/api/alarms/active`),
      api<{total: number}>(`/api/sites${qs({ limit: 1 })}`),
    ]).then(([ts, c, r, s, a, st]) => {
      setSeries((ts.items || []).map(b => ({ ...b, bucket: formatTsShort(b.bucket) })))
      setByClass(c.items || [])
      setByRegion(r.items || [])
      setSources(s.items || [])
      setActive(a.items || [])
      setSitesTotal(st.total || 0)
      setLastRefresh(new Date().toLocaleTimeString())
    }).catch(e => message.error(String(e))).finally(() => {
      setLoading(false); setRefreshing(false)
    })
  }
  useEffect(() => {
    load(true)
    const t = setInterval(() => load(false), POLL_MS)
    return () => clearInterval(t)
  }, [hours])

  const totalEvents = byClass.reduce((n, x) => n + x.count, 0)
  const criticalActive = active.filter(a => a.severity === 'critical').length

  if (loading && series.length === 0) return <Spin style={{ display: 'block', margin: '80px auto' }} />

  return (
    <ProCard ghost gutter={[16, 16]} wrap>
      <ProCard ghost colSpan={24} bodyStyle={{ padding: 0 }}>
        <Row justify="space-between" align="middle" style={{ marginBottom: 8 }}>
          <span style={{ color: '#888', fontSize: 12 }}>
            Auto-refresh every 30s · last update {lastRefresh || '—'} {refreshing && '· refreshing…'}
          </span>
          <Select value={hours} onChange={setHours} style={{ width: 140 }}
            options={[{value:24,label:'24 hours'},{value:72,label:'3 days'},{value:168,label:'7 days'},{value:720,label:'30 days'}]} />
        </Row>
      </ProCard>

      <StatisticCard.Group colSpan={24} direction="row">
        <StatisticCard statistic={{ title: 'Active alarms', value: active.length, valueStyle: { color: active.length ? SEV_COLOR.major : undefined } }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'Critical now', value: criticalActive, valueStyle: { color: criticalActive ? SEV_COLOR.critical : undefined } }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: `Events (last ${hours}h)`, value: totalEvents }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'Sources active (24h)', value: sources.length, suffix: '/ 10' }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'Sites tracked', value: sitesTotal }} />
      </StatisticCard.Group>

      <ProCard title={`Event rate (${hours <= 168 ? 'per hour' : 'per day'})`} colSpan={24}>
        <ResponsiveContainer width="100%" height={240}>
          <AreaChart data={series} margin={{ left: 0, right: 8 }}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="bucket" tick={{ fontSize: 11 }} minTickGap={32} />
            <YAxis tick={{ fontSize: 11 }} />
            <Tooltip />
            <Legend wrapperStyle={{ fontSize: 12 }} />
            <Area type="monotone" stackId="1" dataKey="critical" name="critical" stroke={SEV_COLOR.critical} fill={SEV_COLOR.critical} fillOpacity={0.7} />
            <Area type="monotone" stackId="1" dataKey="major"    name="major"    stroke={SEV_COLOR.major}    fill={SEV_COLOR.major}    fillOpacity={0.6} />
            <Area type="monotone" stackId="1" dataKey="other"    name="minor/warning/info" stroke={SEV_COLOR.info} fill={SEV_COLOR.info} fillOpacity={0.4} />
          </AreaChart>
        </ResponsiveContainer>
      </ProCard>

      <ProCard title="Events by alarm class" colSpan={{ xs: 24, md: 12 }}>
        <ResponsiveContainer width="100%" height={300}>
          <BarChart data={byClass} layout="vertical" margin={{ left: 24 }}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis type="number" tick={{ fontSize: 11 }} />
            <YAxis type="category" dataKey="alarm_class" width={150} tick={{ fontSize: 11 }} />
            <Tooltip /><Bar dataKey="count" fill="#1677ff" />
          </BarChart>
        </ResponsiveContainer>
      </ProCard>

      <ProCard title="Events by region" colSpan={{ xs: 24, md: 12 }}>
        <ResponsiveContainer width="100%" height={300}>
          <BarChart data={byRegion}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="region" tick={{ fontSize: 11 }} />
            <YAxis tick={{ fontSize: 11 }} />
            <Tooltip /><Bar dataKey="count" fill="#52c41a" />
          </BarChart>
        </ResponsiveContainer>
      </ProCard>

      <ProCard title="Sources (last 24h)" colSpan={{ xs: 24, md: 12 }}>
        <ProTable<SourceRow>
          size="small" search={false} options={false} pagination={false}
          dataSource={sources} rowKey="source"
          columns={[
            { title: 'Source', dataIndex: 'source', render: v => <SourceTag v={v as any} /> },
            { title: 'Events (24h)', dataIndex: 'events_24h', align: 'right' },
            { title: 'Last event',  dataIndex: 'last_event',  render: v => formatTs(v as string) },
            { title: 'Last ingest', dataIndex: 'last_ingest', render: v => formatTs(v as string) },
          ]}
        />
      </ProCard>

      <ProCard title={<>Active alarms <Tag color={active.length ? 'red' : 'green'}>{active.length}</Tag></>} colSpan={{ xs: 24, md: 12 }}>
        <ProTable<ActiveRow>
          size="small" search={false} options={false}
          pagination={{ pageSize: 8, simple: true }}
          dataSource={active} rowKey={r => r.site_key + r.alarm_class + r.raised_at}
          columns={[
            { title: 'Site',     dataIndex: 'site_key',    render: v => <Link to={`/sites/${encodeURIComponent(v as string)}`}>{v as string}</Link> },
            { title: 'Class',    dataIndex: 'alarm_class', render: v => <ClassTag v={v as string} /> },
            { title: 'Sev',      dataIndex: 'severity',    render: v => <SeverityTag v={v as any} /> },
            { title: 'Open min', dataIndex: 'open_minutes', align: 'right', render: v => (v as number).toFixed(0) },
          ]}
        />
      </ProCard>
    </ProCard>
  )
}
