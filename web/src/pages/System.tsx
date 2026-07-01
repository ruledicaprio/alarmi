import { useEffect, useState } from 'react'
import { Row, Col, Tabs, Select, Button, Tag, message, Typography } from 'antd'
import { StatisticCard, ProCard, ProDescriptions } from '@ant-design/pro-components'
import { ReloadOutlined } from '@ant-design/icons'
import { api, qs } from '../api'
import { formatTs } from '../utils'

const { Text, Paragraph } = Typography

interface UnitStatus {
  active: string; sub: string; active_since: string; memory_mb: number
}
interface SysStatus {
  db: { size_bytes: number; events: number; measurements: number; open_episodes: number; pg_version: string }
  services: { 'bht-api': UnitStatus; 'bht-poller': UnitStatus; 'postgresql-16': UnitStatus }
  disk_opt: { size: string; used: string; avail: string; use_pct: string } | null
  api_version: string
}
interface SourceRow { source: string; last_ingest: string }

const SERVICES = ['bht-api','bht-poller','postgresql-16','crond'] as const
type Service = typeof SERVICES[number]

function mb(bytes: number) {
  if (bytes < 1024 * 1024) return `${(bytes/1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes/(1024*1024)).toFixed(1)} MB`
  return `${(bytes/(1024*1024*1024)).toFixed(2)} GB`
}

function ActiveTag({ s }: { s: string }) {
  const color = s === 'active' ? 'green' : s === 'failed' ? 'red' : 'orange'
  return <Tag color={color}>{s}</Tag>
}

function snmpPollerHealth(sources: SourceRow[]): { label: string; color: string; ageMin: number | null } {
  const snmpSources = ['u2020', 'rps_sc300', 'rps_sc200', 'baran']
  const latest = sources
    .filter(s => snmpSources.includes(s.source) && s.last_ingest)
    .map(s => new Date(s.last_ingest.replace(' ', 'T')).getTime())
    .sort((a, b) => b - a)[0]
  if (!latest) return { label: 'no data', color: '#aaa', ageMin: null }
  const ageMin = (Date.now() - latest) / 60_000
  if (ageMin < 5)  return { label: 'active',   color: '#52c41a', ageMin }
  if (ageMin < 30) return { label: 'degraded',  color: '#fa8c16', ageMin }
  return { label: 'silent', color: '#cf1322', ageMin }
}

