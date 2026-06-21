use std::path::PathBuf;
fn main() {
    let mut sources = Vec::new();
    let source = PathBuf::from("E:/007Launcher/src-tauri/assets/games");
    if let Ok(entries) = std::fs::read_dir(&source) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    sources.push(entry.path().join("core.0xo"));
                }
            }
        }
    }
    
    // We will just read the json directly using string matching since parse_pack is private
    for pack_path in sources {
        if let Ok(bytes) = std::fs::read(&pack_path) {
            let s = String::from_utf8_lossy(&bytes);
            if let Some(idx) = s.find("\"id\":\"") {
                let end = s[idx+6..].find("\"").unwrap_or(0);
                println!("Loaded ID from {}: {}", pack_path.display(), &s[idx+6..idx+6+end]);
            }
        }
    }
}