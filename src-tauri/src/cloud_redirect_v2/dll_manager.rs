// DLL installation and management for CloudRedirect

use std::fs;
use std::path::Path;

/// Install CloudRedirect DLL to Steam directory
pub fn install_dll(dll_source: &Path, steam_path: &Path) -> Result<(), String> {
    let dest_dll = steam_path.join("0xoCloudRedirect.dll");
    let marker_file = steam_path.join(".0xo-cloud-redirect-enabled");

    // Check if DLL source exists
    if !dll_source.exists() {
        return Err(format!("CloudRedirect DLL not found at {:?}", dll_source));
    }

    // Backup existing file if present
    if dest_dll.exists() {
        let backup = steam_path.join("0xoCloudRedirect.dll.backup");
        fs::copy(&dest_dll, &backup)
            .map_err(|e| format!("Failed to backup existing DLL: {}", e))?;
    }

    // Copy DLL
    fs::copy(dll_source, &dest_dll)
        .map_err(|e| format!("Failed to copy CloudRedirect DLL: {}", e))?;

    // Create marker file
    fs::write(&marker_file, "")
        .map_err(|e| format!("Failed to create marker file: {}", e))?;

    Ok(())
}

/// Uninstall CloudRedirect DLL from Steam directory
pub fn uninstall_dll(steam_path: &Path) -> Result<(), String> {
    let dest_dll = steam_path.join("0xoCloudRedirect.dll");
    let marker_file = steam_path.join(".0xo-cloud-redirect-enabled");
    let backup = steam_path.join("0xoCloudRedirect.dll.backup");

    // Remove DLL
    if dest_dll.exists() {
        fs::remove_file(&dest_dll)
            .map_err(|e| format!("Failed to remove CloudRedirect DLL: {}", e))?;
    }

    // Remove marker
    if marker_file.exists() {
        fs::remove_file(&marker_file)
            .map_err(|e| format!("Failed to remove marker file: {}", e))?;
    }

    // Remove backup file if exists (don't restore it)
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|e| format!("Failed to remove backup file: {}", e))?;
    }

    Ok(())
}

/// Check if CloudRedirect DLL is installed
pub fn is_installed(steam_path: &Path) -> bool {
    let dest_dll = steam_path.join("0xoCloudRedirect.dll");
    let marker_file = steam_path.join(".0xo-cloud-redirect-enabled");
    dest_dll.exists() && marker_file.exists()
}
