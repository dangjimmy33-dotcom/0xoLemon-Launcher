use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Manager};

static BYPASS_LOCK: Mutex<()> = Mutex::new(());
static PAC_SERVER_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamFixStatus {
    pub active: bool,
    pub embedded: bool,
    pub message: Option<String>,
}

const PAC_CONTENT: &str = r#"function FindProxyForURL(url, host) {
    var h = (host || "").toLowerCase();
    if (h.indexOf("steam") !== -1 || 
        h.indexOf("valve") !== -1 ||
        h.indexOf("akamaihd.net") !== -1 ||
        h.indexOf("akamaized.net") !== -1 ||
        h.indexOf("underlords.com") !== -1 ||
        h.indexOf("dota2.com") !== -1 ||
        h.indexOf("counter-strike.net") !== -1) {
        return "SOCKS5 127.0.0.1:1080; SOCKS 127.0.0.1:1080; DIRECT";
    }
    return "DIRECT";
}
"#;

fn ensure_pac_http_server() {
    if PAC_SERVER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(move || {
        if let Ok(listener) = TcpListener::bind("127.0.0.1:1082") {
            for stream in listener.incoming() {
                if let Ok(mut s) = stream {
                    let mut buf = [0u8; 1024];
                    let _ = s.read(&mut buf);
                    let body = PAC_CONTENT;
                    let res = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = s.write_all(res.as_bytes());
                    let _ = s.flush();
                }
            }
        }
    });
}

fn find_ciadpi_binary(app: &AppHandle) -> Option<PathBuf> {
    let mut search_paths = Vec::new();

    if let Ok(res_dir) = app.path().resource_dir() {
        search_paths.push(res_dir.join("resources/tools/byedpi/ciadpi.exe"));
        search_paths.push(res_dir.join("tools/byedpi/ciadpi.exe"));
    }

    if let Ok(app_dir) = app.path().app_data_dir() {
        search_paths.push(app_dir.join("tools/byedpi/ciadpi.exe"));
    }

    // Relative paths for dev mode
    search_paths.push(PathBuf::from(r"src-tauri\resources\tools\byedpi\ciadpi.exe"));
    search_paths.push(PathBuf::from(r"resources\tools\byedpi\ciadpi.exe"));
    search_paths.push(PathBuf::from(r"E:\007Launcher\src-tauri\resources\tools\byedpi\ciadpi.exe"));

    for p in search_paths {
        if p.exists() {
            return Some(p);
        }
    }

    None
}

pub fn is_ciadpi_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if let Ok(output) = Command::new("tasklist")
            .args(&["/FI", "IMAGENAME eq ciadpi.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let out = String::from_utf8_lossy(&output.stdout);
            return out.to_lowercase().contains("ciadpi.exe");
        }
    }
    false
}

fn is_pac_configured() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if let Ok(output) = Command::new("reg")
            .args(&[
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "AutoConfigURL",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            return output.status.success();
        }
    }
    false
}

#[cfg(target_os = "windows")]
fn notify_wininet_proxy_change() {
    #[link(name = "wininet")]
    extern "system" {
        fn InternetSetOptionW(
            h_internet: *mut std::ffi::c_void,
            dw_option: u32,
            lp_buffer: *mut std::ffi::c_void,
            dw_buffer_length: u32,
        ) -> i32;
    }
    const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
    const INTERNET_OPTION_REFRESH: u32 = 37;

    unsafe {
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
    }
}

pub fn get_steam_fix_status_impl(app: &AppHandle) -> Result<SteamFixStatus, String> {
    let has_binary = find_ciadpi_binary(app).is_some();
    let is_running = is_ciadpi_running();
    let is_pac = is_pac_configured();

    Ok(SteamFixStatus {
        active: is_running && is_pac,
        embedded: has_binary,
        message: if is_running {
            Some("Steam DPI bypass engine active (127.0.0.1:1080)".to_string())
        } else {
            None
        },
    })
}

