import { Layout, Menu } from 'antd'
import { DashboardOutlined, AlertOutlined, ClusterOutlined } from '@ant-design/icons'
import { Routes, Route, useNavigate, useLocation, Navigate } from 'react-router-dom'
import Overview from './pages/Overview'
import Alarms from './pages/Alarms'
import Sites from './pages/Sites'
import SiteDetail from './pages/SiteDetail'

const { Header, Sider, Content } = Layout

export default function App() {
  const nav = useNavigate()
  const loc = useLocation()
  const selected = '/' + (loc.pathname.split('/')[1] || 'overview')
  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider breakpoint="lg" collapsedWidth="0" theme="dark">
        <div style={{ color: '#fff', fontWeight: 700, fontSize: 16, padding: '16px 20px', letterSpacing: 1 }}>
          BHT&nbsp;ALARMS
        </div>
        <Menu
          theme="dark" mode="inline" selectedKeys={[selected]}
          onClick={(e) => nav(e.key)}
          items={[
            { key: '/overview', icon: <DashboardOutlined />, label: 'Overview' },
            { key: '/alarms', icon: <AlertOutlined />, label: 'Alarms' },
            { key: '/sites', icon: <ClusterOutlined />, label: 'Sites' },
          ]}
        />
      </Sider>
      <Layout>
        <Header style={{ background: '#fff', paddingLeft: 24, fontSize: 18, fontWeight: 600 }}>
          Telecom Backup Power & Cooling — Alarm Dashboard
        </Header>
        <Content style={{ margin: 16 }}>
          <Routes>
            <Route path="/" element={<Navigate to="/overview" replace />} />
            <Route path="/overview" element={<Overview />} />
            <Route path="/alarms" element={<Alarms />} />
            <Route path="/sites" element={<Sites />} />
            <Route path="/sites/:siteKey" element={<SiteDetail />} />
          </Routes>
        </Content>
      </Layout>
    </Layout>
  )
}
