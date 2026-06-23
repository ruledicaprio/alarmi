import { useEffect, useMemo, useState } from 'react'
import { Select, Spin, Tag, message, Empty, Space, Statistic } from 'antd'
import { ProCard, ProTable } from '@ant-design/pro-components'
import {
  AreaChart, Area, Line, XAxis, YAxis, Tooltip, ResponsiveContainer,
  CartesianGrid, Legend,
} from 'recharts'
import { Link } from 'react-router-dom'
import { api, qs } from '../api'
import { formatTs, formatTsShort, tsSorter } from '../utils'

/* ── types ─────────────────────────────────────── */
interface TopProducer { site_key: string; name: string; region: string; power_kw: number; family: string }
interface SeriesPt    { bucket: string; family: string; avg_kw: number; max_kw: number; sites: number }
interface LoadPt      { bucket: string; avg_kw: number; max_kw: number; sites: number }
interface SolarSummary {
  hours: number
  sites_active_now: number; total_power_kw_now: number
  by_source: { source: string; sites: number; kw_now: number }[]
  top_producers: TopProducer[]; timeseries: SeriesPt[]; load_timeseries: LoadPt[]
}
interface SolarSite {
  site_key: string; name: string; region: string
  power_kw: number; load_kw: number; energy_kwh: number; last_ts: string; family: string
}

/* ── constants ─────────────────────────────────── */
const FAMILY_COLOR: Record<string, string> = {
  eaton: '#52c41a', smartlogger: '#f57e20', unknown: '#8c8c8c',
}
const FAMILY_LABEL: Record<string, string> = {
  eaton: 'Eaton FNE', smartlogger: 'Huawei SmartLogger', unknown: 'Unknown',
}
const REGIONS = ['SARAJEVO', 'TUZLA', 'ZENICA', 'BIHAC', 'MOSTAR', 'TRAVNIK', 'GORAZDE']
const REGION_OPTS = REGIONS.map(r => ({ value: r, label: r }))
const POLL_MS = 30_000

function FamilyTag({ v }: { v: string }) {
  return <Tag color={FAMILY_COLOR[v] || 'default'} style={{ fontSize: 11, fontWeight: 600 }}>{v}</Tag>
}

/* ══════════════════════════════════════════════════
   Component
   ══════════════════════════════════════════════════ */
