import React, { useEffect, useState } from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { ConfigProvider, theme as antdTheme, App as AntdApp } from 'antd'
import enUS from 'antd/locale/en_US'
import App from './App'

function Root() {
  const [dark, setDark] = useState<boolean>(() => localStorage.getItem('bht-dark') === '1')

  useEffect(() => {
    localStorage.setItem('bht-dark', dark ? '1' : '0')
    // ensure html/body background follows the theme — antd ConfigProvider
    // only styles its own components, not the document chrome.
    document.body.style.background = dark ? '#141414' : '#f5f5f7'
    document.body.style.color      = dark ? 'rgba(255,255,255,0.88)' : 'rgba(0,0,0,0.88)'
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
  }, [dark])

  return (
    <ConfigProvider
      locale={enUS}
      theme={{
        algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
        token: { colorPrimary: '#f57e20', borderRadius: 6 },  // BHT brand orange
      }}
    >
      <AntdApp>
        <BrowserRouter>
          <App dark={dark} setDark={setDark} />
        </BrowserRouter>
      </AntdApp>
    </ConfigProvider>
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><Root /></React.StrictMode>
)
