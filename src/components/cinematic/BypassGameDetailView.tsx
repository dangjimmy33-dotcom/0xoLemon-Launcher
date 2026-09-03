import {
  AlertTriangle,
  ArrowLeft,
  BadgeInfo,
  CheckCircle2,
  Download,
  ExternalLink,
  LoaderCircle,
  RotateCcw,
  Settings2,
} from 'lucide-react'
import type { GameToolsCatalogItem, GameToolsGameStatus, GameToolsPackageProgress } from '../../types'
import { formatBytes } from '../../lib/format'
import './cinematic.css'

type BypassGameDetailViewProps = {
  item: GameToolsCatalogItem
  progress: GameToolsPackageProgress | null
  status: GameToolsGameStatus | null
  busy: boolean
  onBack: () => void
  onApply: () => Promise<void>
  onRestore: () => Promise<void>
  onOpenComponents: () => void
  onOpenCommunity: () => void
}

function progressPercent(progress: GameToolsPackageProgress): number {
  return progress.bytesTotal > 0 ? Math.min(100, Math.max(0, progress.bytesDone / progress.bytesTotal * 100)) : 0
}

export default function BypassGameDetailView({ item, progress, status, busy, onBack, onApply, onRestore, onOpenComponents, onOpenCommunity }: BypassGameDetailViewProps) {
  const isStore = item.kind === 'store'
  const hasRollback = Boolean(status?.latestPackage?.committed && !status.latestPackage.restoredAt)
  return (
    <section className="cinematic-game-detail" aria-labelledby="cinematic-game-title">
      <div className="cinematic-game-hero">
        {item.backgroundUrl || item.imageUrl ? <img src={item.backgroundUrl ?? item.imageUrl ?? ''} alt="" decoding="async" /> : null}
        <div />
      </div>
      <button className="cinematic-detail-back" type="button" onClick={onBack}><ArrowLeft /> Back to catalog</button>
      <div className="cinematic-detail-copy">
        <span>{item.category ?? item.kind} · AppID {item.appId}</span>
        {item.logoUrl ? <img className="cinematic-detail-logo" src={item.logoUrl} alt={item.name} decoding="async" /> : <h1 id="cinematic-game-title">{item.name}</h1>}
        {item.logoUrl ? <h1 className="sr-only" id="cinematic-game-title">{item.name}</h1> : null}
        <p>{item.packageName ?? item.sourceRepository}</p>
        <div className="cinematic-capabilities">
          <span className={item.launchWithSteam ? 'is-supported' : ''}><CheckCircle2 />{item.launchWithSteam ? 'Can launch with Steam' : 'Steam launch not declared'}</span>
          <span className={item.launchExecutable ? 'is-supported' : ''}><CheckCircle2 />{item.launchExecutable ? 'Executable launch supported' : 'Executable launch not declared'}</span>
        </div>
      </div>
      <div className="cinematic-detail-information">
        {item.note ? <section><h2><BadgeInfo /> Developer note</h2><p>{item.note}</p></section> : null}
        {item.instructions.length > 0 ? <section><h2><AlertTriangle /> Known issues and instructions</h2><ul>{item.instructions.map((value) => <li key={value}>{value}</li>)}</ul></section> : null}
        {item.dependencies.length > 0 ? (
          <section>
            <h2><Settings2 /> Required software</h2>
            <ul>{item.dependencies.map((value) => <li key={value}>{value}</li>)}</ul>
            <button type="button" onClick={onOpenComponents}>Check in Settings → Components</button>
          </section>
        ) : null}
        <section><h2><BadgeInfo /> Package information</h2><p>Source: {item.sourceRepository}</p><p>Files are staged, hash-verified, backed up and atomically applied. Closing the folder picker creates no job.</p></section>
        {isStore ? <section className="cinematic-price-grid"><div><span>Regular</span><strong>{item.regularPrice ?? '—'}</strong></div><div><span>Supporter</span><strong>{item.supporterPrice ?? '—'}</strong></div><div><span>Status</span><strong>{item.active ? 'Available' : 'Unavailable'}</strong></div></section> : null}
        {progress && progress.appId === item.appId ? (
          <section className="cinematic-package-progress">
            <div><strong>{progress.phase}</strong><span>{formatBytes(progress.bytesDone)} / {formatBytes(progress.bytesTotal)}</span></div>
            <progress value={progress.bytesDone} max={Math.max(1, progress.bytesTotal)} />
            <small>{progress.currentFile ?? `${progress.filesDone}/${progress.filesTotal} files`} · {progressPercent(progress).toFixed(1)}%</small>
          </section>
        ) : null}
        {status?.latestPackage ? <section className="cinematic-rollback-state"><RotateCcw /><div><strong>{status.latestPackage.restoredAt ? 'Package restored' : 'Verified rollback available'}</strong><span>{status.latestPackage.appliedFiles} managed files</span></div></section> : null}
      </div>
      <footer className="cinematic-detail-actions">
        {isStore ? (
          <button type="button" className="is-primary" disabled={!item.active} onClick={onOpenCommunity}><ExternalLink /> Contact community</button>
        ) : (
          <>
            {hasRollback ? <button type="button" disabled={busy} onClick={() => void onRestore()}><RotateCcw /> Restore previous files</button> : null}
            <button type="button" className="is-primary" disabled={busy} onClick={() => void onApply()}>
              {busy ? <LoaderCircle className="spin" /> : <Download />} Apply Fix
            </button>
          </>
        )}
      </footer>
    </section>
  )
}
