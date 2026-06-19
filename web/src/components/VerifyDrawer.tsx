import { useEffect, useState } from 'react'
import {
  Drawer, Form, Input, Select, Checkbox, Space, Button, Alert, Divider, Typography, Tag, message,
} from 'antd'
import { api } from '../api'
import { formatTs } from '../utils'

const { Text } = Typography

interface Summary {
  site_key: string
  last_verified_at: string
  last_verified_by: string
  events_through: string
  events_since: number
  confirmed_ips: string[]
  current_ips: string[]
  new_ips: string[]
  region_confirmed: string
}

export default function VerifyDrawer({
  open, siteKey, currentRegion, onClose, onVerified,
}: {
  open: boolean
  siteKey: string
  currentRegion: string
  onClose: () => void
  onVerified: () => void
}) {
  const [summary, setSummary] = useState<Summary | null>(null)
  const [loading, setLoading] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [form] = Form.useForm()
  const [regions, setRegions] = useState<string[]>([])

  useEffect(() => {
    if (!open) return
    setLoading(true)
    Promise.all([
      api<Summary>(`/api/sites/${encodeURIComponent(siteKey)}/verification/summary`),
      api<{items: string[]}>(`/api/regions`),
    ]).then(([s, r]) => {
      setSummary(s)
      setRegions(r.items || [])
      form.setFieldsValue({
        verified_by: localStorage.getItem('bht-operator') || '',
        notes: '',
        ip_inventory: s.current_ips,
        region_confirmed: s.region_confirmed || currentRegion || '',
      })
    }).catch(e => message.error(String(e))).finally(() => setLoading(false))
  }, [open, siteKey])

  const onSubmit = async () => {
    const vals = await form.validateFields()
    if (vals.verified_by) localStorage.setItem('bht-operator', vals.verified_by)
    setSubmitting(true)
    try {
      const body = {
        verified_by: vals.verified_by,
        notes: vals.notes || '',
        events_through: new Date().toISOString(),
        ip_inventory: vals.ip_inventory || [],
        region_confirmed: vals.region_confirmed || '',
      }
      const r = await api<{id: number; verified_at: string}>(
        `/api/sites/${encodeURIComponent(siteKey)}/verify`,
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) },
      )
      message.success(`Verified · #${r.id} @ ${r.verified_at}`)
      onVerified()
      onClose()
    } catch (e) {
      message.error(String(e))
    } finally { setSubmitting(false) }
  }

  return (
    <Drawer
      open={open} onClose={onClose} width={560}
      title={`Verify final inventory for ${siteKey}`}
      footer={
        <Space style={{ float: 'right' }}>
          <Button onClick={onClose}>Cancel</Button>
          <Button type="primary" loading={submitting} onClick={onSubmit}>Mark verified</Button>
        </Space>
      }
    >
      {loading ? <Text type="secondary">loading summary…</Text> : summary && (
        <>
          <Alert type={summary.last_verified_at ? 'info' : 'warning'} showIcon style={{ marginBottom: 12 }}
            message={summary.last_verified_at
              ? <>Last verified <b>{formatTs(summary.last_verified_at)}</b> by <Tag>{summary.last_verified_by || '?'}</Tag>{summary.events_since > 0 && <> · <b>{summary.events_since}</b> new event{summary.events_since>1?'s':''} since</>}</>
              : <>This site has never been verified.</>}
          />
          {summary.new_ips.length > 0 && (
            <Alert type="warning" showIcon style={{ marginBottom: 12 }}
              message={<>New IPs since last verify: <Space size={4} wrap>{summary.new_ips.map(i => <Tag key={i} color="orange">{i}</Tag>)}</Space></>} />
          )}

          <Divider plain style={{ margin: '8px 0 16px' }}>Your review</Divider>

          <Form form={form} layout="vertical">
            <Form.Item label="Verified by" name="verified_by"
              rules={[{ required: true, message: 'who is verifying?' }]}>
              <Input placeholder="e.g. rusmir / oncall-night" />
            </Form.Item>
            <Form.Item label="Notes (what you checked, what looked wrong)" name="notes">
              <Input.TextArea rows={4} placeholder="optional but useful for audit trail" />
            </Form.Item>
            <Form.Item label={`Confirmed device IPs at this site (${summary.current_ips.length})`} name="ip_inventory">
              <Checkbox.Group options={summary.current_ips.map(i => ({ label: i, value: i }))} />
            </Form.Item>
            <Form.Item label="Confirmed region" name="region_confirmed">
              <Select allowClear showSearch placeholder="region"
                options={regions.map(r => ({ value: r, label: r }))} />
            </Form.Item>
          </Form>
        </>
      )}
    </Drawer>
  )
}
