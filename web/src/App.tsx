import { ProLayout } from '@ant-design/pro-components'
import { Routes, Route, Navigate, useNavigate, useLocation, Link } from 'react-router-dom'
import { Button, Space, Tooltip } from 'antd'
import { SunOutlined, MoonOutlined, DashboardOutlined, AlertOutlined, ClusterOutlined, AuditOutlined, DesktopOutlined, TeamOutlined, GlobalOutlined } from '@ant-design/icons'
import BhLogo from './components/BhLogo'
import HuaweiIcon from './components/HuaweiIcon'
import SolarIcon from './components/SolarIcon'
import Dashboard from './pages/Dashboard'
import Alarms from './pages/Alarms'
import Sites from './pages/Sites'
import SiteDetail from './pages/SiteDetail'
import Inventory from './pages/Inventory'
import Solar from './pages/Solar'
import System from './pages/System'
import Admin from './pages/Admin'
import MapPage from './pages/Map'
import NetEcoAlarms from './pages/NetEcoAlarms'

const ROUTES = {
  path: '/',
  routes: [
    { path: '/dashboard', name: 'Dashboard', icon: <DashboardOutlined /> },
    { path: '/alarms',    name: 'Alarms',    icon: <AlertOutlined /> },
    { path: '/sites',     name: 'Sites',     icon: <ClusterOutlined /> },
    { path: '/map',       name: 'Map',       icon: <GlobalOutlined /> },
    { path: '/neteco',    name: 'NetEco',    icon: <HuaweiIcon /> },
    { path: '/solar',     name: 'Solar PV',  icon: <SolarIcon /> },
    { path: '/inventory', name: 'Inventory', icon: <AuditOutlined /> },
    { path: '/system',    name: 'System',    icon: <DesktopOutlined /> },
    { path: '/admin',     name: 'Admin',     icon: <TeamOutlined /> },
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
      menuHeaderRender={(logoDom, _titleDom, props) => {
        const current = ROUTES.routes.find(
          r => loc.pathname === r.path || loc.pathname.startsWith(r.path + '/')
        )
        return (
          <Link to="/dashboard" style={{ display: 'flex', alignItems: 'center', gap: 10, paddingLeft: 4 }}>
            {logoDom}
            {props?.collapsed !== true && (
              <h1 style={{ margin: 0, fontWeight: 600, fontSize: 18, lineHeight: '32px', overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis' }}>
                {current?.name ?? 'Dashboard'}
              </h1>
            )}
          </Link>
        )
      }}
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
        <Route path="/map"    element={<MapPage />} />
        <Route path="/neteco" element={<NetEcoAlarms />} />
        <Route path="/solar" element={<Solar />} />
        <Route path="/inventory" element={<Inventory />} />
        <Route path="/system" element={<System />} />
        <Route path="/admin" element={<Admin />} />
      </Routes>
    </ProLayout>
  )
}
