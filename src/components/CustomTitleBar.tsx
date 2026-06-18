import type React from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauriRuntime } from '../lib/gameMeta'
import type { CloseBehavior } from '../lib/preferences'

export function CustomTitleBar({ closeBehavior = 'exit' }: { closeBehavior?: CloseBehavior }) {
  const win = isTauriRuntime() ? getCurrentWindow() : null

  function handleMinimize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.minimize()
  }
  function handleMaximize(e: React.MouseEvent) {
    e.stopPropagation()
    void win?.toggleMaximize()
  }
  function handleClose(e: React.MouseEvent) {
    e.stopPropagation()
    if (closeBehavior === 'minimize') {
      void win?.minimize()
      return
    }
    void win?.close()
  }

  return (
    <div data-tauri-drag-region className="custom-titlebar">
      <div className="titlebar-drag-area" data-tauri-drag-region>
        <span className="titlebar-label">0xoLemon Launcher</span>
      </div>
      <div className="titlebar-actions">
        <button
          className="titlebar-btn minimize-btn"
          title="Minimize"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMinimize}
        >
          <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
        </button>
        <button
          className="titlebar-btn maximize-btn"
          title="Maximize"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleMaximize}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor"/></svg>
        </button>
        <button
          className="titlebar-btn close-btn"
          title={closeBehavior === 'minimize' ? 'Minimize to taskbar' : 'Exit launcher'}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={handleClose}
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" strokeWidth="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" strokeWidth="1.2"/></svg>
        </button>
      </div>
    </div>
  )
}
