import { Boxes, CircleDot, Download, RefreshCcw } from 'lucide-react'
import type { ThemeRouteFrameProps } from '../contracts'

export default function XmclRouteFrame({
  children,
  activeTab,
  selectedGameTitle,
  updateCount,
  downloadCount,
}: ThemeRouteFrameProps) {
  const instanceContext = selectedGameTitle && ['Library', 'Downloads', 'Updates'].includes(activeTab)

  return (
    <div className="xmcl-route-frame" data-xmcl-route={activeTab}>
      <header className="xmcl-route-context">
        <div className="xmcl-route-title">
          {instanceContext ? <CircleDot size={17} /> : <Boxes size={17} />}
          <div>
            <strong>{instanceContext ? selectedGameTitle : activeTab}</strong>
            <span>{instanceContext ? activeTab : '0xoLemon workspace'}</span>
          </div>
        </div>
        <div className="xmcl-route-status">
          {updateCount > 0 ? <span><RefreshCcw size={13} />{updateCount}</span> : null}
          {downloadCount > 0 ? <span><Download size={13} />{downloadCount}</span> : null}
        </div>
      </header>
      <div className="xmcl-route-content">{children}</div>
    </div>
  )
}
