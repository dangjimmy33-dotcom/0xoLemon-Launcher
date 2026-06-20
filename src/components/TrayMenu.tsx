import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'

interface MenuItem {
  id: string
  label: string
  isDanger?: boolean
  hasSep?: boolean
}

const ITEMS: MenuItem[] = [
  { id: 'store',     label: 'Cửa hàng' },
  { id: 'library',   label: 'Thư viện' },
  { id: 'community', label: 'Cộng đồng' },
  { id: 'settings',  label: 'Thiết lập', hasSep: true },
  { id: 'quit',      label: 'Thoát 0xoLemon', isDanger: true, hasSep: true },
]

async function handleItem(item: MenuItem) {
  // Hide the tray menu window first
  const self = getCurrentWebviewWindow()
  await self.hide()

  if (item.id === 'quit') {
    // exit_app is a registered Tauri command that calls app.exit(0)
    await invoke('exit_app')
    return
  }

  const tab = item.id === 'store' ? 'Home'
    : item.id === 'library' ? 'Library'
    : item.id === 'community' ? 'Community'
    : 'Settings'

  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
  const main = await WebviewWindow.getByLabel('main')
  if (main) {
    await main.emit('navigate', tab)
    await main.show()
    await main.setFocus()
  }
}

export default function TrayMenu() {
  useEffect(() => {
    // Listen to actual Tauri window focus events — much more reliable than onBlur on div
    const win = getCurrentWebviewWindow()
    let unlisten: (() => void) | undefined
    win.onFocusChanged(({ payload: focused }) => {
      if (!focused) {
        win.hide().catch(() => undefined)
      }
    }).then((fn) => {
      unlisten = fn
    }).catch(() => undefined)

    return () => unlisten?.()
  }, [])

  return (
    <div className="tray-menu-root">
      <div className="tray-menu-logo">
        <span className="tray-menu-logo-dot" />
        <span>0xoLemon</span>
      </div>
      {ITEMS.map((item) => (
        <div key={item.id}>
          {item.hasSep && <div className="tray-menu-divider" />}
          <button
            className={`tray-menu-item${item.isDanger ? ' danger' : ''}`}
            onClick={() => void handleItem(item)}
          >
            {item.label}
          </button>
        </div>
      ))}
    </div>
  )
}