pub fn toggle_steam_bypass_impl(app: &AppHandle, enable: bool) -> Result<bool, String> {
    let _guard = BYPASS_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    if !cfg!(target_os = "windows") {
        return Err("Steam bypass is only supported on Windows".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        if enable {
            let exe_path = find_ciadpi_binary(app)
                .ok_or_else(|| "Embedded DPI bypass binary (ciadpi.exe) not found".to_string())?;

            // 1. Ensure PAC HTTP micro-server is running
            ensure_pac_http_server();

            // 2. Kill old ciadpi / wireproxy instances
            let _ = Command::new("taskkill")
                .args(&["/F", "/IM", "ciadpi.exe"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = Command::new("taskkill")
                .args(&["/F", "/IM", "wireproxy.exe"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // 3. Start ciadpi with high capacity and clean TLS record splitting
            let child = Command::new(&exe_path)
                .args(&[
                    "-i", "127.0.0.1",
                    "-p", "1080",
                    "-c", "4096",
                    "-b", "65536",
                    "-r", "1+s",
                    "-s", "1+s",
                    "-m", "3",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Failed to start DPI bypass: {}", e))?;

            std::mem::forget(child);

            // 4. Disable global ProxyEnable so normal apps (YouTube, Discord) go 100% DIRECT
            let _ = Command::new("reg")
                .args(&[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "ProxyEnable",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    "0",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let _ = Command::new("reg")
                .args(&[
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "ProxyServer",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // 5. Configure AutoConfigURL with local HTTP PAC (Chromium and WinINet fully support this)
            let _ = Command::new("reg")
                .args(&[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "AutoConfigURL",
                    "/t",
                    "REG_SZ",
                    "/d",
                    "http://127.0.0.1:1082/proxy.pac",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // 6. Notify WinINet and flush DNS
            notify_wininet_proxy_change();
            let _ = Command::new("ipconfig")
                .arg("/flushdns")
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            Ok(true)
        } else {
            // Disable PAC and Proxy
            let _ = Command::new("reg")
                .args(&[
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "AutoConfigURL",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let _ = Command::new("reg")
                .args(&[
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "ProxyEnable",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    "0",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let _ = Command::new("reg")
                .args(&[
                    "delete",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "ProxyServer",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // Kill ciadpi
            let _ = Command::new("taskkill")
                .args(&["/F", "/IM", "ciadpi.exe"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // Notify WinINet and flush DNS
            notify_wininet_proxy_change();
            let _ = Command::new("ipconfig")
                .arg("/flushdns")
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            Ok(false)
        }
    }

    #[cfg(not(target_os = "windows"))]
    Ok(false)
}

#[tauri::command]
pub async fn get_steam_fix_status(app: AppHandle) -> Result<SteamFixStatus, String> {
    get_steam_fix_status_impl(&app)
}

#[tauri::command]
pub async fn toggle_steam_bypass(app: AppHandle, enable: bool) -> Result<bool, String> {
    toggle_steam_bypass_impl(&app, enable)
}

// Backward compatibility commands
#[tauri::command]
pub async fn get_steam_vn_fix_status(app: AppHandle) -> Result<bool, String> {
    let st = get_steam_fix_status_impl(&app)?;
    Ok(st.active)
}

#[tauri::command]
pub async fn toggle_steam_vn_fix(app: AppHandle, enable: bool) -> Result<bool, String> {
    toggle_steam_bypass_impl(&app, enable)
}

#[tauri::command]
pub async fn toggle_steam_warp(app: AppHandle, enable: bool) -> Result<bool, String> {
    toggle_steam_bypass_impl(&app, enable)
}

#[tauri::command]
pub async fn toggle_steam_dns(app: AppHandle, enable: bool) -> Result<bool, String> {
    toggle_steam_bypass_impl(&app, enable)
}
