import { Sidebar } from '../../components/layout'
import type { ThemeShellProps } from '../contracts'

export default function DefaultShell({
  children,
  activeTab,
  onNavigate,
  serviceStatus,
  updateCount,
  downloadCount,
  luaModeEnabled,
  isSidebarCollapsed,
  onToggleSidebar,
  onSelectGame,
}: ThemeShellProps) {
  const handleSelectTab = (tabId: import('../../types').TabId) => {
    if (tabId === 'Store' || tabId === 'Library') {
      onSelectGame(null)
    }
    onNavigate(tabId)
  }

  return (
    <main className={`launcher-shell premium-shell${isSidebarCollapsed ? ' sidebar-collapsed-shell' : ''}`} data-theme-shell="default">
      <Sidebar
        serviceStatus={serviceStatus}
        activeTab={activeTab}
        onSelect={handleSelectTab}
        updateCount={updateCount}
        downloadCount={downloadCount}
        luaModeEnabled={luaModeEnabled}
        isSidebarCollapsed={isSidebarCollapsed}
        onToggleSidebar={onToggleSidebar}
      />
      {children}
    </main>
  )
}
