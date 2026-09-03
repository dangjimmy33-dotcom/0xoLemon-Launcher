import { Download, RefreshCcw, Wifi } from 'lucide-react'
import type { ThemeRouteFrameProps } from '../contracts'

export default function SteamRouteFrame({
  children,
  activeTab,
  selectedGameTitle,
  serviceStatus,
  updateCount,
  downloadCount,
}: ThemeRouteFrameProps) {
  return (
    <div className="steam-route-frame" data-steam-route={activeTab}>
      <div className="steam-route-context" aria-label="Current Steam-style workspace">
        <div className="steam-route-breadcrumb">
          <strong>{activeTab}</strong>
          {selectedGameTitle ? <><span aria-hidden="true">/</span><span>{selectedGameTitle}</span></> : null}
        </div>
        <div className="steam-route-indicators">
          <span><Wifi size={12} />{serviceStatus}</span>
          {updateCount > 0 ? <span><RefreshCcw size={12} />{updateCount}</span> : null}
          {downloadCount > 0 ? <span><Download size={12} />{downloadCount}</span> : null}
        </div>
      </div>
      <div className="steam-route-content">{children}</div>
    </div>
  )
}
