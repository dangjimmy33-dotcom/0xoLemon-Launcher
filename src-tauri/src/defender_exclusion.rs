use std::process::Command;
use std::path::Path;

/// Attempt to add Windows Defender exclusion for a path
/// Returns Ok(true) if successful, Ok(false) if already excluded or no action needed
pub fn add_defender_exclusion(path: &Path) -> Result<bool, String> {
    if !cfg!(target_os = "windows") {
        return Ok(false); // Not Windows
    }
    
    let path_str = path.to_string_lossy().to_string();
    
    eprintln!("[DEFENDER] Attempting to add exclusion for: {}", path_str);
    
    // Check if already excluded
    #[cfg(target_os = "windows")]
    let check_output = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle", "Hidden",
                "-Command",
                &format!(
                    "Get-MpPreference | Select-Object -ExpandProperty ExclusionPath | Where-Object {{ $_ -eq '{}' }}",
                    path_str.replace("'", "''")
                )
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };
    
    #[cfg(not(target_os = "windows"))]
    let check_output = Command::new("powershell")
        .args(&["-Command", "echo ''"])
        .output();
    
    if let Ok(output) = check_output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().contains(&path_str) {
            eprintln!("[DEFENDER] Path already excluded");
            return Ok(false);
        }
    }
    
    // Try to add exclusion (requires admin)
    // Use CREATE_NO_WINDOW flag to hide console window
    #[cfg(target_os = "windows")]
    let result = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle", "Hidden",
                "-Command",
                &format!(
                    "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile','-NonInteractive','-WindowStyle','Hidden','-Command','Add-MpPreference -ExclusionPath ''{}''' -WindowStyle Hidden -Wait",
                    path_str.replace("'", "''")
                )
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };
    
    #[cfg(not(target_os = "windows"))]
    let result = Command::new("powershell")
        .args(&["-Command", "echo 'Not Windows'"])
        .output();
    
    match result {
        Ok(output) => {
            if output.status.success() {
                eprintln!("[DEFENDER] Successfully added exclusion (admin prompt may have appeared)");
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("[DEFENDER] Failed to add exclusion: {}", stderr);
                Err(format!("Failed to add Windows Defender exclusion. Please add manually: {}", path_str))
            }
        }
        Err(e) => {
            eprintln!("[DEFENDER] Error executing command: {}", e);
            Err(format!("Could not add Windows Defender exclusion: {}", e))
        }
    }
}

/// Suggest adding exclusion without actually doing it (for UI notification)
pub fn suggest_defender_exclusion(path: &Path) -> String {
    format!(
        "To avoid download issues, add Windows Defender exclusion:\n\
        1. Open Windows Security\n\
        2. Go to Virus & threat protection → Manage settings\n\
        3. Scroll to Exclusions → Add or remove exclusions\n\
        4. Add folder: {}",
        path.display()
    )
}

/// Check if path is likely excluded (best-effort check)
pub fn is_likely_excluded(path: &Path) -> bool {
    if !cfg!(target_os = "windows") {
        return true; // Not applicable
    }
    
    let path_str = path.to_string_lossy().to_string();
    
    #[cfg(target_os = "windows")]
    let check_output = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle", "Hidden",
                "-Command",
                &format!(
                    "Get-MpPreference | Select-Object -ExpandProperty ExclusionPath | Where-Object {{ $_ -eq '{}' }}",
                    path_str.replace("'", "''")
                )
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };
    
    #[cfg(not(target_os = "windows"))]
    let check_output = Command::new("powershell")
        .args(&["-Command", "echo ''"])
        .output();
    
    if let Ok(output) = check_output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.trim().contains(&path_str);
    }
    
    false
}
