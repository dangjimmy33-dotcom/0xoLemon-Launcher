import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import {
  collection, onSnapshot, query, limit, orderBy,
  addDoc, serverTimestamp, doc, deleteDoc, updateDoc
} from 'firebase/firestore'
import { db } from '../firebase'
import { open } from '@tauri-apps/plugin-dialog'
import { Send, Paperclip, Maximize2, X, Edit2, Trash2, Hash } from 'lucide-react'
import type { DiscordAuthUser } from '../types'

export interface ChatMessage {
  id: string
  senderId: string
  senderName: string
  senderAvatar?: string
  text: string
  imageBase64?: string
  mediaUrl?: string
  mediaType?: string
  timestamp: number
  reactions?: Record<string, string[]> // emoji → [senderId, ...]
}

// ── Helpers ──────────────────────────────────────────────
const getSenderId = () => {
  let sid = localStorage.getItem('chat_sender_id')
  if (!sid) { sid = Math.random().toString(36).substring(2, 10); localStorage.setItem('chat_sender_id', sid) }
  return sid
}

function extractYouTubeId(text: string): string | null {
  const m = text.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/|youtube\.com\/embed\/)([a-zA-Z0-9_-]{11})/)
  return m ? m[1] : null
}

function formatTime(ts: number) {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function formatDate(ts: number) {
  return new Date(ts).toLocaleDateString([], { day: 'numeric', month: 'short' })
}

// Track recent/frequent emojis in localStorage
const REACTION_KEY = 'chat_reaction_history'
const DEFAULT_REACTIONS = ['👍', '✅', '❤️', '😂', '😮', '😢']

function getTopReactions(): string[] {
  try {
    const raw = localStorage.getItem(REACTION_KEY)
    if (!raw) return DEFAULT_REACTIONS
    const counts: Record<string, number> = JSON.parse(raw)
    const sorted = Object.entries(counts).sort((a, b) => b[1] - a[1]).map(e => e[0])
    const merged = [...sorted, ...DEFAULT_REACTIONS.filter(e => !sorted.includes(e))]
    return merged.slice(0, 6)
  } catch { return DEFAULT_REACTIONS }
}

function recordReactionUsed(emoji: string) {
  try {
    const raw = localStorage.getItem(REACTION_KEY)
    const counts: Record<string, number> = raw ? JSON.parse(raw) : {}
    counts[emoji] = (counts[emoji] ?? 0) + 1
    localStorage.setItem(REACTION_KEY, JSON.stringify(counts))
  } catch { /* ignore */ }
}

// ── Avatar ────────────────────────────────────────────────
function Avatar({ name, url, size = 36 }: { name: string; url?: string; size?: number }) {
  const [err, setErr] = useState(false)
  const color = `hsl(${[...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 360}, 65%, 55%)`
  if (url && !err) {
    return <img src={url} alt={name} className="chat-avatar" style={{ width: size, height: size, borderRadius: '50%', objectFit: 'cover', flexShrink: 0 }} onError={() => setErr(true)} />
  }
  return (
    <div className="chat-avatar" style={{ width: size, height: size, borderRadius: '50%', background: color, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: '#fff', fontWeight: 700, fontSize: size * 0.42, textTransform: 'uppercase', userSelect: 'none' }}>
      {name[0] ?? '?'}
    </div>
  )
}

// ── Group messages ────────────────────────────────────────
function groupMessages(messages: ChatMessage[], mySenderId: string, myAvatar?: string) {
  const groups: Array<{ senderId: string; senderName: string; senderAvatar?: string; date: string; msgs: ChatMessage[] }> = []
  for (const msg of messages) {
    const date = formatDate(msg.timestamp)
    const last = groups[groups.length - 1]
    const avatar = msg.senderId === mySenderId ? (myAvatar ?? msg.senderAvatar) : msg.senderAvatar
    if (last && last.senderId === msg.senderId && last.date === date) {
      last.msgs.push(msg)
    } else {
      groups.push({ senderId: msg.senderId, senderName: msg.senderName, senderAvatar: avatar, date, msgs: [msg] })
    }
  }
  return groups
}

// ── Message content ───────────────────────────────────────
function MessageContent({ text, imageBase64, mediaUrl, mediaType }: { text: string; imageBase64?: string; mediaUrl?: string; mediaType?: string }) {
  const ytId = text ? extractYouTubeId(text) : null
  const isVideo = mediaType?.startsWith('video/')
  return (
    <div className="msg-content">
      {text && <p className="msg-text">{text}</p>}
      {ytId && (
        <div className="yt-embed">
          <iframe src={`https://www.youtube.com/embed/${ytId}`} title="YouTube video"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen loading="lazy" />
        </div>
      )}
      {imageBase64 && <img src={imageBase64} alt="attached" className="chat-image" />}
      {mediaUrl && (
        isVideo
          ? <video src={mediaUrl} controls className="chat-video" />
          : <img src={mediaUrl} alt="attached" className="chat-image" />
      )}
    </div>
  )
}

// ── Context Menu ─────────────────────────────────────────
interface ContextMenuProps {
  x: number; y: number
  msg: ChatMessage
  isMine: boolean
  topReactions: string[]
  onClose: () => void
  onEdit: () => void
  onDelete: () => void
  onReact: (emoji: string) => void
  onCopy: () => void
}

function ContextMenu({ x, y, msg, isMine, topReactions, onClose, onEdit, onDelete, onReact, onCopy }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handleDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose()
    }
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('mousedown', handleDown)
    document.addEventListener('keydown', handleKey)
    return () => { document.removeEventListener('mousedown', handleDown); document.removeEventListener('keydown', handleKey) }
  }, [onClose])

  // Adjust position so menu doesn't go off screen
  const [pos, setPos] = useState({ x, y })
  useEffect(() => {
    if (!menuRef.current) return
    const rect = menuRef.current.getBoundingClientRect()
    const vw = window.innerWidth, vh = window.innerHeight
    setPos({
      x: rect.right > vw ? x - rect.width : x,
      y: rect.bottom > vh ? y - rect.height : y,
    })
  }, [x, y])

  const menuItem = (icon: React.ReactNode, label: string, action: () => void, danger = false) => (
    <button className={`ctx-item${danger ? ' ctx-danger' : ''}`} onClick={() => { action(); onClose() }}>
      <span className="ctx-icon">{icon}</span>
      <span>{label}</span>
    </button>
  )

  return createPortal(
    <div ref={menuRef} className="ctx-menu" style={{ left: pos.x, top: pos.y }}>
      {/* Quick reactions */}
      <div className="ctx-reactions">
        {topReactions.map(e => (
          <button key={e} className="ctx-reaction-btn" onClick={() => { onReact(e); onClose() }} title={e}>
            {e}
          </button>
        ))}
      </div>
      <div className="ctx-divider" />
      {menuItem(<ReplyIcon />, 'Reply', () => { })}
      {menuItem(<CopyIcon />, 'Copy Text', onCopy)}
      {isMine && menuItem(<EditIcon />, 'Edit Message', onEdit)}
      <div className="ctx-divider" />
      {menuItem(<IdIcon />, 'Copy Message ID', () => navigator.clipboard.writeText(msg.id))}
      {isMine && menuItem(<TrashIcon />, 'Delete Message', onDelete, true)}
    </div>,
    document.body
  )
}

