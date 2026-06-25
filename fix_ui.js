import fs from 'fs';
import path from 'path';

function findFile(dir, filename) {
    if (!fs.existsSync(dir)) return null;
    const files = fs.readdirSync(dir);
    for (const file of files) {
        const fullPath = path.join(dir, file);
        if (fs.statSync(fullPath).isDirectory()) {
            if (fullPath.includes('node_modules') || fullPath.includes('src-tauri') || fullPath.includes('.git')) continue;
            const found = findFile(fullPath, filename);
            if (found) return found;
        } else if (file === filename) {
            return fullPath;
        }
    }
    return null;
}

function patchFile(filename, patches) {
    const filePath = findFile('./src', filename) || findFile('.', filename);
    if (!filePath) {
        console.log(`❌ Không tìm thấy file ${filename}`);
        return;
    }
    
    let content = fs.readFileSync(filePath, 'utf-8');
    let patchedCount = 0;
    
    for (const patch of patches) {
        if (content.includes(patch.find)) {
            content = content.replace(patch.find, patch.replace);
            patchedCount++;
        }
    }
    
    if (patchedCount > 0) {
        fs.writeFileSync(filePath, content, 'utf-8');
        console.log(`✅ Đã vá thành công (${patchedCount} sửa đổi): ${filePath}`);
    } else {
        console.log(`⚠️ Không có thay đổi nào cho: ${filePath} (Có thể đã được chạy trước đó)`);
    }
}

