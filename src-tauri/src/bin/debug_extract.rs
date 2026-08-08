use std::path::PathBuf;
use std::os::windows::process::CommandExt;

fn main() {
    let game_dir = "E:\\Compressed\\FC26_165-001\\UpdatedFiles_20260724_113859";
    let dest_path = PathBuf::from(game_dir).join("FC.26.NOTACRACK.7z");
    
    if dest_path.exists() {
        let size = std::fs::metadata(&dest_path).unwrap().len();
        println!("File exists: {:?}, size: {} MB", dest_path, size / 1_000_000);
    } else {
        println!("File NOT found: {:?}", dest_path);
        return;
    }

    println!("\nTesting extraction with 7z.exe + password '0xoLemon.dll'...");
    let seven_zip = "C:\\Program Files\\7-Zip\\7z.exe";
    
    let result = std::process::Command::new(seven_zip)
        .args([
            "t", // test mode (no actual extraction)
            dest_path.to_str().unwrap(),
            "-p0xoLemon.dll",
        ])
        .creation_flags(0x08000000)
        .output();

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            println!("Exit code: {}", out.status.code().unwrap_or(-1));
            println!("Stdout: {}", stdout);
            if !stderr.is_empty() { println!("Stderr: {}", stderr); }
        }
        Err(e) => println!("Failed to run 7z: {:?}", e),
    }
}
