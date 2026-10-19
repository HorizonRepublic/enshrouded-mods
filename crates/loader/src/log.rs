//! A plain append-only log beside the game's own logs.

use std::fmt::Arguments;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Opens `logs/ensh-mods.log` in `dir`, creating `logs` if needed.
pub fn open(dir: &std::path::Path) {
    let logs = dir.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("ensh-mods.log"));
    if let (Ok(file), Ok(mut slot)) = (file, FILE.lock()) {
        *slot = Some(file);
    }
}

pub fn write(level: &str, args: Arguments) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (h, m, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if let Ok(mut slot) = FILE.lock()
        && let Some(file) = slot.as_mut()
    {
        let millis = now.subsec_millis();
        let _ = writeln!(file, "{h:02}:{m:02}:{s:02}.{millis:03}Z [{level}] {args}");
        let _ = file.flush();
    }
}

#[macro_export]
macro_rules! info {
    ($($t:tt)*) => { $crate::log::write("info", format_args!($($t)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => { $crate::log::write("warn", format_args!($($t)*)) };
}
