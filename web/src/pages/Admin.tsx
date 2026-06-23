import { useEffect, useRef, useState } from 'react'
import { Tabs, Alert, Button, Form, Input, Select, Modal, Tag, message } from 'antd'
import { ProTable, ProCard, type ActionType, type ProColumns } from '@ant-design/pro-components'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api } from '../api'
import { formatTs, tsSorter } from '../utils'

interface User { id: number; username: string; full_name: string; role: string; region: string; created_at: string; last_seen: string; disabled: boolean }
interface RegionRow { region: string; label: string; sort_idx: number; sites: number; users: number }

const ROLES = ['superadmin', 'admin', 'user'] as const
const ROLE_COLOR: Record<string, string> = { superadmin: 'red', admin: 'volcano', user: 'blue' }

function UserFormModal({ open, initial, regions, onClose, onSaved }: {
  open: boolean; initial: Partial<User> | null;
  regions: RegionRow[];
  onClose: () => void; onSaved: () => void
}) {
  const [form] = Form.useForm()
  const [submitting, setSubmitting] = useState(false)
  useEffect(() => {
    if (open) form.setFieldsValue(initial || { role: 'user', disabled: false })
  }, [open, initial])
  const onSubmit = async () => {
    const v = await form.validateFields()
    setSubmitting(true)
    try {
      await api('/api/admin/users', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(v),
      })
      message.success(`User ${v.username} saved`)
      onSaved(); onClose()
    } catch (e) { message.error(String(e)) }
    finally { setSubmitting(false) }
  }
  return (
    <Modal title={initial?.id ? `Edit user #${initial.id}` : 'New user'}
      open={open} onCancel={onClose} onOk={onSubmit} confirmLoading={submitting}>
      <Form form={form} layout="vertical">
        <Form.Item name="username" label="Username" rules={[{ required: true, max: 64 }]}>
          <Input placeholder="e.g. rusmir, oncall-night, m.kovacevic" disabled={!!initial?.id} />
        </Form.Item>
        <Form.Item name="full_name" label="Full name"><Input /></Form.Item>
        <Form.Item name="role" label="Role" rules={[{ required: true }]}>
          <Select options={ROLES.map(r => ({ value: r, label: r }))} />
        </Form.Item>
        <Form.Item name="region" label="Region scope (blank = all)">
          <Select allowClear placeholder="restrict view to a region (optional)"
            options={regions.map(r => ({ value: r.region, label: r.label }))} />
        </Form.Item>
        <Form.Item name="disabled" label="Disabled" valuePropName="checked">
          <input type="checkbox" />
        </Form.Item>
        <Alert type="info" showIcon style={{ marginTop: 8 }}
          message="No password here — LDAP integration to be wired in a later round. Role + region are stored now so authz is ready when auth lands." />
      </Form>
    </Modal>
  )
}