export default function System() {
  const [status, setStatus] = useState<SysStatus | null>(null)
  const [snmpSources, setSnmpSources] = useState<SourceRow[]>([])
  const [svc, setSvc] = useState<Service>('bht-api')
  const [lines, setLines] = useState(100)
  const [log, setLog] = useState('')
  const [loading, setLoading] = useState(false)

  const loadStatus = () => {
    api<SysStatus>('/api/system/status').then(setStatus).catch(e => message.error(String(e)))
    api<{items: SourceRow[]}>('/api/stats/sources').then(d => setSnmpSources(d.items || [])).catch(() => {})
  }
  const loadJournal = () => {
    setLoading(true)
    api<{text: string}>(`/api/system/journal${qs({ service: svc, lines })}`)
      .then(d => setLog(d.text || '(no output)'))
      .catch(e => message.error(String(e)))
      .finally(() => setLoading(false))
  }
  useEffect(() => { loadStatus(); const t = setInterval(loadStatus, 30_000); return () => clearInterval(t) }, [])
  useEffect(loadJournal, [svc, lines])

  if (!status) return <ProCard><Text type="secondary">loading system status…</Text></ProCard>

  return (
    <ProCard ghost gutter={[16,16]} wrap>
      <StatisticCard.Group colSpan={24} direction="row">
        <StatisticCard statistic={{ title: 'bht-api', value: status.services['bht-api'].active,
          description: <ActiveTag s={status.services['bht-api'].sub} /> }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'bht-poller', value: status.services['bht-poller'].active,
          description: <ActiveTag s={status.services['bht-poller'].sub} /> }} />
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'postgresql-16', value: status.services['postgresql-16'].active,
          description: <ActiveTag s={status.services['postgresql-16'].sub} /> }} />
        <StatisticCard.Divider />
        {(() => {
          const h = snmpPollerHealth(snmpSources)
          const desc = h.ageMin != null ? `last ingest ${Math.round(h.ageMin)}m ago` : 'no SNMP events seen'
          return (
            <StatisticCard statistic={{
              title: 'snmp-poller (ext)',
              value: h.label,
              valueStyle: { color: h.color },
              description: <span style={{ fontSize: 11, color: '#888' }}>{desc}</span>,
            }} />
          )
        })()}
        <StatisticCard.Divider />
        <StatisticCard statistic={{ title: 'API version', value: status.api_version }} />
      </StatisticCard.Group>

      <ProCard title="Database (TimescaleDB)" colSpan={{ xs: 24, md: 12 }}>
        <ProDescriptions column={2} size="small"
          dataSource={status.db}
          columns={[
            { title: 'Size on disk',  dataIndex: 'size_bytes',    render: (v: any) => mb(v) },
            { title: 'fact_event',    dataIndex: 'events',        render: (v: any) => v.toLocaleString() },
            { title: 'fact_measurement', dataIndex: 'measurements', render: (v: any) => v.toLocaleString() },
            { title: 'Open episodes', dataIndex: 'open_episodes' },
            { title: 'PG version',    dataIndex: 'pg_version',    span: 2 },
          ] as any}
        />
      </ProCard>

      <ProCard title="Disk usage of /opt" colSpan={{ xs: 24, md: 12 }}>
        {status.disk_opt
          ? <ProDescriptions column={2} size="small" dataSource={status.disk_opt}
              columns={[
                { title: 'Size', dataIndex: 'size' },
                { title: 'Used', dataIndex: 'used' },
                { title: 'Avail', dataIndex: 'avail' },
                { title: 'Use %', dataIndex: 'use_pct' },
              ] as any} />
          : <Text type="secondary">df failed</Text>
        }
      </ProCard>

      <ProCard
        title="Service memory"
        colSpan={24}
        extra={<Button icon={<ReloadOutlined />} onClick={loadStatus}>Refresh</Button>}
      >
        <Row gutter={16}>
          {(Object.keys(status.services) as Array<keyof typeof status.services>).map(name => (
            <Col xs={24} md={8} key={name}>
              <ProDescriptions column={1} size="small"
                title={name}
                dataSource={status.services[name]}
                columns={[
                  { title: 'Active state', dataIndex: 'active' },
                  { title: 'Sub state', dataIndex: 'sub' },
                  { title: 'Active since', dataIndex: 'active_since', render: (v: any) => formatTs(v) },
                  { title: 'Memory', dataIndex: 'memory_mb',
                    render: (v: any) => v >= 0 ? `${v} MB` : '—' },
                ] as any}
              />
            </Col>
          ))}
        </Row>
      </ProCard>

      <ProCard title="journalctl" colSpan={24}
        extra={
          <Row gutter={8} align="middle">
            <Col><Select value={svc} onChange={setSvc} style={{ width: 160 }}
              options={SERVICES.map(s => ({ value: s, label: s }))} /></Col>
            <Col><Select value={lines} onChange={setLines} style={{ width: 100 }}
              options={[50,100,200,500,1000].map(n => ({ value: n, label: `${n} lines` }))} /></Col>
            <Col><Button icon={<ReloadOutlined />} loading={loading} onClick={loadJournal}>Refresh</Button></Col>
          </Row>
        }>
        <Paragraph>
          <pre style={{
            background: 'rgba(127,127,127,0.08)', padding: 12, borderRadius: 4,
            maxHeight: 420, overflow: 'auto', fontSize: 11, lineHeight: 1.4,
            margin: 0, fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
          }}>{log}</pre>
        </Paragraph>
      </ProCard>
    </ProCard>
  )
}
