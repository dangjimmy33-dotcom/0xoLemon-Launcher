use std::env;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;

fn main() {
    let mut args = env::args().skip(1);
    let Some(archive) = args.next() else {
        eprintln!("Usage: debug_extract <archive.7z>");
        std::process::exit(2);
    };
    let dest_path = PathBuf::from(archive);

    if dest_path.exists() {
        let size = std::fs::metadata(&dest_path).unwrap().len();
        println!(
            "File exists: {:?}, size: {} MB",
            dest_path,
            size / 1_000_000
        );
    } else {
        println!("File NOT found: {:?}", dest_path);
        return;
    }

    println!("\nTesting archive with 7z.exe...");
    let seven_zip = env::var("SEVEN_ZIP_PATH")
        .unwrap_or_else(|_| "C:\\Program Files\\7-Zip\\7z.exe".to_string());
    let password = env::var("OXO_ARCHIVE_PASSWORD").ok();
    let mut command = std::process::Command::new(seven_zip);
    command.arg("t").arg(&dest_path);
    if let Some(password) = password {
        command.arg(format!("-p{password}"));
    }

    #[cfg(windows)]
    command.creation_flags(0x08000000);

    let result = command.output();

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            println!("Exit code: {}", out.status.code().unwrap_or(-1));
            println!("Stdout: {}", stdout);
            if !stderr.is_empty() {
                println!("Stderr: {}", stderr);
            }
        }
        Err(e) => println!("Failed to run 7z: {:?}", e),
    }
}
