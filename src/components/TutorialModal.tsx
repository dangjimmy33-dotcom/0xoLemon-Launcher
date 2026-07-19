import { createPortal } from 'react-dom'
import { X } from 'lucide-react'
import './TutorialModal.css'

function AmongUsTutorial() {
  return (
    <>
      <h2>📖 Hướng dẫn sửa lỗi đăng nhập Among Us</h2>

      <div style={{ marginTop: 16, lineHeight: 1.6, color: '#e2e8f0' }}>
        <p>
          <strong>🤖 Cách 2: Sử dụng công cụ sửa lỗi tự động</strong><br/>
          Nếu bạn không muốn mất công tạo các tệp không có đuôi mở rộng thủ công, mình đã biên dịch một ứng dụng mã nguồn mở siêu nhỏ để tự động hóa. Cập nhật xong bạn có thể xóa nó đi ngay lập tức.
        </p>

        <p style={{ marginTop: 16 }}><strong>🕹️ Cách sử dụng:</strong></p>
        <ol style={{ paddingLeft: 20 }}>
          <li>Mở ứng dụng <code>Itch_Login_Fixer.exe</code>.<br/>
            <img src="/tutorial-amongus.webp" alt="Itch Login Fixer" style={{ maxWidth: '100%', borderRadius: 8, marginTop: 8, marginBottom: 8 }} />
          </li>
          <li>Nhấp vào nút "Login with itch.io". (Sẽ mở một web).</li>
          <li>Trên web, nhấp vào "Authorize AU-Launcher" để ứng dụng nhận mã token.</li>
          <li>Giữ nguyên tab đó cho đến khi màn hình báo "AUTHORIZATION COMPLETE".</li>
          <li>Quay trở lại ứng dụng, trạng thái sẽ chuyển thành: "Login fixed! You can launch Among Us now".</li>
          <li>Đóng và xóa ứng dụng. Tệp xác thực hiện đã nằm vĩnh viễn trong thư mục game của bạn.</li>
        </ol>

        <div style={{ background: 'rgba(234, 179, 8, 0.1)', borderLeft: '4px solid #eab308', padding: '12px 16px', marginTop: 24, borderRadius: '0 8px 8px 0' }}>
          <p style={{ margin: 0, color: '#fef08a' }}><strong>💡 Lưu ý nhỏ:</strong> Hãy nhớ đóng hoàn toàn và khởi động lại Among Us sau khi chạy công cụ sửa lỗi nếu bạn đang mở game trong lúc thực hiện nhé!</p>
        </div>
      </div>
    </>
  )
}

function Persona3ReloadTutorial() {
  return (
    <>
      <h2>📖 Hướng dẫn vào game Persona 3 Reload</h2>

      <div style={{ marginTop: 16, lineHeight: 1.6, color: '#e2e8f0' }}>
        <p style={{ fontSize: 15 }}>
          Đầu tiên, mọi người hãy tải bản <strong>demo</strong> từ Steam về, sau khi tải xong, chạy 1 lần vào đến menu thì thoát ra và gỡ cài đặt. Sau đó tải game tại launcher và chạy!
        </p>

        <div style={{ marginTop: 16, textAlign: 'center' }}>
          <img src="/tutorial-persona-3.png" alt="Persona 3 Reload Tutorial" style={{ maxWidth: '100%', borderRadius: 8, marginTop: 8, marginBottom: 8, boxShadow: '0 4px 12px rgba(0,0,0,0.5)' }} />
        </div>

        <div style={{ background: 'rgba(34, 197, 94, 0.1)', borderLeft: '4px solid #22c55e', padding: '12px 16px', marginTop: 24, borderRadius: '0 8px 8px 0' }}>
          <p style={{ margin: 0, color: '#86efac' }}><strong>🎉 Chúc các bạn chơi vui vẻ!</strong></p>
        </div>
      </div>
    </>
  )
}

export function TutorialModal({ gameId, onClose }: { gameId: string; onClose: () => void }) {
  // If we have no tutorial for this game, return null
  if (!gameId.includes('among') && gameId !== 'persona-3-reload') return null

  return createPortal(
    <div className="tutorial-overlay" onClick={onClose} style={{
      position: 'fixed',
      inset: 0,
      zIndex: 10000,
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      background: 'rgba(0, 0, 0, 0.75)',
      backdropFilter: 'blur(4px)'
    }}>
      <div className="tutorial-modal" onClick={e => e.stopPropagation()} style={{
        background: '#1a1c24',
        border: '1px solid rgba(255, 255, 255, 0.08)',
        borderRadius: '14px',
        width: '650px',
        maxWidth: '96vw',
        maxHeight: '85vh',
        overflowY: 'auto',
        boxShadow: '0 20px 40px rgba(0, 0, 0, 0.5)',
        padding: '24px',
        position: 'relative',
        color: '#c7ced3'
      }}>
        <button className="tutorial-close" onClick={onClose} style={{
          position: 'absolute',
          top: '16px',
          right: '16px',
          background: 'transparent',
          border: 'none',
          color: '#8a949d',
          cursor: 'pointer',
          padding: '4px'
        }}>
          <X size={18} />
        </button>

        {gameId.includes('among') && <AmongUsTutorial />}
        {gameId === 'persona-3-reload' && <Persona3ReloadTutorial />}

        <div style={{ marginTop: 24 }}>
          <button className="primary-control" onClick={onClose} style={{ width: '100%', padding: '12px', justifyContent: 'center' }}>
            Got it!
          </button>
        </div>
      </div>
    </div>,
    document.body
  )
}
