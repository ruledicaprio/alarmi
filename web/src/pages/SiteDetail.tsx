import { useEffect, useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { Tabs, Breadcrumb, Spin, Empty, Select, Tag, Button, Space, message } from 'antd'
import { ProCard, StatisticCard, ProTable, ProDescriptions } from '@ant-design/pro-components'
import { SafetyCertificateOutlined } from '@ant-design/icons'
import { LineChart, Line, ScatterChart, Scatter, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, ZAxis, Legend } from 'recharts'
import { api, qs, type Episode, type RecentEvent, ALL_SOURCES } from '../api'
import { formatTs, formatTsShort } from '../utils'
import { SeverityTag, TransitionTag, SourceTag, ClassTag } from '../components/Tags'
import { SEV_COLOR } from '../colors'
import VerifyDrawer from '../components/VerifyDrawer'

const METRICS = ['u_battery_v','p_load_kw','ac_voltage_v','p_solar_kw','e_total_kwh','e_load_kwh','e_solar_day_kwh','inverter_temp_c']

interface Reliability { site_key: string; episodes: number; open_now: number; outage_hours: number; avg_minutes: number }
interface SiteRow    { site_key: string; name: string; region: string; municipality: string; open_alarms: number; last_event: string }
interface IpRow      { ip: string; events: number; last_seen: string }
interface RelatedRow { site_key: string; ip_overlap: number; region: string }
interface VerSummary { last_verified_at: string; last_verified_by: string; events_since: number }
interface VerRow     { id: number; verified_at: string; verified_by: string; notes: string; events_through: string; ip_inventory: string[]; region_confirmed: string }

const SOURCE_Y: Record<string, number> = Object.fromEntries(ALL_SOURCES.map((s, i) => [s, i + 1]))
const SOURCE_TICKS = ALL_SOURCES.map((s, i) => ({ y: i + 1, label: s }))

export default function SiteDetail() {
  const { siteKey = '' } = useParams()
  const [rel, setRel]         = useState<Reliability | null>(null)
  const [site, setSite]       = useState<SiteRow | null>(null)
  const [metric, setMetric]   = useState(METRICS[0])
  const [series, setSeries]   = useState<{t:string; value:number}[]>([])
  const [timeline, setTimeline] = useState<RecentEvent[]>([])
  const [ips, setIps]         = useState<IpRow[]>([])
  const [related, setRelated] = useState<RelatedRow[]>([])
  const [verSum, setVerSum]   = useState<VerSummary | null>(null)
  const [verHist, setVerHist] = useState<VerRow[]>([])
  const [verifyOpen, setVerifyOpen] = useState(false)
  const [loading, setLoading] = useState(true)
  const [timelineHours, setTimelineHours] = useState(168)

  const loadAll = () => {
    setLoading(true)
    Promise.all([
      api<Reliability>(`/api/sites/${encodeURIComponent(siteKey)}/reliability`),
      api<{items: SiteRow[]}>(`/api/sites${qs({ q: siteKey, limit: 5 })}`),
      api<{items: IpRow[]}>(`/api/sites/${encodeURIComponent(siteKey)}/ips`),
      api<{items: RelatedRow[]}>(`/api/sites/${encodeURIComponent(siteKey)}/related`),
      api<VerSummary>(`/api/sites/${encodeURIComponent(siteKey)}/verification/summary`),
      api<{items: VerRow[]}>(`/api/sites/${encodeURIComponent(siteKey)}/verification`),
    ]).then(([r, s, ip, rel, vs, vh]) => {
      setRel(r)
      setSite((s.items || []).find(x => x.site_key === siteKey) || null)
      setIps(ip.items || [])
      setRelated(rel.items || [])
      setVerSum(vs)
      setVerHist(vh.items || [])
    }).catch(e => message.error(String(e))).finally(() => setLoading(false))
  }
  useEffect(loadAll, [siteKey])

  useEffect(() => {
    api<{series: {ts:string; value:number}[]}>(`/api/sites/${encodeURIComponent(siteKey)}/measurements${qs({ metric, hours: 168 })}`)
      .then(d => setSeries((d.series || []).map(p => ({ t: formatTsShort(p.ts), value: p.value }))))
      .catch(() => setSeries([]))
  }, [siteKey, metric])

  useEffect(() => {
    api<{items: RecentEvent[]}>(`/api/sites/${encodeURIComponent(siteKey)}/timeline${qs({ hours: timelineHours, limit: 1000 })}`)
      .then(d => setTimeline(d.items || []))
      .catch(() => setTimeline([]))
  }, [siteKey, timelineHours])

  if (loading) return <Spin style={{ display:'block', margin:'80px auto' }} />

  const scatterPoints = timeline.map(e => ({
    x: new Date(e.event_time).getTime(),
    y: SOURCE_Y[e.source] || 0,
    severity: e.severity, raw: e.raw_alarm, class: e.alarm_class, source: e.source, when: e.event_time,
  }))

  const verifyBadge = !verSum?.last_verified_at
    ? <Tag color="warning">unverified</Tag>
    : <Tag color={verSum.events_since > 0 ? 'orange' : 'green'}>
        verified · {formatTs(verSum.last_verified_at)}
        {verSum.events_since > 0 && ` · ${verSum.events_since} new`}
      </Tag>

  return (
    <>
      <Breadcrumb style={{ marginBottom: 16 }}
        items={[{ title: <Link to="/sites">Sites</Link> }, { title: siteKey }]} />

      <ProCard style={{ marginBottom: 16 }}
        extra={
          <Space>
            {verifyBadge}
            <Button type="primary" icon={<SafetyCertificateOutlined />}
              onClick={() => setVerifyOpen(true)}>Verify data</Button>
          </Space>
        }
      >
        <ProDescriptions column={4} size="small"
          dataSource={site || { site_key: siteKey, name: '', region: '', municipality: '', open_alarms: 0 }}
          columns={[
            { title: 'Site key',     dataIndex: 'site_key',     copyable: true },
            { title: 'Display name', dataIndex: 'name' },
            { title: 'Region',       dataIndex: 'region' },
            { title: 'Municipality', dataIndex: 'municipality' },
            { title: 'Open alarms',  dataIndex: 'open_alarms',
              render: (v: any) => (v as number) > 0 ? <Tag color="red">{v}</Tag> : <Tag color="green">0</Tag> },
            { title: 'Device IPs', dataIndex: 'site_key',
              render: () => ips.length > 0
                ? <Space size={4} wrap>{ips.slice(0, 4).map(i => <Tag key={i.ip}>{i.ip}</Tag>)}{ips.length > 4 && <Tag>+{ips.length - 4}</Tag>}</Space>
                : <span style={{ color: '#999' }}>none</span>
            },
          ] as any}
        />
      </ProCard>

      <Tabs
        defaultActiveKey="overview"
        items={[
          {
            key: 'overview', label: 'Overview',
            children: (
              <ProCard ghost gutter={[16,16]} wrap>
                <StatisticCard.Group colSpan={24} direction="row">
                  <StatisticCard statistic={{ title: 'Episodes (30d)', value: rel?.episodes ?? 0 }} />
                  <StatisticCard.Divider />
                  <StatisticCard statistic={{ title: 'Open now', value: rel?.open_now ?? 0, valueStyle: { color: (rel?.open_now ?? 0) > 0 ? SEV_COLOR.critical : undefined } }} />
                  <StatisticCard.Divider />
                  <StatisticCard statistic={{ title: 'Outage hours (30d)', value: rel?.outage_hours ?? 0, precision: 1 }} />
                  <StatisticCard.Divider />
                  <StatisticCard statistic={{ title: 'Avg outage (min)', value: rel?.avg_minutes ?? 0, precision: 1 }} />
                </StatisticCard.Group>

                <ProCard title="Measurements (last 7 days)" colSpan={24}
                  extra={<Select value={metric} onChange={setMetric} style={{ width: 220 }}
                    options={METRICS.map(m => ({ value: m, label: m }))} />}>
                  {series.length === 0 ? <Empty description="No measurement data (poller hasn't logged this metric for this site)" /> :
                    <ResponsiveContainer width="100%" height={320}>
                      <LineChart data={series}>
                        <CartesianGrid strokeDasharray="3 3" />
                        <XAxis dataKey="t" tick={{ fontSize: 11 }} minTickGap={40} />
                        <YAxis domain={['auto','auto']} tick={{ fontSize: 11 }} />
                        <Tooltip /><Line type="monotone" dataKey="value" stroke="#1677ff" dot={false} />
                      </LineChart>
                    </ResponsiveContainer>}
                </ProCard>
              </ProCard>
            ),
          },
          {
            key: 'timeline', label: 'Timeline (cross-source)',
            children: (
              <ProCard
                title={`Events across all sources at this site (last ${timelineHours}h, ${timeline.length} events)`}
                extra={
                  <Select value={timelineHours} onChange={setTimelineHours} style={{ width: 130 }}
                    options={[{value:24,label:'24h'},{value:168,label:'7d'},{value:720,label:'30d'}]} />
                }
              >
                {timeline.length === 0 ? <Empty /> : (
                  <ResponsiveContainer width="100%" height={Math.max(280, ALL_SOURCES.length * 34)}>
                    <ScatterChart margin={{ left: 100, right: 16, top: 8, bottom: 24 }}>
                      <CartesianGrid strokeDasharray="3 3" />
                      <XAxis dataKey="x" type="number" domain={['auto','auto']}
                        tickFormatter={(t) => new Date(t).toISOString().slice(5, 16).replace('T',' ')}
                        tick={{ fontSize: 11 }} />
                      <YAxis dataKey="y" type="number" domain={[0, ALL_SOURCES.length + 1]}
                        ticks={SOURCE_TICKS.map(s => s.y)}
                        tickFormatter={(y) => SOURCE_TICKS.find(s => s.y === y)?.label || ''}
                        tick={{ fontSize: 11 }} width={120} />
                      <ZAxis range={[40,40]} />
                      <Tooltip content={({ active, payload }) => {
                        if (!active || !payload?.[0]) return null
                        const p = payload[0].payload
                        return (
                          <div style={{ background:'#fff', border:'1px solid #ddd', padding:8, fontSize:12, color: '#000' }}>
                            <div><b>{formatTs(p.when)}</b></div>
                            <div>{p.source} · {p.class}</div>
                            <div>severity: <b style={{ color: SEV_COLOR[p.severity as keyof typeof SEV_COLOR] }}>{p.severity}</b></div>
                            <div style={{ color:'#666' }}>{p.raw}</div>
                          </div>
                        )
                      }} />
                      <Legend wrapperStyle={{ fontSize: 12 }} />
                      {(['critical','major','minor','warning','info'] as const).map(sev => (
                        <Scatter key={sev} name={sev} data={scatterPoints.filter(p => p.severity === sev)}
                          fill={SEV_COLOR[sev]} />
                      ))}
                    </ScatterChart>
                  </ResponsiveContainer>
                )}
              </ProCard>
            ),
          },
          {
            key: 'events', label: 'Events',
            children: (
              <ProTable<RecentEvent>
                rowKey={r => r.event_time + r.alarm_class + r.transition}
                search={false} options={false}
                request={async (params) => {
                  const offset = ((params.current ?? 1) - 1) * (params.pageSize ?? 50)
                  const d = await api<{items: RecentEvent[]; total: number}>(
                    `/api/alarms/recent${qs({ hours: 168 * 4, limit: params.pageSize ?? 50, offset, site: siteKey })}`)
                  return { data: d.items, success: true, total: d.total }
                }}
                pagination={{ defaultPageSize: 50 }}
                columns={[
                  { title: 'Time',  dataIndex: 'event_time', width: 175, render: v => formatTs(v as string) },
                  { title: 'Class', dataIndex: 'alarm_class', render: v => <ClassTag v={v as string} /> },
                  { title: 'Sev',   dataIndex: 'severity',   render: v => <SeverityTag v={v as any} /> },
                  { title: 'Trans', dataIndex: 'transition', render: v => <TransitionTag v={v as any} /> },
                  { title: 'Src',   dataIndex: 'source',     render: v => <SourceTag v={v as any} /> },
                  { title: 'Raw alarm', dataIndex: 'raw_alarm', ellipsis: true },
                  { title: 'IP',    dataIndex: 'device_ip',  width: 130 },
                ]}
              />
            ),
          },
          {
            key: 'episodes', label: 'Episodes',
            children: (
              <ProTable<Episode>
                rowKey={r => r.raised_at + r.alarm_class}
                search={false} options={false}
                request={async () => {
                  const d = await api<{items: Episode[]}>(`/api/sites/${encodeURIComponent(siteKey)}/episodes`)
                  return { data: d.items, success: true, total: d.items.length }
                }}
                pagination={{ defaultPageSize: 50 }}
                columns={[
                  { title: 'Raised', dataIndex: 'raised_at', width: 175, render: v => formatTs(v as string) },
                  { title: 'Cleared', dataIndex: 'cleared_at', width: 175, render: v => v ? formatTs(v as string) : <Tag color="red">open</Tag> },
                  { title: 'Duration (min)', dataIndex: 'duration_seconds', align: 'right',
                    render: v => (v as number) > 0 ? ((v as number) / 60).toFixed(1) : '—' },
                  { title: 'Class', dataIndex: 'alarm_class', render: v => <ClassTag v={v as string} /> },
                  { title: 'Sev',   dataIndex: 'severity',   render: v => <SeverityTag v={v as any} /> },
                  { title: 'Src',   dataIndex: 'source',     render: v => <SourceTag v={v as any} /> },
                ]}
              />
            ),
          },
          {
            key: 'ips', label: `Device IPs (${ips.length})`,
            children: (
              <ProTable<IpRow>
                rowKey="ip" search={false} options={false} pagination={false}
                dataSource={ips}
                columns={[
                  { title: 'IP', dataIndex: 'ip', copyable: true },
                  { title: 'Events seen', dataIndex: 'events', align: 'right' },
                  { title: 'Last seen', dataIndex: 'last_seen', render: v => formatTs(v as string) },
                ]}
              />
            ),
          },
          {
            key: 'related', label: `Related sites (${related.length})`,
            children: (
              related.length === 0 ? <Empty description="No other sites share a /24 subnet with this one" /> :
              <ProTable<RelatedRow>
                rowKey="site_key" search={false} options={false} pagination={false}
                dataSource={related}
                columns={[
                  { title: 'Site',  dataIndex: 'site_key',
                    render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link> },
                  { title: 'Region', dataIndex: 'region' },
                  { title: 'IP-subnet overlaps', dataIndex: 'ip_overlap', align: 'right' },
                ]}
              />
            ),
          },
          {
            key: 'verification', label: `Verification (${verHist.length})`,
            children: (
              <ProTable<VerRow>
                rowKey="id" search={false} options={false} pagination={{ pageSize: 20 }}
                dataSource={verHist}
                columns={[
                  { title: 'Verified at',   dataIndex: 'verified_at', width: 175, render: v => formatTs(v as string) },
                  { title: 'By',            dataIndex: 'verified_by', width: 140 },
                  { title: 'Events through',dataIndex: 'events_through', width: 175, render: v => formatTs(v as string) },
                  { title: 'Region', dataIndex: 'region_confirmed', width: 120 },
                  { title: 'IPs confirmed', dataIndex: 'ip_inventory',
                    render: (_, r) => <Space wrap size={4}>{r.ip_inventory.map(i => <Tag key={i}>{i}</Tag>)}</Space> },
                  { title: 'Notes', dataIndex: 'notes', ellipsis: true },
                ]}
              />
            ),
          },
        ]}
      />

      <VerifyDrawer
        open={verifyOpen} siteKey={siteKey}
        currentRegion={site?.region || ''}
        onClose={() => setVerifyOpen(false)}
        onVerified={loadAll}
      />
    </>
  )
}
