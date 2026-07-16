#![deny(unsafe_code)]

pub mod compressor;
pub mod pipeline;
pub mod split_mode;
pub mod stream_mode;
pub mod tui;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex, OnceLock};

pub static VERBOSE: AtomicBool = AtomicBool::new(false);
pub static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static LOG_SCROLL_OFFSET: AtomicUsize = AtomicUsize::new(0);

pub fn get_log_buffer() -> &'static Arc<Mutex<Vec<String>>> {
    static LOG_BUFFER: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOG_BUFFER.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

#[macro_export]
macro_rules! log_verbose {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        if $crate::TUI_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(mut buffer) = $crate::get_log_buffer().lock() {
                buffer.push(msg.clone());
                if buffer.len() > 100 {
                    buffer.remove(0);
                }
            }
        } else if $crate::VERBOSE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[INFO] {}", msg);
        }
    };
}

pub static OUTPUT_FILE_PATH: OnceLock<Arc<Mutex<Option<PathBuf>>>> = OnceLock::new();

pub fn get_output_path_mutex() -> &'static Arc<Mutex<Option<PathBuf>>> {
    OUTPUT_FILE_PATH.get_or_init(|| Arc::new(Mutex::new(None)))
}

pub fn cleanup_output_file() {
    if let Some(mutex) = OUTPUT_FILE_PATH.get() {
        if let Ok(guard) = mutex.lock() {
            if let Some(ref path) = *guard {
                if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}
