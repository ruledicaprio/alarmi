import { ProLayout } from '@ant-design/pro-components'
import { Routes, Route, Navigate, useNavigate, useLocation, Link } from 'react-router-dom'
import { Button, Space, Tooltip } from 'antd'
import { SunOutlined, MoonOutlined, DashboardOutlined, AlertOutlined, ClusterOutlined, AuditOutlined, ThunderboltOutlined } from '@ant-design/icons'
import BhLogo from './components/BhLogo'
import Dashboard from './pages/Dashboard'
import Alarms from './pages/Alarms'
import Sites from './pages/Sites'
import SiteDetail from './pages/SiteDetail'
import Inventory from './pages/Inventory'
import Solar from './pages/Solar'

const ROUTES = {
  path: '/',
  routes: [
    { path: '/dashboard', name: 'Dashboard', icon: <DashboardOutlined /> },
    { path: '/alarms',    name: 'Alarms',    icon: <AlertOutlined /> },
    { path: '/sites',     name: 'Sites',     icon: <ClusterOutlined /> },
    { path: '/solar',     name: 'Solar PV',  icon: <ThunderboltOutlined /> },
    { path: '/inventory', name: 'Inventory', icon: <AuditOutlined /> },
  ],
}

export default function App({ dark, setDark }: { dark: boolean; setDark: (b: boolean) => void }) {
  const nav = useNavigate()
  const loc = useLocation()
  return (
    <ProLayout
      title="BHT Alarm Dashboard"
      logo={<BhLogo size={28} />}
      layout="mix"
      fixSiderbar
      contentStyle={{ background: dark ? '#141414' : '#f5f5f7', minHeight: 'calc(100vh - 56px)' }}
      navTheme={dark ? 'realDark' : 'light'}
      headerTheme={dark ? 'realDark' : 'light'}
      route={ROUTES}
      location={{ pathname: loc.pathname }}
      menuItemRender={(item, dom) =>
        <a onClick={(e) => { e.preventDefault(); nav(item.path || '/') }}>{dom}</a>}
      menuHeaderRender={(logoDom, titleDom) => (
        <Link to="/dashboard" style={{ display: 'flex', alignItems: 'center', gap: 10, paddingLeft: 4 }}>
          {logoDom}{titleDom}
        </Link>
      )}
      rightContentRender={() => (
        <Space size="middle" style={{ paddingRight: 16 }}>
          <Tooltip title={dark ? 'Switch to light mode' : 'Switch to dark mode'}>
            <Button
              type={dark ? 'primary' : 'default'}
              shape="round"
              icon={dark ? <MoonOutlined /> : <SunOutlined />}
              onClick={() => setDark(!dark)}
            >
              {dark ? 'Dark' : 'Light'}
            </Button>
          </Tooltip>
        </Space>
      )}
      breadcrumbRender={(routers = []) => [{ path: '/', breadcrumbName: 'Home' }, ...routers]}
      itemRender={(route) => <span>{route.breadcrumbName}</span>}
    >
      <Routes>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<Dashboard />} />
        <Route path="/alarms" element={<Alarms />} />
        <Route path="/sites" element={<Sites />} />
        <Route path="/sites/:siteKey" element={<SiteDetail />} />
        <Route path="/solar" element={<Solar />} />
        <Route path="/inventory" element={<Inventory />} />
      </Routes>
    </ProLayout>
  )
}
