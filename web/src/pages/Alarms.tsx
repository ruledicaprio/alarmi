import { useEffect, useState } from 'react'
import { Card, Table, Tag, Select, Input, Space, Button, message } from 'antd'
import { api, sevColor } from '../api'

export default function Alarms() {
  const [hours, setHours] = useState(24)
  const [source, setSource] = useState('')
  const [klass, setKlass] = useState('')
  const [site, setSite] = useState('')
  const [rows, setRows] = useState<any[]>([])
  const [loading, setLoading] = useState(false)

  const load = () => {
    setLoading(true)
    const q = new URLSearchParams({ hours: String(hours), limit: '500' })
    if (source) q.set('source', source)
    if (klass) q.set('class', klass)
    if (site) q.set('site', site)
    api(`/api/alarms/recent?${q.toString()}`)
      .then((d) => setRows(d.items || []))
      .catch((e) => message.error(String(e)))
      .finally(() => setLoading(false))
  }
  useEffect(() => { load() }, [hours, source, klass])

  return (
    <Card title="Recent alarms" extra={
      <Space wrap>
        <Select value={hours} onChange={setHours} style={{ width: 120 }}
          options={[{value:1,label:'1h'},{value:24,label:'24h'},{value:168,label:'7d'},{value:720,label:'30d'},{value:100000,label:'All'}]} />
        <Select value={source} onChange={setSource} style={{ width: 150 }} placeholder="source" allowClear
          options={['ignition','net_eco','u2020','rps_sc200','rps_sc300','dse74xx','benning','baran','modbus_eaton','html_oos'].map(v=>({value:v,label:v}))} />
        <Select value={klass} onChange={setKlass} style={{ width: 170 }} placeholder="class" allowClear
          options={['MAINS_FAILURE','RECTIFIER_FAILURE','BATTERY_LOW','BATTERY_FAULT','COMMS_LOST','NE_DISCONNECTED','COOLING_FAULT','GENSET_EVENT','SOLAR_FAULT','UPS_MODULE','HIGH_VOLTAGE','FUSE_LOAD','GENERIC_ERROR'].map(v=>({value:v,label:v}))} />
        <Input.Search value={site} onChange={(e)=>setSite(e.target.value)} onSearch={load} placeholder="site_key" style={{ width: 160 }} />
        <Button onClick={load} type="primary">Refresh</Button>
      </Space>
    }>
      <Table size="small" loading={loading} dataSource={rows} pagination={{ pageSize: 20 }}
        rowKey={(r:any)=>r.event_time+r.site_key+r.alarm_class+r.transition}
        columns={[
          { title: 'Time', dataIndex: 'event_time', width: 180 },
          { title: 'Site', dataIndex: 'site_key' },
          { title: 'Class', dataIndex: 'alarm_class', render:(v:string)=><Tag>{v}</Tag> },
          { title: 'Sev', dataIndex: 'severity', render:(v:string)=><Tag color={sevColor[v]}>{v}</Tag> },
          { title: 'Transition', dataIndex: 'transition', render:(v:string)=><Tag color={v==='clear'?'green':v==='raise'?'red':'default'}>{v}</Tag> },
          { title: 'Source', dataIndex: 'source' },
          { title: 'Raw', dataIndex: 'raw_alarm', ellipsis: true },
          { title: 'IP', dataIndex: 'device_ip', width: 120 },
        ]} />
    </Card>
  )
}