// SVG icons (no emoji)
const ReplyIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="9 17 4 12 9 7" /><path d="M20 18v-2a4 4 0 0 0-4-4H4" />
  </svg>
)
const CopyIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </svg>
)
const EditIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
  </svg>
)
const TrashIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polyline points="3 6 5 6 21 6" /><path d="M19 6l-1 14H6L5 6" /><path d="M10 11v6M14 11v6" /><path d="M9 6V4h6v2" />
  </svg>
)
const IdIcon = () => (
  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="2" y="5" width="20" height="14" rx="2" /><line x1="2" y1="10" x2="22" y2="10" />
  </svg>
)

// ── Hover quick-action bar ────────────────────────────────
function HoverActions({ msg: _msg, isMine, topReactions, onContext, onEdit, onDelete, onReact }: {
  msg: ChatMessage; isMine: boolean; topReactions: string[]
  onContext: (e: React.MouseEvent) => void
  onEdit: () => void; onDelete: () => void; onReact: (emoji: string) => void
}) {
  return (
    <div className="msg-hover-actions">
      {topReactions.slice(0, 4).map(e => (
        <button key={e} className="msg-quick-react" onClick={() => onReact(e)} title={`React ${e}`}>
          {e}
        </button>
      ))}
      {isMine && (
        <button className="msg-action-btn" onClick={onEdit} title="Edit">
          <Edit2 size={14} />
        </button>
      )}
      {isMine && (
        <button className="msg-action-btn danger" onClick={onDelete} title="Delete">
          <Trash2 size={14} />
        </button>
      )}
      <button className="msg-action-btn" onClick={onContext} title="More options">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="5" cy="12" r="2" /><circle cx="12" cy="12" r="2" /><circle cx="19" cy="12" r="2" />
        </svg>
      </button>
    </div>
  )
}

