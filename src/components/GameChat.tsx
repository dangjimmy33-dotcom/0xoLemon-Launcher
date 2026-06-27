import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { collection, onSnapshot, query, limit, orderBy, addDoc, serverTimestamp } from 'firebase/firestore'
import { db } from '../firebase'
import { Send, Image as ImageIcon, Trash2, Maximize2, X } from 'lucide-react'
import type { DiscordAuthUser } from '../types'

export interface ChatMessage {
  id: string
  senderId: string
  senderName: string
  senderAvatar?: string
  text: string
  imageBase64?: string
  timestamp: number
}

const getSenderId = () => {
  let sid = localStorage.getItem('chat_sender_id')
  if (!sid) {
    sid = Math.random().toString(36).substring(2, 10)
    localStorage.setItem('chat_sender_id', sid)
  }
  return sid
}

// Extract YouTube video ID from various YouTube URL formats
function extractYouTubeId(text: string): string | null {
  const patterns = [
    /(?:youtube\.com\/watch\?v=|youtu\.be\/|youtube\.com\/embed\/)([a-zA-Z0-9_-]{11})/,
  ]
  for (const p of patterns) {
    const m = text.match(p)
    if (m) return m[1]
  }
  return null
}

function formatTime(ts: number) {
  const d = new Date(ts)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

function formatDate(ts: number) {
  const d = new Date(ts)
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

// Discord-style avatar placeholder
function Avatar({ name, url, size = 36 }: { name: string; url?: string; size?: number }) {
  if (url) {
    return <img src={url} alt={name} className="chat-avatar" style={{ width: size, height: size, borderRadius: '50%', objectFit: 'cover', flexShrink: 0 }} />
  }
  const color = `hsl(${[...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 360}, 65%, 55%)`
  return (
    <div className="chat-avatar" style={{ width: size, height: size, borderRadius: '50%', background: color, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: '#fff', fontWeight: 700, fontSize: size * 0.42, textTransform: 'uppercase', fontFamily: 'inherit' }}>
      {name[0] ?? '?'}
    </div>
  )
}

// Group messages by sender+time to avoid repeating sender names (Discord style)
function groupMessages(messages: ChatMessage[]) {
  const groups: Array<{ senderId: string; senderName: string; senderAvatar?: string; date: string; msgs: ChatMessage[] }> = []
  for (const msg of messages) {
    const date = formatDate(msg.timestamp)
    const last = groups[groups.length - 1]
    if (last && last.senderId === msg.senderId && last.date === date) {
      last.msgs.push(msg)
    } else {
      groups.push({ senderId: msg.senderId, senderName: msg.senderName, senderAvatar: msg.senderAvatar, date, msgs: [msg] })
    }
  }
  return groups
}

// Render text with YouTube embed
function MessageContent({ text, imageBase64 }: { text: string; imageBase64?: string }) {
  const ytId = text ? extractYouTubeId(text) : null
  return (
    <div className="msg-content">
      {text && <p className="msg-text">{text}</p>}
      {ytId && (
        <div className="yt-embed">
          <iframe
            src={`https://www.youtube.com/embed/${ytId}`}
            title="YouTube video"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
            allowFullScreen
            loading="lazy"
          />
        </div>
      )}
      {imageBase64 && <img src={imageBase64} alt="attached" className="chat-image" />}
    </div>
  )
}

interface GameChatProps {
  gameId: string
  discordUser?: DiscordAuthUser | null
}

function ChatBody({ gameId, discordUser, compact }: GameChatProps & { compact?: boolean }) {
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [inputText, setInputText] = useState('')
  const [sending, setSending] = useState(false)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const senderId = getSenderId()
  const senderName = discordUser?.displayName ?? discordUser?.username ?? `User_${senderId.substring(0, 4)}`
  const senderAvatar = discordUser?.avatarUrl

  useEffect(() => {
    let mounted = true
    const loadLocal = () => {
      invoke<ChatMessage[]>('load_chat_history', { gameId }).then((history) => {
        if (mounted) setMessages(history)
      }).catch(console.error)
    }
    loadLocal()
    invoke('download_from_huggingface', { gameId }).then(() => {
      if (mounted) loadLocal()
    }).catch(console.error)
    return () => { mounted = false }
  }, [gameId])

  useEffect(() => {
    const q = query(
      collection(db, 'chats', gameId, 'messages'),
      orderBy('timestamp', 'desc'),
      limit(50)
    )
    const unsubscribe = onSnapshot(q, (snapshot) => {
      snapshot.docChanges().forEach((change) => {
        if (change.type === 'added') {
          const data = change.doc.data()
          const ts = data.timestamp?.toMillis ? data.timestamp.toMillis() : Date.now()
          const msg: ChatMessage = {
            id: change.doc.id,
            senderId: data.senderId || 'unknown',
            senderName: data.senderName || 'Unknown',
            senderAvatar: data.senderAvatar,
            text: data.text || '',
            imageBase64: data.imageBase64,
            timestamp: ts,
          }
          setMessages((prev) => {
            if (prev.some(m => m.id === msg.id)) return prev
            const next = [...prev, msg].sort((a, b) => a.timestamp - b.timestamp)
            invoke('save_chat_message', { gameId, message: msg }).catch(console.error)
            return next
          })
        }
      })
    })
    return () => unsubscribe()
  }, [gameId])

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  const handleSend = async () => {
    if (!inputText.trim()) return
    setSending(true)
    try {
      await addDoc(collection(db, 'chats', gameId, 'messages'), {
        senderId,
        senderName,
        senderAvatar: senderAvatar ?? null,
        text: inputText.trim(),
        timestamp: serverTimestamp(),
      })
      setInputText('')
      inputRef.current?.focus()
    } catch (e) {
      console.error('Failed to send', e)
    } finally {
      setSending(false)
    }
  }

  const handleImageUpload = () => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = 'image/*'
    input.onchange = (e: Event) => {
      const file = (e.target as HTMLInputElement).files?.[0]
      if (!file) return
      const reader = new FileReader()
      reader.onload = async (re) => {
        const base64 = re.target?.result as string
        if (base64.length > 500000) {
          alert('Image too large. Please use an image under ~300 KB.')
          return
        }
        try {
          await addDoc(collection(db, 'chats', gameId, 'messages'), {
            senderId,
            senderName,
            senderAvatar: senderAvatar ?? null,
            text: '',
            imageBase64: base64,
            timestamp: serverTimestamp(),
          })
        } catch (err) {
          console.error(err)
        }
      }
      reader.readAsDataURL(file)
    }
    input.click()
  }

  const handleClearHistory = async () => {
    if (!confirm('Clear all locally saved chat history?')) return
    await invoke('clear_chat_history', { gameId })
    setMessages([])
  }

  const groups = groupMessages(messages)

  return (
    <>
      <div className="chat-messages" style={{ flex: 1, overflowY: 'auto', padding: '8px 0' }}>
        {groups.length === 0 && (
          <div className="chat-empty">
            <span>No messages yet. Be the first to say something! 👋</span>
          </div>
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
              {group.msgs.map((msg) => (
                <div key={msg.id} className="chat-row">
                  <span className="msg-time">{formatTime(msg.timestamp)}</span>
                  <MessageContent text={msg.text} imageBase64={msg.imageBase64} />
                </div>
              ))}
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      <div className="chat-input-area">
        <button className="icon-btn" onClick={handleImageUpload} title="Attach Image">
          <ImageIcon size={18} />
        </button>
        <input
          ref={inputRef}
          type="text"
          placeholder={`Message as ${senderName}...`}
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && !e.shiftKey && handleSend()}
        />
        <button className="send-btn" onClick={handleSend} disabled={sending || !inputText.trim()}>
          <Send size={18} />
        </button>
        <button className="icon-btn danger" onClick={handleClearHistory} title="Clear Local History" style={{ marginLeft: 2 }}>
          <Trash2 size={15} />
        </button>
      </div>
    </>
  )
}

export function GameChat({ gameId, discordUser }: GameChatProps) {
  const [expanded, setExpanded] = useState(false)

  return (
    <>
      <div className="game-chat-panel">
        <div className="chat-header">
          <span className="chat-header-icon">💬</span>
          <h4>Community Hub</h4>
          <button
            className="icon-btn"
            onClick={() => setExpanded(true)}
            title="Expand chat"
            style={{ marginLeft: 'auto' }}
          >
            <Maximize2 size={15} />
          </button>
        </div>
        <ChatBody gameId={gameId} discordUser={discordUser} compact />
      </div>

      {expanded && typeof document !== 'undefined' && createPortal(
        <div
          className="chat-modal-backdrop"
          role="presentation"
          onClick={() => setExpanded(false)}
        >
          <div
            className="chat-modal"
            role="dialog"
            aria-modal="true"
            aria-label="Community Hub"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="chat-header">
              <span className="chat-header-icon">💬</span>
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
