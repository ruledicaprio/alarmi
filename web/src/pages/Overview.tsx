import { useEffect, useState } from 'react'
import { Row, Col, Card, Statistic, Table, Tag, Select, Spin, message } from 'antd'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts'
import { api, sevColor } from '../api'

export default function Overview() {
  const [hours, setHours] = useState(168)
  const [loading, setLoading] = useState(true)
  const [byClass, setByClass] = useState<any[]>([])
  const [byRegion, setByRegion] = useState<any[]>([])
  const [active, setActive] = useState<any[]>([])
  const [sites, setSites] = useState<number>(0)

  useEffect(() => {
    setLoading(true)
    Promise.all([
      api(`/api/stats/by-class?hours=${hours}`),
      api(`/api/stats/by-region?hours=${hours}`),
      api(`/api/alarms/active`),
      api(`/api/sites`),
    ]).then(([c, r, a, s]) => {
      setByClass(c.items || [])
      setByRegion(r.items || [])
      setActive(a.items || [])
      setSites(s.count || 0)
    }).catch((e) => message.error(String(e))).finally(() => setLoading(false))
  }, [hours])

  const totalEvents = byClass.reduce((n, x) => n + (x.count || 0), 0)

  if (loading) return <Spin style={{ display: 'block', margin: '80px auto' }} />
  return (
    <>
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col xs={12} md={6}><Card><Statistic title="Active alarms" value={active.length} valueStyle={{ color: '#cf1322' }} /></Card></Col>
        <Col xs={12} md={6}><Card><Statistic title={`Events (last ${hours}h)`} value={totalEvents} /></Card></Col>
        <Col xs={12} md={6}><Card><Statistic title="Sites" value={sites} /></Card></Col>
        <Col xs={12} md={6}>
          <Card>
            <div style={{ marginBottom: 4, color: '#888' }}>Window</div>
            <Select value={hours} style={{ width: '100%' }} onChange={setHours}
              options={[{value:24,label:'24 hours'},{value:168,label:'7 days'},{value:720,label:'30 days'},{value:100000,label:'All'}]} />
          </Card>
        </Col>
      </Row>
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col xs={24} lg={12}>
          <Card title="Events by alarm class">
            <ResponsiveContainer width="100%" height={280}>
              <BarChart data={byClass} layout="vertical" margin={{ left: 40 }}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis type="number" /><YAxis type="category" dataKey="alarm_class" width={130} tick={{ fontSize: 11 }} />
                <Tooltip /><Bar dataKey="count" fill="#1677ff" />
              </BarChart>
            </ResponsiveContainer>
          </Card>
        </Col>
        <Col xs={24} lg={12}>
          <Card title="Events by region">
            <ResponsiveContainer width="100%" height={280}>
              <BarChart data={byRegion} margin={{ left: 10 }}>
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis dataKey="region" tick={{ fontSize: 11 }} /><YAxis />
                <Tooltip /><Bar dataKey="count" fill="#52c41a" />
              </BarChart>
            </ResponsiveContainer>
          </Card>
        </Col>
      </Row>
      <Card title={`Active alarms (${active.length})`}>
        <Table size="small" rowKey={(r:any)=>r.site_key+r.alarm_class+r.raised_at} dataSource={active} pagination={{ pageSize: 10 }}
          columns={[
            { title: 'Site', dataIndex: 'site_key' },
            { title: 'Class', dataIndex: 'alarm_class', render: (v:string)=><Tag>{v}</Tag> },
            { title: 'Severity', dataIndex: 'severity', render: (v:string)=><Tag color={sevColor[v]}>{v}</Tag> },
            { title: 'Source', dataIndex: 'source' },
            { title: 'Open (min)', dataIndex: 'open_minutes', render:(v:number)=>v?.toFixed(0) },
            { title: 'Raised', dataIndex: 'raised_at' },
          ]} />
      </Card>
    </>
  )
}