// ── ChatBody ──────────────────────────────────────────────
interface GameChatProps {
  gameId: string
  discordUser?: DiscordAuthUser | null
}

function ChatBody({ gameId, discordUser, compact }: GameChatProps & { compact?: boolean }) {
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [inputText, setInputText] = useState('')
  const [sending, setSending] = useState(false)
  const [uploadProgress, setUploadProgress] = useState<string | null>(null)
  const [editingMsgId, setEditingMsgId] = useState<string | null>(null)
  const [editInput, setEditInput] = useState('')
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; msg: ChatMessage } | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const topReactions = getTopReactions()

  const senderId = getSenderId()
  const senderName = discordUser?.displayName ?? discordUser?.username ?? `User_${senderId.substring(0, 4)}`
  const senderAvatar = discordUser?.avatarUrl

  // Load local history + HF sync
  useEffect(() => {
    let mounted = true
    invoke<ChatMessage[]>('load_chat_history', { gameId })
      .then(h => { if (mounted) setMessages(h) })
      .catch(console.error)
    invoke('download_from_huggingface', { gameId })
      .then(() => invoke<ChatMessage[]>('load_chat_history', { gameId }))
      .then(h => { if (mounted) setMessages(h) })
      .catch(console.error)
    return () => { mounted = false }
  }, [gameId])

  // Firestore realtime
  useEffect(() => {
    const q = query(collection(db, 'chats', gameId, 'messages'), orderBy('timestamp', 'desc'), limit(50))
    return onSnapshot(q, snapshot => {
      snapshot.docChanges().forEach(change => {
        const d = change.doc.data()
        const ts = d.timestamp?.toMillis ? d.timestamp.toMillis() : Date.now()
        const msg: ChatMessage = {
          id: change.doc.id, senderId: d.senderId || 'unknown',
          senderName: d.senderName || 'Unknown', senderAvatar: d.senderAvatar,
          text: d.text || '', imageBase64: d.imageBase64,
          mediaUrl: d.mediaUrl, mediaType: d.mediaType,
          timestamp: ts, reactions: d.reactions ?? {},
        }
        if (change.type === 'added') {
          setMessages(prev => {
            if (prev.some(m => m.id === msg.id)) return prev
            const next = [...prev, msg].sort((a, b) => a.timestamp - b.timestamp)
            invoke('save_chat_message', { gameId, message: msg }).catch(console.error)
            return next
          })
        } else if (change.type === 'modified') {
          setMessages(prev => {
            const next = prev.map(m => m.id === msg.id ? msg : m)
            invoke('edit_chat_message', { gameId, messageId: msg.id, newText: msg.text }).catch(console.error)
            return next
          })
        } else if (change.type === 'removed') {
          setMessages(prev => {
            invoke('delete_chat_message', { gameId, messageId: change.doc.id }).catch(console.error)
            return prev.filter(m => m.id !== change.doc.id)
          })
        }
      })
    })
  }, [gameId])

  useEffect(() => {
    const el = containerRef.current
    if (el) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
    }
  }, [messages])

  const handleSend = async () => {
    if (!inputText.trim() || sending) return
    setSending(true)
    try {
      await addDoc(collection(db, 'chats', gameId, 'messages'), {
        senderId, senderName, senderAvatar: senderAvatar ?? null,
        text: inputText.trim(), timestamp: serverTimestamp(),
      })
      setInputText('')
      inputRef.current?.focus()
    } catch (e) { console.error(e) } finally { setSending(false) }
  }

  const handleMediaUpload = async () => {
    try {
      const selected = await open({ multiple: false, filters: [{ name: 'Media', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'mp4', 'webm'] }] })
      if (!selected) return
      const filepath = Array.isArray(selected) ? selected[0] : selected
      if (!filepath) return

      const ext = filepath.split('.').pop()?.toLowerCase() || 'png'
      const isVideo = ['mp4', 'webm'].includes(ext)
      const filename = `${Date.now()}_${Math.random().toString(36).substring(2, 8)}.${ext}`
      const mediaType = isVideo ? `video/${ext}` : `image/${ext}`

      setUploadProgress(isVideo ? 'Uploading video...' : 'Uploading image...')
      setSending(true)

      const mediaUrl = await invoke<string>('upload_chat_media_from_path', { filename, filepath })

      await addDoc(collection(db, 'chats', gameId, 'messages'), {
        senderId, senderName, senderAvatar: senderAvatar ?? null,
        text: '', mediaUrl, mediaType, timestamp: serverTimestamp(),
      })
    } catch (err) {
      console.error('Upload failed:', err)
      alert('Upload failed. Image max 5MB, video max 20MB.')
    } finally {
      setSending(false)
      setUploadProgress(null)
    }
  }

  const handleReact = useCallback(async (msg: ChatMessage, emoji: string) => {
    recordReactionUsed(emoji)
    const reactions = { ...(msg.reactions ?? {}) }
    const existing = reactions[emoji] ?? []
    if (existing.includes(senderId)) {
      reactions[emoji] = existing.filter(id => id !== senderId)
      if (reactions[emoji].length === 0) delete reactions[emoji]
    } else {
      reactions[emoji] = [...existing, senderId]
    }
    try {
      await updateDoc(doc(db, 'chats', gameId, 'messages', msg.id), { reactions })
    } catch (e) { console.error(e) }
  }, [gameId, senderId])

  const handleEdit = async (msgId: string, newText: string) => {
    try {
      await updateDoc(doc(db, 'chats', gameId, 'messages', msgId), { text: newText })
      setEditingMsgId(null); setEditInput('')
    } catch (e) { console.error(e) }
  }

  const handleDelete = async (msg: ChatMessage) => {
    if (!confirm('Delete this message?')) return
    try {
      await deleteDoc(doc(db, 'chats', gameId, 'messages', msg.id))
      if (msg.mediaUrl) invoke('delete_chat_media', { url: msg.mediaUrl }).catch(console.error)
    } catch (e) { console.error(e) }
  }

  const openCtx = (e: React.MouseEvent, msg: ChatMessage) => {
    e.preventDefault(); e.stopPropagation()
    setCtxMenu({ x: e.clientX, y: e.clientY, msg })
  }

  const groups = groupMessages(messages, senderId, senderAvatar)

  return (
    <>
      {/* Messages */}
      <div className="chat-messages" ref={containerRef}>
        {groups.length === 0 && (
          <div className="chat-empty">No messages yet. Be the first!</div>
        )}
        {groups.map((group, gi) => (
          <div key={`${group.senderId}-${gi}`} className="chat-group">
            <div className="chat-group-header">
              <Avatar name={group.senderName} url={group.senderAvatar} size={compact ? 28 : 36} />
              <div className="chat-group-meta">
                <span className="sender-name">{group.senderName}</span>
                <span className="msg-date">{group.date}</span>
              </div>
            </div>
            <div className="chat-group-messages">
              {group.msgs.map(msg => (
                <div key={msg.id} className="chat-row" onContextMenu={e => openCtx(e, msg)}>
                  <span className="msg-time">{formatTime(msg.timestamp)}</span>
                  <div className="msg-body">
                    {editingMsgId === msg.id ? (
                      <div className="msg-edit-area">
                        <input
                          autoFocus value={editInput}
                          onChange={e => setEditInput(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Enter') handleEdit(msg.id, editInput)
                            if (e.key === 'Escape') setEditingMsgId(null)
                          }}
                        />
                        <span className="msg-edit-hint">esc to cancel · enter to save</span>
                      </div>
                    ) : (
                      <MessageContent text={msg.text} imageBase64={msg.imageBase64} mediaUrl={msg.mediaUrl} mediaType={msg.mediaType} />
                    )}
                    {/* Reactions */}
                    {msg.reactions && Object.keys(msg.reactions).length > 0 && (
                      <div className="msg-reactions">
                        {Object.entries(msg.reactions).map(([emoji, users]) => (
                          users.length > 0 && (
                            <button
                              key={emoji}
                              className={`reaction-chip${users.includes(senderId) ? ' active' : ''}`}
                              onClick={() => handleReact(msg, emoji)}
                            >
                              {emoji} <span>{users.length}</span>
                            </button>
                          )
                        ))}
                      </div>
                    )}
                  </div>
                  {/* Hover actions */}
                  {editingMsgId !== msg.id && (
                    <HoverActions
                      msg={msg}
                      isMine={msg.senderId === senderId}
                      topReactions={topReactions}
                      onContext={e => openCtx(e, msg)}
                      onEdit={() => { setEditingMsgId(msg.id); setEditInput(msg.text) }}
                      onDelete={() => handleDelete(msg)}
                      onReact={emoji => handleReact(msg, emoji)}
                    />
                  )}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>

      {/* Context menu */}
      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x} y={ctxMenu.y} msg={ctxMenu.msg}
          isMine={ctxMenu.msg.senderId === senderId}
          topReactions={topReactions}
          onClose={() => setCtxMenu(null)}
          onEdit={() => { setEditingMsgId(ctxMenu.msg.id); setEditInput(ctxMenu.msg.text) }}
          onDelete={() => handleDelete(ctxMenu.msg)}
          onReact={emoji => handleReact(ctxMenu.msg, emoji)}
          onCopy={() => navigator.clipboard.writeText(ctxMenu.msg.text)}
        />
      )}

      {/* Upload progress */}
      {uploadProgress && (
        <div className="chat-upload-progress">
          <div className="chat-upload-spinner" />
          <span>{uploadProgress}</span>
        </div>
      )}

      {/* Input area */}
      <div className="chat-input-area">
        <button className="icon-btn" onClick={handleMediaUpload} disabled={sending} title="Attach image or video">
          <Paperclip size={18} />
        </button>
        <input
          ref={inputRef}
          type="text"
          placeholder={`Message as ${senderName}...`}
          value={inputText}
          onChange={e => setInputText(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && !e.shiftKey && handleSend()}
          disabled={sending}
        />
        <button className="send-btn" onClick={handleSend} disabled={sending || !inputText.trim()}>
          <Send size={16} />
        </button>
      </div>
    </>
  )
}

// ── GameChat (panel + modal) ──────────────────────────────
export function GameChat({ gameId, discordUser }: GameChatProps) {
  const [expanded, setExpanded] = useState(false)

  return (
    <>
      <div className="game-chat-panel">
        <div className="chat-header">
          <Hash size={15} style={{ opacity: 0.6 }} />
          <h4>Community Hub</h4>
          <button className="icon-btn" onClick={() => setExpanded(true)} title="Expand" style={{ marginLeft: 'auto' }}>
            <Maximize2 size={14} />
          </button>
        </div>
        <ChatBody gameId={gameId} discordUser={discordUser} compact />
      </div>

      {expanded && typeof document !== 'undefined' && createPortal(
        <div className="chat-modal-backdrop" role="presentation" onClick={() => setExpanded(false)}>
          <div className="chat-modal" role="dialog" aria-modal="true" aria-label="Community Hub" onClick={e => e.stopPropagation()}>
            <div className="chat-header">
              <Hash size={15} style={{ opacity: 0.6 }} />
              <h4>Community Hub</h4>
              <button className="icon-btn" onClick={() => setExpanded(false)} title="Close" style={{ marginLeft: 'auto' }}>
                <X size={16} />
              </button>
            </div>
            <ChatBody gameId={gameId} discordUser={discordUser} />
          </div>
        </div>,
        document.body
      )}
    </>
  )
}
