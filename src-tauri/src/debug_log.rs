use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init_debug_log(log_path: PathBuf) {
    let mut file = LOG_FILE.lock().unwrap();
    *file = Some(log_path);
}

pub fn debug_log(message: &str) {
    // Also print to stderr for dev mode
    eprintln!("{}", message);
    
    // Write to file if initialized
    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(path) = guard.as_ref() {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{}", message);
            }
        }
    }
}

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {
        $crate::debug_log::debug_log(&format!($($arg)*))
    };
}
