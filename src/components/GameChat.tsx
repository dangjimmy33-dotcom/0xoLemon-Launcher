import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { invoke } from '@tauri-apps/api/core'
import { collection, onSnapshot, query, limit, orderBy, addDoc, serverTimestamp, doc, deleteDoc, updateDoc } from 'firebase/firestore'
import { db } from '../firebase'
import { open } from '@tauri-apps/plugin-dialog'
import { Send, Image as ImageIcon, Trash2, Maximize2, X, Edit2 } from 'lucide-react'
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
  const [imgError, setImgError] = useState(false)
  const color = `hsl(${[...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 360}, 65%, 55%)`

  if (url && !imgError) {
    return (
      <img
        src={url}
        alt={name}
        className="chat-avatar"
        style={{ width: size, height: size, borderRadius: '50%', objectFit: 'cover', flexShrink: 0 }}
        onError={() => setImgError(true)}
      />
    )
  }
  return (
    <div className="chat-avatar" style={{ width: size, height: size, borderRadius: '50%', background: color, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: '#fff', fontWeight: 700, fontSize: size * 0.42, textTransform: 'uppercase', fontFamily: 'inherit' }}>
      {name[0] ?? '?'}
    </div>
  )
}

// Group messages by sender+time to avoid repeating sender names (Discord style)
function groupMessages(messages: ChatMessage[], mySenderId: string, myAvatar?: string) {
  const groups: Array<{ senderId: string; senderName: string; senderAvatar?: string; date: string; msgs: ChatMessage[] }> = []
  for (const msg of messages) {
    const date = formatDate(msg.timestamp)
    const last = groups[groups.length - 1]
    // Use live avatar for own messages (in case old messages don't have it stored)
    const avatar = msg.senderId === mySenderId ? (myAvatar ?? msg.senderAvatar) : msg.senderAvatar
    if (last && last.senderId === msg.senderId && last.date === date) {
      last.msgs.push(msg)
    } else {
      groups.push({ senderId: msg.senderId, senderName: msg.senderName, senderAvatar: avatar, date, msgs: [msg] })
    }
  }
  return groups
}

// Render text with YouTube embed
function MessageContent({ text, imageBase64, mediaUrl, mediaType }: { text: string; imageBase64?: string; mediaUrl?: string; mediaType?: string }) {
  const ytId = text ? extractYouTubeId(text) : null
  const isVideo = mediaType?.startsWith('video/')
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
      {mediaUrl && (
        isVideo ? (
          <video src={mediaUrl} controls className="chat-video" style={{ maxWidth: 'min(360px, 100%)', maxHeight: 240, borderRadius: 6, marginTop: 6 }} />
        ) : (
          <img src={mediaUrl} alt="attached" className="chat-image" />
        )
      )}
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

  const [editingMsgId, setEditingMsgId] = useState<string | null>(null)
  const [editInput, setEditInput] = useState('')

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
        const data = change.doc.data()
        const ts = data.timestamp?.toMillis ? data.timestamp.toMillis() : Date.now()
        const msg: ChatMessage = {
          id: change.doc.id,
          senderId: data.senderId || 'unknown',
          senderName: data.senderName || 'Unknown',
          senderAvatar: data.senderAvatar,
          text: data.text || '',
          imageBase64: data.imageBase64,
          mediaUrl: data.mediaUrl,
          mediaType: data.mediaType,
          timestamp: ts,
        }
        
        if (change.type === 'added') {
          setMessages((prev) => {
            if (prev.some(m => m.id === msg.id)) return prev
            const next = [...prev, msg].sort((a, b) => a.timestamp - b.timestamp)
            invoke('save_chat_message', { gameId, message: msg }).catch(console.error)
            return next
          })
        } else if (change.type === 'modified') {
          setMessages((prev) => {
            const next = prev.map(m => m.id === msg.id ? msg : m)
            invoke('edit_chat_message', { gameId, messageId: msg.id, newText: msg.text }).catch(console.error)
            return next
          })
        } else if (change.type === 'removed') {
          setMessages((prev) => {
            const next = prev.filter(m => m.id !== change.doc.id)
            invoke('delete_chat_message', { gameId, messageId: change.doc.id }).catch(console.error)
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

  const handleMediaUpload = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Media',
          extensions: ['png', 'jpg', 'jpeg', 'gif', 'mp4', 'webm']
        }]
      })
      if (!selected) return

      const filepath = Array.isArray(selected) ? selected[0] : selected
      if (!filepath) return

      const isVideo = filepath.endsWith('.mp4') || filepath.endsWith('.webm')
      const ext = filepath.split('.').pop() || (isVideo ? 'mp4' : 'png')
      const filename = `${Date.now()}_${Math.random().toString(36).substring(2, 8)}.${ext}`
      const mediaType = isVideo ? `video/${ext}` : `image/${ext}`

      setSending(true)
      
      const mediaUrl = await invoke<string>('upload_chat_media_from_path', { filename, filepath })

      await addDoc(collection(db, 'chats', gameId, 'messages'), {
        senderId,
        senderName,
        senderAvatar: senderAvatar ?? null,
        text: '',
        mediaUrl,
        mediaType,
        timestamp: serverTimestamp(),
      })
    } catch (err) {
      console.error('Media upload failed:', err)
      alert('Failed to upload media. Ensure image is <5MB or video is <20MB.')
    } finally {
      setSending(false)
    }
  }

  const handleClearHistory = async () => {
    if (!confirm('Clear all locally saved chat history?')) return
    await invoke('clear_chat_history', { gameId })
    setMessages([])
  }

  const groups = groupMessages(messages, senderId, senderAvatar)

  const handleEditMessage = async (msgId: string, newText: string) => {
    try {
      await updateDoc(doc(db, 'chats', gameId, 'messages', msgId), {
        text: newText
      })
      setEditingMsgId(null)
      setEditInput('')
    } catch (e) {
      console.error('Failed to edit message', e)
    }
  }

  const handleDeleteMessage = async (msgId: string) => {
    if (!confirm('Delete this message?')) return
    try {
      await deleteDoc(doc(db, 'chats', gameId, 'messages', msgId))
    } catch (e) {
      console.error('Failed to delete message', e)
    }
  }

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
                <div key={msg.id} className="chat-row msg-hover-row">
                  <span className="msg-time">{formatTime(msg.timestamp)}</span>
                  {editingMsgId === msg.id ? (
                    <div className="msg-edit-area">
                      <input 
                        autoFocus
                        value={editInput}
                        onChange={e => setEditInput(e.target.value)}
                        onKeyDown={e => {
                          if (e.key === 'Enter') handleEditMessage(msg.id, editInput)
                          if (e.key === 'Escape') setEditingMsgId(null)
                        }}
                      />
                      <span className="msg-edit-hint">esc to cancel, enter to save</span>
                    </div>
                  ) : (
                    <MessageContent text={msg.text} imageBase64={msg.imageBase64} mediaUrl={msg.mediaUrl} mediaType={msg.mediaType} />
                  )}
                  {msg.senderId === senderId && editingMsgId !== msg.id && (
                    <div className="msg-actions">
                      <button onClick={() => {
                        setEditingMsgId(msg.id)
                        setEditInput(msg.text)
                      }}><Edit2 size={12} /></button>
                      <button className="danger" onClick={() => handleDeleteMessage(msg.id)}><Trash2 size={12} /></button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      <div className="chat-input-area">
        <button className="icon-btn" onClick={handleMediaUpload} title="Attach Image/Video" disabled={sending}>
          <ImageIcon size={20} />
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