function UsersTab() {
  const ref = useRef<ActionType>()
  const [regions, setRegions] = useState<RegionRow[]>([])
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<Partial<User> | null>(null)
  useEffect(() => {
    api<{items: RegionRow[]}>('/api/admin/regions').then(d => setRegions(d.items || []))
  }, [])

  const cols: ProColumns<User>[] = [
    { title: 'ID', dataIndex: 'id', width: 60, sorter: (a,b) => a.id - b.id },
    { title: 'Username', dataIndex: 'username', copyable: true,
      sorter: (a,b) => a.username.localeCompare(b.username) },
    { title: 'Full name', dataIndex: 'full_name', ellipsis: true },
    { title: 'Role', dataIndex: 'role', width: 110,
      filters: ROLES.map(r => ({ text: r, value: r })),
      onFilter: (val, rec) => rec.role === val,
      render: (v: any) => <Tag color={ROLE_COLOR[v as string]}>{v as string}</Tag> },
    { title: 'Region', dataIndex: 'region', width: 130,
      render: (v: any) => v ? <Tag>{v as string}</Tag> : <Tag color="default">all</Tag> },
    { title: 'Created', dataIndex: 'created_at', width: 175,
      sorter: tsSorter<User>('created_at'), render: (v: any) => formatTs(v) },
    { title: 'Disabled', dataIndex: 'disabled', width: 90,
      render: (v: any) => v ? <Tag color="red">disabled</Tag> : <Tag color="green">active</Tag> },
    { title: 'Action', valueType: 'option', width: 130, render: (_, rec) => [
      <a key="edit" onClick={() => { setEditing(rec); setModalOpen(true) }}>edit</a>,
      <a key="del" onClick={async () => {
        Modal.confirm({
          title: `Delete user ${rec.username}?`,
          okType: 'danger',
          onOk: async () => {
            await api(`/api/admin/users/${rec.id}`, { method: 'DELETE' })
            message.success(`deleted ${rec.username}`); ref.current?.reload()
          },
        })
      }}><DeleteOutlined /> delete</a>,
    ] },
  ]
  return (
    <>
      <ProTable<User>
        actionRef={ref}
        rowKey="id"
        columns={cols}
        search={false}
        request={async () => {
          const d = await api<{items: User[]}>('/api/admin/users')
          return { data: d.items, success: true, total: d.items.length }
        }}
        pagination={{ defaultPageSize: 20 }}
        headerTitle="Users"
        toolBarRender={() => [
          <Button key="new" type="primary" icon={<PlusOutlined />}
            onClick={() => { setEditing(null); setModalOpen(true) }}>New user</Button>,
        ]}
      />
      <UserFormModal
        open={modalOpen} initial={editing} regions={regions}
        onClose={() => setModalOpen(false)}
        onSaved={() => ref.current?.reload()}
      />
    </>
  )
}

function RegionsTab() {
  const cols: ProColumns<RegionRow>[] = [
    { title: '#', dataIndex: 'sort_idx', width: 50, sorter: (a,b)=>a.sort_idx-b.sort_idx, defaultSortOrder: 'ascend' },
    { title: 'Region key', dataIndex: 'region', copyable: true },
    { title: 'Display label', dataIndex: 'label' },
    { title: 'Sites', dataIndex: 'sites', align: 'right', sorter: (a,b)=>a.sites-b.sites },
    { title: 'Users', dataIndex: 'users', align: 'right', sorter: (a,b)=>a.users-b.users },
  ]
  return (
    <ProTable<RegionRow>
      rowKey="region"
      columns={cols}
      search={false} options={false} pagination={false}
      request={async () => {
        const d = await api<{items: RegionRow[]}>('/api/admin/regions')
        return { data: d.items, success: true, total: d.items.length }
      }}
      headerTitle="Canonical regions"
    />
  )
}

function RolesTab() {
  return (
    <ProCard>
      <Alert type="info" showIcon style={{ marginBottom: 12 }}
        message="Roles ship with v7 but enforcement is not wired yet — LDAP integration in a later round. Use these to provision the user table now; permissions go live with auth." />
      <ProTable<{role: string; desc: string}>
        rowKey="role" search={false} options={false} pagination={false}
        dataSource={[
          { role: 'superadmin', desc: 'Full access incl. user management, system console, all regions.' },
          { role: 'admin',      desc: 'All regions, write access to verifications + inventory. No user management.' },
          { role: 'user',       desc: 'Read-only on assigned region (NULL = all). Can submit verifications.' },
        ]}
        columns={[
          { title: 'Role', dataIndex: 'role', width: 140,
            render: v => <Tag color={ROLE_COLOR[v as string]}>{v as string}</Tag> },
          { title: 'Intended access', dataIndex: 'desc' },
        ]}
      />
    </ProCard>
  )
}

export default function Admin() {
  return (
    <Tabs defaultActiveKey="users" items={[
      { key: 'users',   label: 'Users',   children: <UsersTab /> },
      { key: 'regions', label: 'Regions', children: <RegionsTab /> },
      { key: 'roles',   label: 'Roles',   children: <RolesTab /> },
    ]} />
  )
}
