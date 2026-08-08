// CloudRedirect CLI (STFixer).

// Several PE/signature helpers (payload P1-P3, hook finders) are kept for
// completeness but aren't exercised by the offline-setup flow.
#![allow(dead_code)]

mod crypto;
mod embedded;
mod file_util;
mod patcher;
mod pe;
mod signatures;
mod steam_detector;

use std::path::Path;
use std::process::exit;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        exit(0);
    }

    let command = args[0]
        .to_lowercase()
        .trim_start_matches('/')
        .trim_start_matches('-')
        .to_string();

    let code = match command.as_str() {
        "stfixer" => run_stfixer(),
        "help" | "?" => {
            print_help();
            0
        }
        _ => {
            eprintln!("Unknown command: {}", args[0]);
            eprintln!();
            print_help();
            1
        }
    };
    exit(code);
}

fn print_help() {
    println!("CloudRedirect v{}-CLI", VERSION);
    println!();
    println!("Usage: CloudRedirectCLI.exe <command>");
    println!();
    println!("Commands:");
    println!("  /stfixer    Apply STFixer patches (fixes Capcom saves, manifest downloads)");
    println!("  /help       Show this help message");
}

fn run_stfixer() -> i32 {
    println!("=== CloudRedirect STFixer ===");
    println!();

    let steam_path = match steam_detector::find_steam_path() {
        Some(p) => p,
        None => {
            eprintln!("ERROR: Steam installation not found.");
            eprintln!("Checked registry and common install paths.");
            return 1;
        }
    };
    println!("Steam: {}", steam_path.display());

    match steam_detector::get_steam_version(&steam_path) {
        None => println!("WARNING: Could not read Steam version -- continuing anyway."),
        Some(v) if !steam_detector::is_supported_steam_version(v) => {
            println!("WARNING: Steam version {} not in whitelist -- continuing anyway.", v)
        }
        Some(v) => println!("Steam version: {} (OK)", v),
    }

    // Close Steam if running (the DLL/exe are locked while it runs).
    if steam_detector::is_steam_running() {
        println!("Steam is running -- shutting it down...");
        let steam_exe = steam_path.join("steam.exe");
        if steam_exe.is_file() {
            let _ = std::process::Command::new(&steam_exe)
                .arg("-shutdown")
                .spawn();
        }
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !steam_detector::is_steam_running() {
                break;
            }
        }
        if steam_detector::is_steam_running() {
            println!("Graceful shutdown timed out, killing Steam...");
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/IM", "steam.exe"])
                .output();
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        println!("Steam closed.");
    }

    let p = patcher::Patcher::new(steam_path.clone());

    // Download core DLLs if missing.
    if !p.has_core_dll() {
        println!();
        println!("Downloading SteamTools core DLLs...");
        let repair = p.repair_core_dlls();
        if !repair.succeeded {
            eprintln!(
                "FAILED: {}",
                repair.error.unwrap_or_else(|| "Could not download core DLLs".into())
            );
            return 1;
        }
        println!("OK");
    }

    // Apply STFixer patches.
    println!();
    println!("Applying STFixer patches...");
    let result = p.apply_offline_setup();
    if !result.succeeded {
        eprintln!("FAILED: {}", result.error.unwrap_or_default());
        return 1;
    }
    println!("OK");

    // Patch SteamTools.exe so it doesn't overwrite Core.dll or prompt on startup.
    println!();
    println!("Patching SteamTools.exe...");
    match p.patch_steamtools_exe() {
        1 => println!("OK"),
        0 => println!("Skipped (SteamTools.exe not installed)."),
        _ => println!(
            "WARNING: SteamTools.exe patch failed -- see messages above. \
             STFixer patches still applied; you may be prompted by SteamTools on startup."
        ),
    }

    // Deploy DLL.
    println!();
    println!("Deploying cloud_redirect.dll...");
    if !embedded::dll_available() {
        eprintln!("ERROR: cloud_redirect.dll is not embedded in this build.");
        return 1;
    }
    let dll_dest = steam_path.join("cloud_redirect.dll");
    if let Some(err) = embedded::deploy_dll(&dll_dest) {
        eprintln!("FAILED: {}", err);
        return 1;
    }
    println!("Deployed to {}", dll_dest.display());

    // Enable auto-update in config.json so the DLL stays current.
    enable_auto_update(&steam_path);

    println!();
    println!("All patches applied. Start Steam to use STFixer.");
    0
}

fn enable_auto_update(steam_path: &Path) {
    let config_dir = steam_path.join("cloud_redirect");
    let config_path = config_dir.join("config.json");
    if std::fs::create_dir_all(&config_dir).is_err() {
        return;
    }
    let json = if let Ok(existing) = std::fs::read_to_string(&config_path) {
        if existing.contains("auto_update_dll") {
            existing
        } else {
            let trimmed = existing.trim_end().trim_end_matches('}');
            format!("{},\n  \"auto_update_dll\": true\n}}", trimmed)
        }
    } else {
        "{\n  \"auto_update_dll\": true\n}".to_string()
    };
    if std::fs::write(&config_path, json).is_ok() {
        println!("DLL auto-update enabled.");
    }
}
