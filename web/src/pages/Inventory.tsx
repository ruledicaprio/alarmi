import { useState, useRef, useCallback } from 'react'
import {
  Tabs, Alert, Select, Input, Tag, Button, Modal, Form, Switch,
  Space, Popconfirm, message, Checkbox, InputNumber, Drawer, Statistic,
} from 'antd'
import {
  PlusOutlined, EditOutlined, DeleteOutlined,
  CheckCircleOutlined, ImportOutlined, FormOutlined,
} from '@ant-design/icons'
import { ProTable, ProCard } from '@ant-design/pro-components'
import type { ActionType, ProColumns } from '@ant-design/pro-components'
import { Link } from 'react-router-dom'
import { api, qs } from '../api'
import { formatTs } from '../utils'

/* ── pre-v8 types ─────────────────────────────── */
interface OrphanRow { site_key: string; events: number; last_seen: string }
interface StaleRow  { site_key: string; name: string; region: string; last_event: string }
interface CovRow    { region: string; sites: number; sites_with_events: number; sites_with_open_alarms: number }

/* ── v8 types ─────────────────────────────────── */
type Health = 'ok' | 'degraded' | 'dead' | 'stale' | 'never'

interface DeviceSummary {
  total: number; ok: number; degraded: number; dead: number; stale: number; never: number; disabled: number
}

interface DeviceRow {
  id: number; ip: string; port: number; unit_id: number
  site_key: string; site_name: string | null; region: string | null
  dev_type: string; fne: boolean; enabled: boolean
  name: string | null; fail_streak: number
  last_polled: string | null; last_ok: string | null
  health: Health; added_by: string | null; updated_at: string
}

interface DeviceListResp {
  summary: DeviceSummary; total: number; count: number; items: DeviceRow[]
}

interface DeviceOrphanRow {
  ip: string; site_key: string; event_count: number; last_seen: string; source: string
}

interface StubSiteRow {
  site_key: string; display_name: string | null; event_count: number
  device_count: number; first_seen: string; updated_at: string
}

/* ── constants ────────────────────────────────── */
const HEALTH_COLOR: Record<Health, string> = {
  ok: 'green', degraded: 'orange', dead: 'red', stale: 'default', never: 'default',
}

const REGIONS = ['SARAJEVO', 'TUZLA', 'ZENICA', 'BIHAC', 'MOSTAR', 'TRAVNIK', 'GORAZDE']
const REGION_OPTS = REGIONS.map(r => ({ value: r, label: r }))

const DEV_TYPES = ['eaton', 'smartlogger']

/* ══════════════════════════════════════════════════════════════
   Component
   ══════════════════════════════════════════════════════════════ */
