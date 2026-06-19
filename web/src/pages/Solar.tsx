import { useEffect, useState } from 'react'
import { Empty, Select, Row, Col, Spin, message } from 'antd'
import { ProCard, StatisticCard, ProTable } from '@ant-design/pro-components'
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend } from 'recharts'
import { Link } from 'react-router-dom'
import { api, qs } from '../api'
import { SEV_COLOR } from '../colors'
import { formatTs, formatTsShort } from '../utils'

interface TopProducer { site_key: string; name: string; region: string; power_kw: number }
interface SeriesPt    { bucket: string; avg_kw: number; max_kw: number; sites: number }
interface SolarSummary {
  sites_active_now: number; total_power_kw_now: number;
  top_producers: TopProducer[]; timeseries: SeriesPt[]
}
interface SolarSite {
  site_key: string; name: string; region: string;
  power_kw: number; energy_kwh: number; last_ts: string
}

const POLL_MS = 30_000

export default function Solar() {
  const [hours, setHours] = useState(24)
  const [summary, setSummary] = useState<SolarSummary | null>(null)
  const [sites, setSites] = useState<SolarSite[]>([])
  const [loading, setLoading] = useState(true)

  const load = () => {
    Promise.all([
      api<SolarSummary>(`/api/solar/summary${qs({ hours })}`),
      api<{items: SolarSite[]}>(`/api/solar/sites${qs({ hours })}`),
    ]).then(([s, ss]) => {
      setSummary({
        ...s,
        timeseries: s.timeseries.map(p => ({ ...p, bucket: formatTsShort(p.bucket) })),
      })
      setSites(ss.items || [])
    }).catch(e => message.error(String(e))).finally(() => setLoading(false))
  }
  useEffect(() => {
    setLoading(true); load()
    const t = setInterval(load, POLL_MS)
    return () => clearInterval(t)
  }, [hours])

  if (loading && !summary) return <Spin style={{ display: 'block', margin: '80px auto' }} />
  if (!summary || summary.sites_active_now === 0) return (
    <ProCard>
      <Empty description={
        <>No solar data in the last hour. Either the Modbus poller isn't reading <code>p_solar_kw</code>, or no PV sites are producing right now (night?).</>
      } />
    </ProCard>
  )

  return (
    <ProCard ghost gutter={[16,16]} wrap>
      <ProCard ghost colSpan={24} bodyStyle={{ padding: 0 }}>
        <Row justify="end" style={{ marginBottom: 8 }}>
          <Select value={hours} onChange={setHours} style={{ width: 140 }}
            options={[{value:6,label:'6h'},{value:24,label:'24h'},{value:72,label:'3 days'},{value:168,label:'7 days'}]} />
        </Row>
      </ProCard>

      <StatisticCard.Group colSpan={24} direction="row">
        <StatisticCard statistic={{
          title: 'Active PV sites (last hour)', value: summary.sites_active_now,
          valueStyle: { color: '#52c41a' },
        }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{
          title: 'Current total power', value: summary.total_power_kw_now,
          precision: 1, suffix: 'kW',
        }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{
          title: `Producers > 0 kW`, value: summary.top_producers.length,
        }} />
      </StatisticCard.Group>

      <ProCard title={`Total solar power (last ${hours}h, hourly)`} colSpan={24}>
        <ResponsiveContainer width="100%" height={260}>
          <AreaChart data={summary.timeseries} margin={{ left: 0, right: 8 }}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="bucket" tick={{ fontSize: 11 }} minTickGap={32} />
            <YAxis tick={{ fontSize: 11 }} unit=" kW" />
            <Tooltip />
            <Legend wrapperStyle={{ fontSize: 12 }} />
            <Area type="monotone" dataKey="avg_kw" name="avg sum kW" stroke="#52c41a" fill="#52c41a" fillOpacity={0.4} />
            <Area type="monotone" dataKey="max_kw" name="peak kW"    stroke="#fa8c16" fill="#fa8c16" fillOpacity={0.2} />
          </AreaChart>
        </ResponsiveContainer>
      </ProCard>

      <ProCard title="Top 10 producers (now)" colSpan={{ xs: 24, md: 10 }}>
        <ProTable<TopProducer>
          size="small" search={false} options={false} pagination={false}
          dataSource={summary.top_producers} rowKey="site_key"
          columns={[
            { title: 'Site', dataIndex: 'site_key', render: v => <Link to={`/sites/${encodeURIComponent(v as string)}`}>{v as string}</Link> },
            { title: 'Region', dataIndex: 'region' },
            { title: 'kW', dataIndex: 'power_kw', align: 'right', render: v => (v as number).toFixed(1) },
          ]}
        />
      </ProCard>

      <ProCard title={`All producing sites (${sites.length})`} colSpan={{ xs: 24, md: 14 }}
        extra={<span style={{ color: '#888', fontSize: 12 }}>auto-refresh 30s · solar = p_solar_kw, energy = Δ(e_total_kwh) over window</span>}>
        <ProTable<SolarSite>
          size="small" search={false} options={{ density: false, fullScreen: true, reload: false, setting: true }}
          dataSource={sites} rowKey="site_key"
          pagination={{ pageSize: 20, showSizeChanger: true }}
          columns={[
            { title: 'Site', dataIndex: 'site_key', render: v => <Link to={`/sites/${encodeURIComponent(v as string)}`}>{v as string}</Link> },
            { title: 'Name', dataIndex: 'name', ellipsis: true },
            { title: 'Region', dataIndex: 'region' },
            { title: 'Now (kW)', dataIndex: 'power_kw', align: 'right', sorter: (a,b)=>a.power_kw-b.power_kw, defaultSortOrder: 'descend', render: v => (v as number).toFixed(2) },
            { title: `Energy (kWh, ${hours}h)`, dataIndex: 'energy_kwh', align: 'right', sorter: (a,b)=>a.energy_kwh-b.energy_kwh, render: v => (v as number).toFixed(1) },
            { title: 'Last sample', dataIndex: 'last_ts', render: v => formatTs(v as string) },
          ]}
        />
      </ProCard>
    </ProCard>
  )
}
