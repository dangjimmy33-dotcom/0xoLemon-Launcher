// Atomic file write: write .tmp → fsync → rename.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

fn write_flush_and_publish<F>(path: &Path, writer: F) -> io::Result<()>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let tmp = with_extension_suffix(path, ".tmp");
    let result = (|| {
        {
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)?;
            writer(&mut f)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn atomic_write_all_bytes(path: &Path, data: &[u8]) -> io::Result<()> {
    write_flush_and_publish(path, |f| f.write_all(data))
}

pub fn atomic_copy(source: &Path, dest: &Path) -> io::Result<()> {
    write_flush_and_publish(dest, |f| {
        let mut src = File::open(source)?;
        let mut buf = vec![0u8; 81920];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 { break; }
            f.write_all(&buf[..n])?;
        }
        Ok(())
    })
}

pub fn with_extension_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    std::path::PathBuf::from(s)
}
