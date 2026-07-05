use std::fs;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

pub fn get_steam_path() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let steam_key = hkcu.open_subkey("Software\\Valve\\Steam").ok()?;
    let steam_path: String = steam_key.get_value("SteamPath").ok()?;
    Some(PathBuf::from(steam_path))
}

fn main() {
    let steam_path = get_steam_path().unwrap();
    let lua_path = steam_path.join("config").join("stplug-in").join("2495100.lua");
    println!("Trying to delete {:?}", lua_path);
    match fs::remove_file(&lua_path) {
        Ok(_) => println!("Successfully deleted!"),
        Err(e) => println!("Failed to delete: {:?}", e),
    }
}
