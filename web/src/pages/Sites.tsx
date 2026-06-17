import { useEffect, useState } from 'react'
import { Card, Table, Tag, Input, message } from 'antd'
import { Link } from 'react-router-dom'
import { api } from '../api'

export default function Sites() {
  const [rows, setRows] = useState<any[]>([])
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(true)
  useEffect(() => {
    api(`/api/sites`).then((d)=>setRows(d.items||[])).catch((e)=>message.error(String(e))).finally(()=>setLoading(false))
  }, [])
  const filtered = q ? rows.filter(r => (r.site_key||'').toLowerCase().includes(q.toLowerCase())) : rows
  return (
    <Card title={`Sites (${rows.length})`} extra={<Input.Search placeholder="filter" onChange={(e)=>setQ(e.target.value)} style={{ width: 220 }} />}>
      <Table size="small" loading={loading} dataSource={filtered} rowKey="site_key" pagination={{ pageSize: 20 }}
        columns={[
          { title: 'Site', dataIndex: 'site_key', render:(v:string)=><Link to={`/sites/${encodeURIComponent(v)}`}>{v}</Link>, sorter:(a:any,b:any)=>a.site_key.localeCompare(b.site_key) },
          { title: 'Name', dataIndex: 'name' },
          { title: 'Region', dataIndex: 'region', render:(v:string)=>v?<Tag>{v}</Tag>:null },
          { title: 'Open alarms', dataIndex: 'open_alarms', sorter:(a:any,b:any)=>a.open_alarms-b.open_alarms, defaultSortOrder:'descend',
            render:(v:number)=>v>0?<Tag color="red">{v}</Tag>:<Tag color="green">0</Tag> },
        ]} />
    </Card>
  )
}