export default function Inventory() {
  /* ── pre-v8 state ── */
  const [staleDays, setStaleDays] = useState(30)
  const [orphanQ, setOrphanQ] = useState('')
  const [staleQ, setStaleQ] = useState('')

  /* ── Device fleet state ── */
  const fleetRef = useRef<ActionType>()
  const [fleetSummary, setFleetSummary] = useState<DeviceSummary | null>(null)
  const [fRegion, setFRegion] = useState<string>()
  const [fHealth, setFHealth] = useState<string>()
  const [fType, setFType] = useState<string>()
  const [fSearch, setFSearch] = useState('')

  /* ── Add / Edit device modal ── */
  const [deviceModalOpen, setDeviceModalOpen] = useState(false)
  const [editingDevice, setEditingDevice] = useState<DeviceRow | null>(null)
  const [deviceForm] = Form.useForm()

  /* ── Orphan claim modal ── */
  const orphanRef = useRef<ActionType>()
  const [claimOpen, setClaimOpen] = useState(false)
  const [claimingOrphan, setClaimingOrphan] = useState<DeviceOrphanRow | null>(null)
  const [claimForm] = Form.useForm()

  /* ── Stub enrich drawer ── */
  const stubRef = useRef<ActionType>()
  const [enrichOpen, setEnrichOpen] = useState(false)
  const [enrichingSite, setEnrichingSite] = useState<StubSiteRow | null>(null)
  const [enrichForm] = Form.useForm()

  /* ── helpers ── */
  const reloadFleet = useCallback(() => fleetRef.current?.reload(), [])

  const openAddDevice = () => {
    setEditingDevice(null)
    deviceForm.resetFields()
    deviceForm.setFieldsValue({ port: 502, unit_id: 1, dev_type: 'eaton', enabled: true })
    setDeviceModalOpen(true)
  }

  const openEditDevice = (r: DeviceRow) => {
    setEditingDevice(r)
    deviceForm.setFieldsValue({
      ip: r.ip, site_key: r.site_key, port: r.port, unit_id: r.unit_id,
      dev_type: r.dev_type, name: r.name, fne: r.fne, enabled: r.enabled, notes: '',
    })
    setDeviceModalOpen(true)
  }

  const submitDevice = async () => {
    const vals = await deviceForm.validateFields()
    if (editingDevice) {
      await api(`/api/inventory/devices/${editingDevice.id}`, {
        method: 'PATCH', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(vals),
      })
      message.success('Device updated')
    } else {
      await api('/api/inventory/devices', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(vals),
      })
      message.success('Device added')
    }
    setDeviceModalOpen(false)
    reloadFleet()
  }

  const deleteDevice = async (id: number) => {
    await api(`/api/inventory/devices/${id}`, { method: 'DELETE' })
    message.success('Device deleted')
    reloadFleet()
  }

  const toggleEnabled = async (r: DeviceRow) => {
    await api(`/api/inventory/devices/${r.id}`, {
      method: 'PATCH', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled: !r.enabled }),
    })
    reloadFleet()
  }

  const submitClaim = async () => {
    const vals = await claimForm.validateFields()
    await api('/api/inventory/device-orphans/claim', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ...vals, ip: claimingOrphan!.ip }),
    })
    message.success(`Claimed ${claimingOrphan!.ip}`)
    setClaimOpen(false)
    orphanRef.current?.reload()
    reloadFleet()
  }

  const submitEnrich = async () => {
    const vals = await enrichForm.validateFields()
    // strip undefined/empty values
    const body: Record<string, any> = {}
    for (const [k, v] of Object.entries(vals)) {
      if (v !== undefined && v !== null && v !== '') body[k] = v
    }
    await api(`/api/sites/${encodeURIComponent(enrichingSite!.site_key)}`, {
      method: 'PATCH', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    message.success(`Site ${enrichingSite!.site_key} enriched`)
    setEnrichOpen(false)
    stubRef.current?.reload()
  }

  /* ── Device fleet columns ── */
  const fleetCols: ProColumns<DeviceRow>[] = [
    { title: 'IP', dataIndex: 'ip', width: 140, copyable: true },
    {
      title: 'Site', dataIndex: 'site_key', width: 160, ellipsis: true,
      render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link>,
    },
    { title: 'Region', dataIndex: 'region', width: 100, render: v => v ? <Tag>{v as string}</Tag> : '—' },
    { title: 'Type', dataIndex: 'dev_type', width: 100 },
    {
      title: 'Health', dataIndex: 'health', width: 100,
      render: (_, r) => <Tag color={HEALTH_COLOR[r.health]}>{r.health}</Tag>,
    },
    {
      title: 'Last OK', dataIndex: 'last_ok', width: 170,
      render: v => v ? formatTs(v as string) : <i style={{ color: '#999' }}>never</i>,
    },
    {
      title: 'Fails', dataIndex: 'fail_streak', width: 70, align: 'right',
      render: v => (v as number) > 0 ? <Tag color="red">{v as number}</Tag> : 0,
    },
    {
      title: 'Enabled', dataIndex: 'enabled', width: 80, align: 'center',
      render: (_, r) => <Switch size="small" checked={r.enabled} onChange={() => toggleEnabled(r)} />,
    },
    { title: 'Name', dataIndex: 'name', width: 160, ellipsis: true, render: v => v || '—' },
    {
      title: 'Actions', width: 100, valueType: 'option',
      render: (_, r) => (
        <Space size={4}>
          <Button type="link" size="small" icon={<EditOutlined />} onClick={() => openEditDevice(r)} />
          <Popconfirm title={`Delete device ${r.ip}?`} onConfirm={() => deleteDevice(r.id)}>
            <Button type="link" size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ]

  /* ══════════════════════════════════════════════════════════════
     Render
     ══════════════════════════════════════════════════════════════ */
  return (
    <>
      <Tabs
        defaultActiveKey="fleet"
        items={[
          /* ─── Tab: Device fleet ─── */
          {
            key: 'fleet', label: 'Device fleet',
            children: (
              <ProCard direction="column" ghost gutter={[0, 12]}>
                {/* summary cards */}
                {fleetSummary && (
                  <ProCard ghost gutter={12} wrap>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Total"    value={fleetSummary.total} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="OK"       value={fleetSummary.ok}       valueStyle={{ color: '#52c41a' }} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Degraded" value={fleetSummary.degraded} valueStyle={{ color: '#faad14' }} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Dead"     value={fleetSummary.dead}     valueStyle={{ color: '#ff4d4f' }} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Stale"    value={fleetSummary.stale}    valueStyle={{ color: '#999' }} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Never"    value={fleetSummary.never} /></ProCard>
                    <ProCard colSpan={{ xs: 12, sm: 8, md: 4 }} bordered><Statistic title="Disabled" value={fleetSummary.disabled} valueStyle={{ color: '#999' }} /></ProCard>
                  </ProCard>
                )}

                {/* filter bar */}
                <ProCard ghost>
                  <Space wrap style={{ marginBottom: 8 }}>
                    <Select allowClear placeholder="Region" value={fRegion} onChange={setFRegion}
                      style={{ width: 160 }} options={REGION_OPTS} popupMatchSelectWidth={false}
                      showSearch />
                    <Select allowClear placeholder="Health" value={fHealth} onChange={setFHealth}
                      style={{ width: 130 }}
                      options={['ok','degraded','dead','stale','never'].map(h => ({ value: h, label: h }))} />
                    <Select allowClear placeholder="Type" value={fType} onChange={setFType}
                      style={{ width: 140 }}
                      options={DEV_TYPES.map(t => ({ value: t, label: t }))} />
                    <Input.Search allowClear placeholder="search IP, name, site…"
                      value={fSearch} onChange={e => setFSearch(e.target.value)}
                      style={{ width: 240 }} />
                    <Button type="primary" icon={<PlusOutlined />} onClick={openAddDevice}>
                      Add device
                    </Button>
                  </Space>
                </ProCard>

                {/* table */}
                <ProTable<DeviceRow>
                  actionRef={fleetRef}
                  rowKey="id" search={false} options={false}
                  params={{ region: fRegion, health: fHealth, dev_type: fType, q: fSearch }}
                  request={async (params) => {
                    const p = params as any
                    const resp = await api<DeviceListResp>(
                      `/api/inventory/devices${qs({
                        region: p.region, health: p.health, dev_type: p.dev_type,
                        q: p.q, page: p.current, page_size: p.pageSize,
                      })}`,
                    )
                    setFleetSummary(resp.summary)
                    return { data: resp.items, success: true, total: resp.total }
                  }}
                  pagination={{ defaultPageSize: 50, showSizeChanger: true }}
                  columns={fleetCols}
                  scroll={{ x: 1200 }}
                />
              </ProCard>
            ),
          },

          /* ─── Tab: Device orphans ─── */
          {
            key: 'device-orphans', label: 'Device orphans',
            children: (
              <ProCard>
                <Alert type="warning" showIcon style={{ marginBottom: 12 }}
                  message="IPs seen in events but missing from the device inventory. Claim them to start monitoring." />
                <ProTable<DeviceOrphanRow>
                  actionRef={orphanRef}
                  rowKey="ip" search={false} options={false}
                  request={async () => {
                    const d = await api<{ count: number; items: DeviceOrphanRow[] }>('/api/inventory/device-orphans')
                    return { data: d.items, success: true, total: d.count }
                  }}
                  pagination={{ defaultPageSize: 25 }}
                  columns={[
                    { title: 'IP', dataIndex: 'ip', width: 140, copyable: true },
                    { title: 'Site key', dataIndex: 'site_key', width: 180 },
                    { title: 'Events', dataIndex: 'event_count', width: 100, align: 'right',
                      sorter: (a, b) => a.event_count - b.event_count, defaultSortOrder: 'descend' },
                    { title: 'Last seen', dataIndex: 'last_seen', width: 180,
                      render: v => formatTs(v as string) },
                    { title: 'Source', dataIndex: 'source', width: 130 },
                    {
                      title: 'Action', width: 100, valueType: 'option',
                      render: (_, r) => (
                        <Button type="link" size="small" icon={<ImportOutlined />}
                          onClick={() => {
                            setClaimingOrphan(r)
                            claimForm.resetFields()
                            claimForm.setFieldsValue({
                              site_key: r.site_key, port: 502, unit_id: 1, dev_type: 'eaton',
                            })
                            setClaimOpen(true)
                          }}>
                          Claim
                        </Button>
                      ),
                    },
                  ]}
                />
              </ProCard>
            ),
          },

          /* ─── Tab: Stub sites ─── */
          {
            key: 'stubs', label: 'Stub sites',
            children: (
              <ProCard>
                <Alert type="info" showIcon style={{ marginBottom: 12 }}
                  message="Sites auto-created when events arrived with an unknown site_key. Enrich them with region, name, and equipment flags." />
                <ProTable<StubSiteRow>
                  actionRef={stubRef}
                  rowKey="site_key" search={false} options={false}
                  request={async (params) => {
                    const p = params as any
                    const d = await api<{ total: number; count: number; items: StubSiteRow[] }>(
                      `/api/inventory/stubs${qs({ page: p.current, page_size: p.pageSize })}`,
                    )
                    return { data: d.items, success: true, total: d.total }
                  }}
                  pagination={{ defaultPageSize: 25 }}
                  columns={[
                    { title: 'Site key', dataIndex: 'site_key', width: 180, copyable: true },
                    { title: 'Events', dataIndex: 'event_count', width: 100, align: 'right',
                      sorter: (a, b) => a.event_count - b.event_count, defaultSortOrder: 'descend' },
                    { title: 'Devices', dataIndex: 'device_count', width: 90, align: 'right' },
                    { title: 'First seen', dataIndex: 'first_seen', width: 180,
                      render: v => formatTs(v as string) },
                    {
                      title: 'Action', width: 100, valueType: 'option',
                      render: (_, r) => (
                        <Button type="link" size="small" icon={<FormOutlined />}
                          onClick={() => {
                            setEnrichingSite(r)
                            enrichForm.resetFields()
                            enrichForm.setFieldsValue({ display_name: r.display_name })
                            setEnrichOpen(true)
                          }}>
                          Enrich
                        </Button>
                      ),
                    },
                  ]}
                />
              </ProCard>
            ),
          },

          /* ─── Tab: Orphan events (pre-v8) ─── */
          {
            key: 'orphans', label: 'Orphan events',
            children: (
              <ProCard>
                <Alert type="warning" showIcon style={{ marginBottom: 12 }}
                  message="Site keys appearing in events but missing from dim_site. These need a row in the inventory to be classified by region/municipality." />
                <Input.Search allowClear placeholder="filter by site_key…"
                  value={orphanQ} onChange={e => setOrphanQ(e.target.value)}
                  style={{ width: 280, marginBottom: 8 }} />
                <ProTable<OrphanRow>
                  rowKey="site_key" search={false} options={false}
                  params={{ q: orphanQ }}
                  request={async (params) => {
                    const d = await api<{ items: OrphanRow[] }>('/api/inventory/orphans')
                    const q = ((params as any).q || '').toLowerCase()
                    const rows = q ? d.items.filter(r => r.site_key.toLowerCase().includes(q)) : d.items
                    return { data: rows, success: true, total: rows.length }
                  }}
                  pagination={{ defaultPageSize: 25 }}
                  columns={[
                    { title: 'Site key', dataIndex: 'site_key', copyable: true },
                    { title: 'Events', dataIndex: 'events', align: 'right',
                      sorter: (a, b) => a.events - b.events, defaultSortOrder: 'descend' },
                    { title: 'Last seen', dataIndex: 'last_seen', render: v => formatTs(v as string) },
                  ]}
                />
              </ProCard>
            ),
          },

          /* ─── Tab: Silent sites (pre-v8) ─── */
          {
            key: 'stale', label: 'Silent sites',
            children: (
              <ProCard
                title={`Sites in inventory with no events in the last ${staleDays} days`}
                extra={
                  <Select value={staleDays} onChange={setStaleDays} style={{ width: 140 }}
                    options={[7, 14, 30, 60, 90].map(d => ({ value: d, label: `${d} days` }))} />
                }>
                <Input.Search allowClear placeholder="filter by site_key, name or region…"
                  value={staleQ} onChange={e => setStaleQ(e.target.value)}
                  style={{ width: 360, marginBottom: 8 }} />
                <ProTable<StaleRow>
                  rowKey="site_key" search={false} options={false}
                  params={{ days: staleDays, q: staleQ }}
                  request={async (params) => {
                    const days = (params as any).days ?? staleDays
                    const d = await api<{ items: StaleRow[] }>(`/api/inventory/stale${qs({ days })}`)
                    const q = ((params as any).q || '').toLowerCase()
                    const rows = q
                      ? d.items.filter(r =>
                          r.site_key.toLowerCase().includes(q) ||
                          (r.name || '').toLowerCase().includes(q) ||
                          (r.region || '').toLowerCase().includes(q))
                      : d.items
                    return { data: rows, success: true, total: rows.length }
                  }}
                  pagination={{ defaultPageSize: 25 }}
                  columns={[
                    { title: 'Site key', dataIndex: 'site_key',
                      render: (_, r) => <Link to={`/sites/${encodeURIComponent(r.site_key)}`}>{r.site_key}</Link> },
                    { title: 'Name', dataIndex: 'name', ellipsis: true },
                    { title: 'Region', dataIndex: 'region', render: v => v ? <Tag>{v as string}</Tag> : '—' },
                    { title: 'Last event ever', dataIndex: 'last_event',
                      render: v => v ? formatTs(v as string) : <i style={{ color: '#999' }}>never</i> },
                  ]}
                />
              </ProCard>
            ),
          },

          /* ─── Tab: Region coverage (pre-v8) ─── */
          {
            key: 'coverage', label: 'Region coverage',
            children: (
              <ProCard>
                <Alert type="info" showIcon style={{ marginBottom: 12 }}
                  message="Per-region rollup: inventoried sites vs. sites that have actually emitted events vs. sites with active open alarms." />
                <ProTable<CovRow>
                  rowKey="region" search={false} options={false}
                  request={async () => {
                    const d = await api<{ items: CovRow[] }>('/api/inventory/coverage')
                    return { data: d.items, success: true, total: d.items.length }
                  }}
                  pagination={false}
                  columns={[
                    { title: 'Region', dataIndex: 'region' },
                    { title: 'Sites', dataIndex: 'sites', align: 'right',
                      sorter: (a, b) => a.sites - b.sites },
                    { title: 'Sites with events', dataIndex: 'sites_with_events', align: 'right',
                      sorter: (a, b) => a.sites_with_events - b.sites_with_events },
                    { title: 'Sites with open alarms', dataIndex: 'sites_with_open_alarms', align: 'right',
                      sorter: (a, b) => a.sites_with_open_alarms - b.sites_with_open_alarms,
                      render: v => (v as number) > 0
                        ? <Tag color="red">{v as number}</Tag>
                        : <Tag color="green">0</Tag> },
                  ]}
                />
              </ProCard>
            ),
          },
        ]}
      />

      {/* ── Add / Edit device modal ── */}
      <Modal
        title={editingDevice ? `Edit device ${editingDevice.ip}` : 'Add device'}
        open={deviceModalOpen}
        onCancel={() => setDeviceModalOpen(false)}
        onOk={submitDevice}
        okText={editingDevice ? 'Update' : 'Add'}
        destroyOnClose
      >
        <Form form={deviceForm} layout="vertical" size="small">
          <Form.Item name="ip" label="IP address" rules={[{ required: true }]}>
            <Input disabled={!!editingDevice} placeholder="10.10.1.100" />
          </Form.Item>
          <Form.Item name="site_key" label="Site key" rules={[{ required: true }]}>
            <Input placeholder="TK-SARAJEVO-001" />
          </Form.Item>
          <Space size={16}>
            <Form.Item name="port" label="Port"><InputNumber min={1} max={65535} /></Form.Item>
            <Form.Item name="unit_id" label="Unit ID"><InputNumber min={0} max={255} /></Form.Item>
          </Space>
          <Form.Item name="dev_type" label="Device type">
            <Select options={DEV_TYPES.map(t => ({ value: t, label: t }))} />
          </Form.Item>
          <Form.Item name="name" label="Name"><Input placeholder="optional display name" /></Form.Item>
          <Space size={24}>
            <Form.Item name="fne" valuePropName="checked"><Checkbox>FNE (fotonaponska elektrarna)</Checkbox></Form.Item>
            <Form.Item name="enabled" valuePropName="checked"><Checkbox>Enabled</Checkbox></Form.Item>
          </Space>
        </Form>
      </Modal>

      {/* ── Claim orphan modal ── */}
      <Modal
        title={`Claim orphan ${claimingOrphan?.ip ?? ''}`}
        open={claimOpen}
        onCancel={() => setClaimOpen(false)}
        onOk={submitClaim}
        okText="Claim"
        okButtonProps={{ icon: <CheckCircleOutlined /> }}
        destroyOnClose
      >
        <Form form={claimForm} layout="vertical" size="small">
          <Form.Item name="site_key" label="Site key" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Space size={16}>
            <Form.Item name="port" label="Port"><InputNumber min={1} max={65535} /></Form.Item>
            <Form.Item name="unit_id" label="Unit ID"><InputNumber min={0} max={255} /></Form.Item>
          </Space>
          <Form.Item name="dev_type" label="Device type">
            <Select options={DEV_TYPES.map(t => ({ value: t, label: t }))} />
          </Form.Item>
          <Form.Item name="name" label="Name"><Input placeholder="optional" /></Form.Item>
        </Form>
      </Modal>

      {/* ── Enrich stub site drawer ── */}
      <Drawer
        title={`Enrich site ${enrichingSite?.site_key ?? ''}`}
        open={enrichOpen}
        onClose={() => setEnrichOpen(false)}
        width={420}
        extra={<Button type="primary" onClick={submitEnrich}>Save</Button>}
        destroyOnClose
      >
        <Form form={enrichForm} layout="vertical" size="small">
          <Form.Item name="display_name" label="Display name"><Input /></Form.Item>
          <Form.Item name="region" label="Region" rules={[{ required: true, message: 'Region clears the stub flag' }]}>
            <Select placeholder="Select region" options={REGION_OPTS} showSearch />
          </Form.Item>
          <Form.Item name="municipality" label="Municipality"><Input /></Form.Item>
          <Space size={16}>
            <Form.Item name="lat" label="Latitude"><InputNumber step={0.0001} /></Form.Item>
            <Form.Item name="lon" label="Longitude"><InputNumber step={0.0001} /></Form.Item>
          </Space>
          <Form.Item name="technologies" label="Technologies">
            <Checkbox.Group options={['2G', '3G', '4G', '5G', 'MW', 'FO']} />
          </Form.Item>
          <Space size={24}>
            <Form.Item name="has_genset" valuePropName="checked"><Checkbox>Genset</Checkbox></Form.Item>
            <Form.Item name="has_battery" valuePropName="checked"><Checkbox>Battery</Checkbox></Form.Item>
            <Form.Item name="has_solar" valuePropName="checked"><Checkbox>Solar</Checkbox></Form.Item>
          </Space>
          <Form.Item name="notes" label="Notes"><Input.TextArea rows={3} /></Form.Item>
        </Form>
      </Drawer>
    </>
  )
}
