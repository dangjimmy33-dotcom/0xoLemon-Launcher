import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { collection, onSnapshot, query, limit, orderBy, addDoc, serverTimestamp } from 'firebase/firestore'
import { db } from '../firebase'
import { Send, Image as ImageIcon, Trash2 } from 'lucide-react'

export interface ChatMessage {
  id: string
  senderId: string
  senderName: string
  text: string
  imageBase64?: string
  timestamp: number
}

// Generate a random stable sender ID for this session
const getSenderId = () => {
  let sid = localStorage.getItem('chat_sender_id')
  if (!sid) {
    sid = Math.random().toString(36).substring(2, 10)
    localStorage.setItem('chat_sender_id', sid)
  }
  return sid
}

const getSenderName = () => {
  return localStorage.getItem('chat_sender_name') || `User_${getSenderId().substring(0, 4)}`
}

export function GameChat({ gameId }: { gameId: string }) {
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [inputText, setInputText] = useState('')
  const [sending, setSending] = useState(false)
  const messagesEndRef = useRef<HTMLDivElement>(null)
  
  const senderId = getSenderId()
  const senderName = getSenderName()

  // 1. Load local history, then fetch from HF, then reload local history
  useEffect(() => {
    let mounted = true
    const loadLocal = () => {
      invoke<ChatMessage[]>('load_chat_history', { gameId }).then((history) => {
        if (mounted) setMessages(history)
      }).catch(console.error)
    }

    loadLocal() // Load immediately for instant feedback
    
    // Background fetch from cold storage
    invoke('download_from_huggingface', { gameId }).then(() => {
      if (mounted) loadLocal() // Reload with newly merged messages
    }).catch(console.error)

    return () => { mounted = false }
  }, [gameId])

  // 2. Subscribe to Firebase real-time
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
          // For initial optimistic updates, serverTimestamp might be null, use Date.now()
          const ts = data.timestamp?.toMillis ? data.timestamp.toMillis() : Date.now()
          
          const msg: ChatMessage = {
            id: change.doc.id,
            senderId: data.senderId || 'unknown',
            senderName: data.senderName || 'Unknown',
            text: data.text || '',
            imageBase64: data.imageBase64,
            timestamp: ts
          }

          setMessages((prev) => {
            if (prev.some(m => m.id === msg.id)) return prev
            const newHistory = [...prev, msg].sort((a, b) => a.timestamp - b.timestamp)
            // Save to local Rust
            invoke('save_chat_message', { gameId, message: msg }).catch(console.error)
            return newHistory
          })
        }
      })
    })

    return () => unsubscribe()
  }, [gameId])

  // Auto scroll
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
        text: inputText.trim(),
        timestamp: serverTimestamp()
      })
      setInputText('')
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
    input.onchange = (e: any) => {
      const file = e.target.files[0]
      if (!file) return
      
      const reader = new FileReader()
      reader.onload = async (re) => {
        let base64 = re.target?.result as string
        // Giới hạn ảnh 500KB để không vượt quá giới hạn 1MB document của Firestore
        if (base64.length > 500000) {
          alert('Ảnh quá lớn. Vui lòng chọn ảnh dưới 300KB.')
          return
        }
        try {
          await addDoc(collection(db, 'chats', gameId, 'messages'), {
            senderId,
            senderName,
            text: '',
            imageBase64: base64,
            timestamp: serverTimestamp()
          })
        } catch (e) {
          console.error(e)
          alert('Lỗi khi gửi ảnh.')
        }
      }
      reader.readAsDataURL(file)
    }
    input.click()
  }

  const handleClearHistory = async () => {
    if (!confirm('Bạn có chắc muốn xóa lịch sử chat lưu trên máy tính không?')) return
    await invoke('clear_chat_history', { gameId })
    setMessages([])
  }

  return (
    <div className="game-chat-panel">
      <div className="chat-header">
        <h4>Community Hub</h4>
        <button className="icon-btn danger" onClick={handleClearHistory} title="Clear Local History">
          <Trash2 size={16} />
        </button>
      </div>

      <div className="chat-messages">
        {messages.map((msg) => {
          const isMe = msg.senderId === senderId
          return (
            <div key={msg.id} className={`chat-message ${isMe ? 'me' : 'them'}`}>
              {!isMe && <span className="sender-name">{msg.senderName}</span>}
              <div className="message-bubble">
                {msg.text && <p>{msg.text}</p>}
                {msg.imageBase64 && <img src={msg.imageBase64} alt="attached" className="chat-image" />}
              </div>
            </div>
          )
        })}
        <div ref={messagesEndRef} />
      </div>

      <div className="chat-input-area">
        <button className="icon-btn" onClick={handleImageUpload} title="Attach Image">
          <ImageIcon size={18} />
        </button>
        <input 
          type="text" 
          placeholder="Type a message..." 
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSend()}
        />
        <button className="send-btn" onClick={handleSend} disabled={sending || (!inputText.trim())}>
          <Send size={18} />
        </button>
      </div>
    </div>
  )
}