// ============================================
// DANH SÁCH BẢN VÁ (TỐI ƯU UI/UX & ANIMATION)
// ============================================
const patches = [
    {
        file: 'App.css',
        changes: [
            {
                find: "--sans: 'Arya', Inter, ui-sans-serif, 'Segoe UI', Arial, sans-serif;",
                replace: "--sans: 'Inter', 'Segoe UI', Roboto, Arial, sans-serif;"
            },
            {
                find: `top: 7px;\n  left: 7px;`,
                replace: `bottom: 8px;\n  left: 8px;`
            },
            {
                find: `max-width: calc(100% - 14px);\n  pointer-events: none;`,
                replace: `max-width: calc(100% - 16px);\n  pointer-events: none;\n  opacity: 0.95;`
            }
        ]
    },
    {
        file: 'premium.css',
        changes: [
            {
                find: `.home-game-card {\n  position: relative;\n  overflow: hidden;`,
                replace: `.home-game-card {\n  position: relative;\n  overflow: hidden;\n  will-change: transform, box-shadow, border-color;`
            },
            {
                find: `  transform: rotate(25deg);\n  pointer-events: none;\n  opacity: 0;\n  top: -60%;\n  left: -60%;\n  z-index: 1;`,
                replace: `  transform: rotate(25deg);\n  pointer-events: none;\n  opacity: 0;\n  top: -60%;\n  left: -60%;\n  z-index: 1;\n  will-change: transform, opacity;`
            },
            {
                find: `.titlebar-discord-user {\n  max-width: 190px;\n  height: 30px;\n  padding: 3px 8px 3px 4px;\n  display: inline-flex;\n  align-items: center;\n  gap: 7px;\n  border: 1px solid rgba(88, 101, 242, 0.24);\n  border-radius: 9px;\n  color: rgba(239, 242, 248, 0.86);\n  background: rgba(88, 101, 242, 0.09);\n  cursor: pointer;\n}`,
                replace: `.titlebar-discord-user {\n  max-width: 190px;\n  height: 30px;\n  padding: 3px 8px 3px 4px;\n  display: inline-flex;\n  align-items: center;\n  gap: 7px;\n  border: 1px solid rgba(255, 255, 255, 0.1);\n  border-radius: 9px;\n  color: rgba(239, 242, 248, 0.86);\n  background: rgba(255, 255, 255, 0.05);\n  cursor: pointer;\n  transition: all 0.2s ease;\n}`
            },
            {
                find: `.titlebar-discord-user:hover {\n  color: #fff;\n  border-color: rgba(112, 126, 255, 0.44);\n  background: rgba(88, 101, 242, 0.17);\n}`,
                replace: `.titlebar-discord-user:hover {\n  color: #fff;\n  border-color: rgba(255, 255, 255, 0.25);\n  background: rgba(255, 255, 255, 0.12);\n}`
            },
            {
                find: `.home-side-card {\n  position: relative;\n  padding: 14px;\n  overflow: hidden;\n  border: 1px solid rgba(255, 255, 255, 0.08);\n  border-radius: 13px;\n  background:\n    linear-gradient(145deg, rgba(255, 255, 255, 0.035), rgba(255, 255, 255, 0.012)),\n    rgba(12, 17, 23, 0.72);\n}`,
                replace: `.home-side-card {\n  position: relative;\n  padding: 14px;\n  overflow: hidden;\n  border: 1px solid rgba(255, 255, 255, 0.08);\n  border-radius: 13px;\n  background:\n    linear-gradient(145deg, rgba(255, 255, 255, 0.035), rgba(255, 255, 255, 0.012)),\n    rgba(12, 17, 23, 0.72);\n  display: flex;\n  flex-direction: column;\n  height: 100%;\n}\n.home-side-card > button {\n  margin-top: auto;\n}`
            },
            {
                find: `.update-button.downloading-btn {\n  color: #fff;\n  background: linear-gradient(180deg, #4da4ff, #2978d1);\n  border-color: rgba(100, 181, 255, 0.78);\n  box-shadow: 0 8px 24px rgba(41, 120, 209, 0.24);\n  animation: pulse-running 1.8s ease-in-out infinite;\n}`,
                replace: `.update-button.downloading-btn {\n  position: relative;\n  overflow: hidden;\n  color: #fff;\n  background: rgba(255, 255, 255, 0.05);\n  border-color: rgba(100, 181, 255, 0.4);\n  z-index: 1;\n}\n.update-button.downloading-btn::before {\n  content: '';\n  position: absolute;\n  top: 0;\n  left: 0;\n  bottom: 0;\n  width: var(--progress, 0%);\n  background: linear-gradient(90deg, #2978d1, #4da4ff);\n  z-index: -1;\n  transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1);\n}\n.update-button.downloading-btn:not(:disabled) {\n  box-shadow: 0 8px 24px rgba(41, 120, 209, 0.24);\n}`
            }
        ]
    },
    {
        file: 'layout.tsx',
        changes: [
            {
                find: "import { Cloud, Database, Download, Home, Image as ImageIcon, Library, RefreshCcw, Settings, ShoppingBag, Wifi } from 'lucide-react'",
                replace: "import { Cloud, Database, Download, Home, Image as ImageIcon, Library, RefreshCcw, Settings, ShoppingBag, Wifi, WifiOff } from 'lucide-react'"
            },
            {
                find: `const items = [\n    [t.nav.home, Home],\n    [t.nav.store, ShoppingBag],\n    [t.nav.library, Library],\n    [t.nav.updates, RefreshCcw],\n    [t.nav.downloads, Download],\n    [t.nav.cloudSaves, Cloud],\n    [t.nav.cache, Database],\n    [t.nav.settings, Settings],\n  ] as const`,
                replace: `const mainItems = [\n    [t.nav.home, Home],\n    [t.nav.store, ShoppingBag],\n    [t.nav.library, Library],\n    [t.nav.updates, RefreshCcw],\n    [t.nav.downloads, Download],\n    [t.nav.cloudSaves, Cloud],\n    [t.nav.cache, Database],\n  ] as const`
            },
            {
                find: `{items.map(([label, Icon]) => (`,
                replace: `{mainItems.map(([label, Icon]) => (`
            },
            {
                find: `<div className="sidebar-status">\n        <div className={\`status-line\${connectionLabel === 'Offline' ? ' offline' : ''}\`}>\n          <Wifi size={16} />\n          <span>{connectionLabel}</span>\n        </div>\n      </div>`,
                replace: `<div className="sidebar-status" style={{ marginTop: 'auto', display: 'flex', flexDirection: 'column', gap: '10px' }}>\n        <button\n          className={activeTab === 'Settings' ? 'nav-item active' : 'nav-item'}\n          type="button"\n          onClick={() => onSelect('Settings')}\n          style={{ width: '100%', padding: '0 22px', height: '44px', borderRadius: '6px' }}\n        >\n          <Settings size={20} />\n          <span>{t.nav.settings}</span>\n        </button>\n\n        <div className={\`status-line\${connectionLabel === 'Offline' ? ' offline' : ''}\`} style={{ paddingLeft: '4px' }}>\n          {connectionLabel === 'Offline' ? <WifiOff size={16} /> : <Wifi size={16} />}\n          <span>{connectionLabel}</span>\n        </div>\n      </div>`
            }
        ]
    },
    {
        file: 'App.tsx',
        changes: [
            {
                find: `            selectedVersion={targetVersion}\n          />`,
                replace: `            selectedVersion={targetVersion}\n            progress={progress}\n          />`
            },
            {
                find: `<div key={activeTab} className={reducedMotion ? undefined : 'tab-enter'}>`,
                replace: `<AnimatePresence mode="wait">\n          <motion.div \n            key={activeTab}\n            initial={reducedMotion ? false : { opacity: 0, y: 8 }}\n            animate={{ opacity: 1, y: 0 }}\n            exit={reducedMotion ? undefined : { opacity: 0, y: -8 }}\n            transition={{ duration: 0.18, ease: "easeOut" }}\n            className="tab-content-wrapper"\n          >`
            },
            {
                find: `onClearCache={() => void clearLauncherCache()}\n        />\n        )}\n        </div>`,
                replace: `onClearCache={() => void clearLauncherCache()}\n        />\n        )}\n          </motion.div>\n        </AnimatePresence>`
            }
        ]
    },
    {
        file: 'library.tsx',
        changes: [
            {
                find: `installMode: boolean\n  selectedVersion: string\n}) {`,
                replace: `installMode: boolean\n  selectedVersion: string\n  progress?: number\n}) {`
            },
            {
                find: `className={\`update-button\${isJobRunning ? ' downloading-btn' : ''}\`}\n              type="button"\n              onClick={onUpdate}\n              disabled={isJobRunning || !canUpdate}\n            >`,
                replace: `className={\`update-button\${isJobRunning ? ' downloading-btn' : ''}\`}\n              type="button"\n              onClick={onUpdate}\n              disabled={isJobRunning || !canUpdate}\n              style={isJobRunning ? { '--progress': \`\${progress ?? 0}%\` } as React.CSSProperties : undefined}\n            >`
            },
            {
                find: `<button className={playClass} type="button" onClick={onPlay} disabled={playDisabled}>`,
                replace: `<button \n                className={playClass} \n                type="button" \n                onClick={onPlay} \n                disabled={playDisabled}\n                style={isJobRunning ? { '--progress': \`\${progress ?? 0}%\` } as React.CSSProperties : undefined}\n              >`
            }
        ]
    },
    {
        file: 'HomeView.tsx',
        changes: [
            {
                find: `export function HomeView({`,
                replace: `import { memo } from 'react'\n\nexport const HomeView = memo(function HomeView({`
            },
            {
                find: `    </section>\n  )\n}`,
                replace: `    </section>\n  )\n})`
            }
        ]
    },
    {
        file: 'ActiveView.tsx',
        changes: [
            {
                find: `export function ActiveView({`,
                replace: `import { memo } from 'react'\n\nexport const ActiveView = memo(function ActiveView({`
            },
            {
                find: `  return null\n}`,
                replace: `  return null\n})`
            }
        ]
    }
];

console.log('🚀 Bắt đầu quét và nâng cấp UI/UX cho 0xoLemon...');
for (const task of patches) {
    patchFile(task.file, task.changes);
}
console.log('🎉 Hoàn tất quá trình nâng cấp!');