export default function Solar() {
  const [hours, setHours] = useState(24)
  const [summary, setSummary] = useState<SolarSummary | null>(null)
  const [sites, setSites] = useState<SolarSite[]>([])
  const [loading, setLoading] = useState(true)
  const [lastRefresh, setLastRefresh] = useState('')

  /* filters */
  const [fRegion, setFRegion] = useState<string>()
  const [fFamily, setFFamily] = useState<string>()

  const load = () => {
    Promise.all([
      api<SolarSummary>(`/api/solar/summary${qs({ hours })}`),
      api<{ items: SolarSite[] }>(`/api/solar/sites${qs({ hours })}`),
    ]).then(([s, ss]) => {
      setSummary(s)
      setSites(ss.items || [])
      setLastRefresh(new Date().toLocaleTimeString())
    }).catch(e => message.error(String(e))).finally(() => setLoading(false))
  }
  useEffect(() => {
    setLoading(true); load()
    const t = setInterval(load, POLL_MS)
    return () => clearInterval(t)
  }, [hours])

  /* ── pivot timeseries: rows=buckets, columns=families + consumption ── */
  const { stackedTs, families } = useMemo(() => {
    if (!summary) return { stackedTs: [], families: [] as string[] }
    const familySet = new Set<string>()
    const byBucket = new Map<string, Record<string, number>>()

    for (const p of summary.timeseries) {
      familySet.add(p.family)
      const row = byBucket.get(p.bucket) || {}
      row[p.family] = (row[p.family] || 0) + p.avg_kw * p.sites
      byBucket.set(p.bucket, row)
    }
    // merge consumption into same buckets
    for (const lp of (summary.load_timeseries || [])) {
      const row = byBucket.get(lp.bucket) || {}
      row._load = (row._load || 0) + lp.avg_kw * lp.sites
      byBucket.set(lp.bucket, row)
    }

    const fams = Array.from(familySet).sort()
    const data = Array.from(byBucket.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([bucket, vals]) => ({ bucket: formatTsShort(bucket), ...vals }))

    return { stackedTs: data, families: fams }
  }, [summary])

  /* ── filtered sites ── */
  const filteredSites = useMemo(() => {
    let out = sites
    if (fRegion) out = out.filter(s => s.region === fRegion)
    if (fFamily) out = out.filter(s => s.family === fFamily)
    return out
  }, [sites, fRegion, fFamily])

  /* ── derived stats ── */
  const totalEnergy = useMemo(() =>
    sites.reduce((sum, s) => sum + s.energy_kwh, 0), [sites])

  if (loading && !summary) return <Spin style={{ display: 'block', margin: '80px auto' }} />
  if (!summary) return (
    <ProCard>
      <Empty description="Waiting for solar data…" />
    </ProCard>
  )

  return (
    <ProCard ghost gutter={[16, 16]} wrap>
      {/* ── header row ── */}
      <ProCard ghost colSpan={24} bodyStyle={{ padding: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
          <span style={{ color: '#888', fontSize: 12 }}>
            auto-refresh 30s · last update {lastRefresh || '—'}
          </span>
          <Select value={hours} onChange={setHours} style={{ width: 140 }}
            options={[
              { value: 6, label: '6h' }, { value: 24, label: '24h' },
              { value: 72, label: '3 days' }, { value: 168, label: '7 days' },
            ]} />
        </div>
      </ProCard>

      {/* ── summary cards ── */}
      <ProCard ghost colSpan={24} gutter={12} wrap>
        <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered>
          <Statistic title="Active PV sites" value={summary.sites_active_now}
            valueStyle={{ color: '#52c41a' }} />
        </ProCard>
        <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered>
          <Statistic title="Total power now" value={summary.total_power_kw_now}
            precision={1} suffix="kW" />
        </ProCard>
        <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered>
          <Statistic title={`Energy yield (${hours}h)`} value={totalEnergy}
            precision={1} suffix="kWh" valueStyle={{ color: '#1677ff' }} />
        </ProCard>
        {summary.by_source.map(s => (
          <ProCard key={s.source} colSpan={{ xs: 12, sm: 8, md: 4 }} bordered>
            <Statistic
              title={<><FamilyTag v={s.source} /> now</>}
              value={s.kw_now} precision={1}
              suffix={<span style={{ fontSize: 13 }}>kW · {s.sites} sites</span>}
            />
          </ProCard>
        ))}
      </ProCard>

      {/* ── production + consumption chart ── */}
      <ProCard title={`Solar production vs. consumption (last ${hours}h)`} colSpan={24}>
        {stackedTs.length === 0 ? <Empty /> : (
          <ResponsiveContainer width="100%" height={300}>
            <AreaChart data={stackedTs} margin={{ left: 0, right: 8 }}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="bucket" tick={{ fontSize: 11 }} minTickGap={40} />
              <YAxis tick={{ fontSize: 11 }} unit=" kW" />
              <Tooltip
                formatter={(value: number, name: string) => [`${value.toFixed(1)} kW`, name]}
                labelStyle={{ fontWeight: 600 }}
              />
              <Legend wrapperStyle={{ fontSize: 12 }} />
              {families.map(f => (
                <Area key={f} type="monotone" stackId="1" dataKey={f}
                  name={FAMILY_LABEL[f] || f}
                  stroke={FAMILY_COLOR[f] || '#8c8c8c'}
                  fill={FAMILY_COLOR[f] || '#8c8c8c'}
                  fillOpacity={0.5} />
              ))}
              <Line type="monotone" dataKey="_load"
                name="Consumption (P load)" stroke="#ff4d4f"
                strokeWidth={2} dot={false} strokeDasharray="5 3" />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </ProCard>

      {/* ── filter bar + full sites table ── */}
      <ProCard
        title={`All FNE sites (${filteredSites.length})`}
        colSpan={24}
        extra={
          <Space wrap>
            <Select allowClear placeholder="Region" value={fRegion} onChange={setFRegion}
              style={{ width: 150 }} options={REGION_OPTS} showSearch />
            <Select allowClear placeholder="Family" value={fFamily} onChange={setFFamily}
              style={{ width: 150 }}
              options={[
                { value: 'eaton', label: 'Eaton FNE' },
                { value: 'smartlogger', label: 'SmartLogger' },
                { value: 'unknown', label: 'Unknown' },
              ]} />
          </Space>
        }
      >
        <ProTable<SolarSite>
          size="small" search={false}
          options={{ density: false, fullScreen: true, reload: false, setting: true }}
          dataSource={filteredSites} rowKey="site_key"
          pagination={{ pageSize: 25, showSizeChanger: true }}
          columns={[
            {
              title: 'Site', dataIndex: 'site_key',
              render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link>,
              sorter: (a, b) => a.site_key.localeCompare(b.site_key),
            },
            {
              title: 'Family', dataIndex: 'family', width: 120,
              render: (_, r) => <FamilyTag v={r.family} />,
            },
            {
              title: 'Region', dataIndex: 'region', width: 110,
              sorter: (a, b) => (a.region || '').localeCompare(b.region || ''),
              render: v => (v as string) || <span style={{ color: '#ccc' }}>—</span>,
            },
            {
              title: 'Solar (kW)', dataIndex: 'power_kw', align: 'right', width: 110,
              sorter: (a, b) => a.power_kw - b.power_kw, defaultSortOrder: 'descend',
              render: v => <span style={{ fontWeight: 600, color: '#52c41a' }}>{(v as number).toFixed(2)}</span>,
            },
            {
              title: 'Load (kW)', dataIndex: 'load_kw', align: 'right', width: 110,
              sorter: (a, b) => a.load_kw - b.load_kw,
              render: v => <span style={{ color: '#ff4d4f' }}>{(v as number).toFixed(2)}</span>,
            },
            {
              title: `Energy (kWh, ${hours}h)`, dataIndex: 'energy_kwh', align: 'right', width: 150,
              sorter: (a, b) => a.energy_kwh - b.energy_kwh,
              render: v => (v as number).toFixed(1),
            },
            {
              title: 'Last sample', dataIndex: 'last_ts', width: 175,
              sorter: tsSorter<SolarSite>('last_ts'),
              render: v => formatTs(v),
            },
          ]}
        />
      </ProCard>
    </ProCard>
  )
}
