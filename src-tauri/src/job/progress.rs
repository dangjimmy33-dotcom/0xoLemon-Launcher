pub(super) fn verify_percent(
    checked_bytes: u64,
    total_bytes: u64,
    checked_files: usize,
    total_files: usize,
) -> f32 {
    if total_bytes > 0 {
        (checked_bytes as f32 / total_bytes as f32).clamp(0.0, 1.0)
    } else if total_files > 0 {
        (checked_files as f32 / total_files as f32).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

pub(super) fn progress_fraction(done: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        done as f32 / total as f32
    }
}

pub(super) fn byte_progress(done: u64, total: u64) -> f32 {
    if total == 0 {
        1.0
    } else {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    }
}

pub(super) fn overall_progress(step_index: usize, step_progress: f32) -> f32 {
    (step_index as f32 + step_progress) / 6.0
}

pub(super) fn human_bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if value == 0 {
        return "0 B".to_string();
    }
    let mut size = value as f64;
    let mut index = 0usize;
    while size >= 1024.0 && index < UNITS.len() - 1 {
        size /= 1024.0;
        index += 1;
    }
    format!("{size:.2} {}", UNITS[index])
}
