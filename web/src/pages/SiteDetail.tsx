import { useEffect, useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { Card, Row, Col, Statistic, Select, Breadcrumb, Spin, message, Empty } from 'antd'
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts'
import { api } from '../api'

const METRICS = ['u_battery_v','p_load_kw','ac_voltage_v','p_solar_kw','e_total_kwh','e_load_kwh']

export default function SiteDetail() {
  const { siteKey = '' } = useParams()
  const [rel, setRel] = useState<any>(null)
  const [metric, setMetric] = useState('u_battery_v')
  const [series, setSeries] = useState<any[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    setLoading(true)
    api(`/api/sites/${encodeURIComponent(siteKey)}/reliability`)
      .then(setRel).catch((e)=>message.error(String(e))).finally(()=>setLoading(false))
  }, [siteKey])

  useEffect(() => {
    api(`/api/sites/${encodeURIComponent(siteKey)}/measurements?metric=${metric}&hours=168`)
      .then((d)=>setSeries((d.series||[]).map((p:any)=>({ ...p, t: (p.ts||'').slice(5,16) }))))
      .catch(()=>setSeries([]))
  }, [siteKey, metric])

  if (loading) return <Spin style={{ display:'block', margin:'80px auto' }} />
  return (
    <>
      <Breadcrumb style={{ marginBottom: 16 }} items={[{ title: <Link to="/sites">Sites</Link> }, { title: siteKey }]} />
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col xs={12} md={6}><Card><Statistic title="Episodes (30d)" value={rel?.episodes ?? 0} /></Card></Col>
        <Col xs={12} md={6}><Card><Statistic title="Open now" value={rel?.open_now ?? 0} valueStyle={{ color: (rel?.open_now>0)?'#cf1322':undefined }} /></Card></Col>
        <Col xs={12} md={6}><Card><Statistic title="Outage hours (30d)" value={rel?.outage_hours ?? 0} precision={1} /></Card></Col>
        <Col xs={12} md={6}><Card><Statistic title="Avg outage (min)" value={rel?.avg_minutes ?? 0} precision={1} /></Card></Col>
      </Row>
      <Card title="Measurements (last 7 days)" extra={
        <Select value={metric} onChange={setMetric} style={{ width: 180 }} options={METRICS.map(m=>({value:m,label:m}))} />
      }>
        {series.length === 0 ? <Empty description="No measurement data (poller not run for this site yet)" /> :
          <ResponsiveContainer width="100%" height={320}>
            <LineChart data={series}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="t" tick={{ fontSize: 11 }} minTickGap={40} /><YAxis domain={['auto','auto']} />
              <Tooltip /><Line type="monotone" dataKey="value" stroke="#1677ff" dot={false} />
            </LineChart>
          </ResponsiveContainer>}
      </Card>
    </>
  )
}
