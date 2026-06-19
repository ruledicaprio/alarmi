import { useEffect, useState } from 'react'
import { Drawer, Descriptions, Divider, Empty, Spin, Space, Typography, Tag } from 'antd'
import { Link } from 'react-router-dom'
import { ProTable } from '@ant-design/pro-components'
import { api, qs, type RecentEvent } from '../api'
import { formatTs } from '../utils'
import { SeverityTag, TransitionTag, SourceTag, ClassTag } from './Tags'

const { Text } = Typography

export default function AlarmDrawer({
  open, alarm, onClose,
}: {
  open: boolean
  alarm: RecentEvent | null
  onClose: () => void
}) {
  const [siteEvents, setSiteEvents] = useState<RecentEvent[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!open || !alarm) return
    setLoading(true)
    api<{items: RecentEvent[]}>(
      `/api/sites/${encodeURIComponent(alarm.site_key)}/timeline${qs({ hours: 168, limit: 30 })}`
    ).then(d => setSiteEvents(d.items || []))
     .catch(() => setSiteEvents([]))
     .finally(() => setLoading(false))
  }, [open, alarm?.site_key, alarm?.event_time])

  if (!alarm) return null

  return (
    <Drawer
      title={<Space size="middle">
        <span>{alarm.site_key}</span>
        <SeverityTag v={alarm.severity} />
        <ClassTag v={alarm.alarm_class} />
        <TransitionTag v={alarm.transition} />
      </Space>}
      width={720} open={open} onClose={onClose} placement="right"
    >
      <Descriptions size="small" column={2} bordered
        items={[
          { key:'t',  label:'Time',       children: formatTs(alarm.event_time) },
          { key:'s',  label:'Source',     children: <SourceTag v={alarm.source} /> },
          { key:'ra', label:'Raw alarm',  children: <Text code copyable>{alarm.raw_alarm || '(none)'}</Text>, span:2 },
          { key:'ip', label:'Device IP',  children: alarm.device_ip || '—' },
          { key:'rg', label:'Region',     children: alarm.region || '—' },
          { key:'lk', label:'Site detail', children: <Link to={`/sites/${encodeURIComponent(alarm.site_key)}`}>{alarm.site_key} →</Link>, span:2 },
        ]}
      />

      <Divider orientation="left" plain style={{ marginTop: 24 }}>
        Last 30 events at <Tag>{alarm.site_key}</Tag>
      </Divider>
      {loading ? <Spin /> : siteEvents.length === 0 ? <Empty description="No history" /> :
        <ProTable<RecentEvent>
          size="small" search={false} options={false} pagination={false}
          dataSource={siteEvents} rowKey={(r) => r.event_time + r.alarm_class + r.transition}
          columns={[
            { title:'Time',  dataIndex:'event_time', width:170, render: v => formatTs(v as string) },
            { title:'Class', dataIndex:'alarm_class', render: v => <ClassTag v={v as string} /> },
            { title:'Sev',   dataIndex:'severity',   render: v => <SeverityTag v={v as any} /> },
            { title:'Trans', dataIndex:'transition', render: v => <TransitionTag v={v as any} /> },
            { title:'Src',   dataIndex:'source',     render: v => <SourceTag v={v as any} /> },
            { title:'Raw',   dataIndex:'raw_alarm', ellipsis:true },
          ]}
        />}
    </Drawer>
  )
}
