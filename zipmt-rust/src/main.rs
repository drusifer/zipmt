#![deny(unsafe_code)]

pub mod compressor;
pub mod split_mode;
pub mod stream_mode;
pub mod tui;

use std::fs::File;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use clap::Parser;
use compressor::{Compressor, GzipCompressor, Bzip2Compressor, XzCompressor, ZipError};

// We use lazy_static to hold the output file path for cleanup on Ctrl-C.
// Since we don't have lazy_static in Cargo.toml dependencies, let's add it first!
// Wait! Let's check if we can write a clean once_cell or standard atomic instead of lazy_static to avoid too many dependencies.
// Yes! A standard std::sync::OnceLock is available in Rust 1.70+ and completely standard!
// OnceLock is perfect and doesn't require any external crate!
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering, AtomicU64, AtomicUsize};

pub static VERBOSE: AtomicBool = AtomicBool::new(false);
pub static TUI_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static THROTTLE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
pub static IS_PAUSED: AtomicBool = AtomicBool::new(false);
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

fn get_output_path_mutex() -> &'static Arc<Mutex<Option<PathBuf>>> {
    OUTPUT_FILE_PATH.get_or_init(|| Arc::new(Mutex::new(None)))
}

pub fn cleanup_output_file() {
    if let Some(mutex) = OUTPUT_FILE_PATH.get() {
        if let Ok(guard) = mutex.lock() {
            if let Some(ref path) = *guard {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

#[derive(Parser, Clone, Debug)]
#[command(name = "zipmt-rust", version, about = "Parallel Multi-Format Compression Tool in Rust")]
struct Args {
    /// Input file path. If omitted or "-", reads from stdin.
    #[arg(index = 1)]
    input_file: Option<String>,

    /// Output file path. Defaults to <input_file>.<ext> or stdout.
    #[arg(short, long)]
    output: Option<String>,

    /// Compression algorithm to use: xz, bz2, gz.
    #[arg(short, long, default_value = "xz")]
    algo: String,

    /// Number of worker threads (defaults to CPU core count).
    #[arg(short = 'j', long)]
    threads: Option<usize>,

    /// Run integrity verification test on the input file.
    #[arg(short, long)]
    test: bool,

    /// Delete the source input file upon successful compression.
    #[arg(short, long)]
    delete: bool,

    /// Force writing output to stdout.
    #[arg(short = 'c', long)]
    stdout: bool,

    /// Verbose output / metrics.
    #[arg(short, long)]
    verbose: bool,
}


fn main() {
    if std::env::var("TEST_QUERY_SIZE").is_ok() {
        let (cols, rows) = crate::tui::query_initial_size();
        println!("SIZE:{}x{}", cols, rows);
        return;
    }

    let args = Args::parse();

    let compressor: Arc<Box<dyn Compressor + Send + Sync>> = Arc::new(match args.algo.as_str() {
        "gz" => Box::new(GzipCompressor),
        "bz2" => Box::new(Bzip2Compressor),
        "xz" => Box::new(XzCompressor),
        other => {
            eprintln!("Error: Unknown algorithm '{}'. Supported: xz, bz2, gz", other);
            std::process::exit(1);
        }
    });

    // Setup signal handler for Ctrl-C safety
    let cleanup_mutex = get_output_path_mutex().clone();
    if let Err(e) = ctrlc::set_handler(move || {
        eprintln!("\nReceived interrupt. Aborting and cleaning up...");
        let guard = cleanup_mutex.lock().unwrap();
        if let Some(ref path) = *guard {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
        std::process::exit(2);
    }) {
        eprintln!("Warning: Failed to set signal handler: {}", e);
    }

    let result = run_app(args, compressor);

    match result {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("Error: {}", e);
            // Delete output file on failure
            let guard = get_output_path_mutex().lock().unwrap();
            if let Some(ref path) = *guard {
                if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
            }
            match e {
                ZipError::Verification(_) => std::process::exit(3),
                _ => std::process::exit(2),
            }
        }
    }
}

fn run_compression(
    args: Args,
    compressor: Arc<Box<dyn Compressor + Send + Sync>>,
    tui_state: Option<Arc<Mutex<tui::TuiState>>>,
) -> Result<(), ZipError> {
    let is_stdin = args.input_file.is_none() || args.input_file.as_deref() == Some("-");
    let threads_count = args.threads.unwrap_or(0);

    let run_res = if is_stdin {
        log_verbose!("Running in Stream Mode (reading from standard input)...");
        let mut stdin = io::stdin();

        // Determine destination
        if args.stdout || args.output.is_none() {
            log_verbose!("Writing compressed stream to standard output");
            let mut stdout = io::stdout();
            stream_mode::compress_stream(&mut stdin, &mut stdout, compressor.as_ref().as_ref(), threads_count, tui_state.clone())
        } else {
            let out_path = PathBuf::from(args.output.as_ref().unwrap());
            log_verbose!("Writing compressed stream to output file: {:?}", out_path);
            // Register path for Ctrl-C cleanup
            {
                let mut guard = get_output_path_mutex().lock().unwrap();
                *guard = Some(out_path.clone());
            }

            let mut out_file = File::create(&out_path)?;
            stream_mode::compress_stream(&mut stdin, &mut out_file, compressor.as_ref().as_ref(), threads_count, tui_state.clone())
        }
    } else {
        // Split mode / File compression
        let input_path = PathBuf::from(args.input_file.as_ref().unwrap());
        log_verbose!("Running in File Compression Mode for: {:?}", input_path);
        if !input_path.exists() {
            return Err(ZipError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Input file not found: {:?}", input_path),
            )));
        }

        // Determine output path
        let out_path = if args.stdout {
            log_verbose!("Output directed to standard output (--stdout / -c flag)");
            None
        } else if let Some(ref out) = args.output {
            Some(PathBuf::from(out))
        } else {
            let ext = match args.algo.as_str() {
                "gz" => "gz",
                "bz2" => "bz2",
                _ => "xz",
            };
            let mut p = input_path.clone();
            let new_ext = match p.extension() {
                Some(existing) => format!("{}.{}", existing.to_string_lossy(), ext),
                None => ext.to_string(),
            };
            p.set_extension(new_ext);
            Some(p)
        };

        if let Some(ref path) = out_path {
            log_verbose!("Compression destination file: {:?}", path);
            // Register for cleanup
            {
                let mut guard = get_output_path_mutex().lock().unwrap();
                *guard = Some(path.clone());
            }

            let res = split_mode::compress_file(&input_path, path, compressor.as_ref().as_ref(), threads_count, tui_state.clone());

            if res.is_ok() && args.delete {
                log_verbose!("--delete option active. Removing source file: {:?}", input_path);
                std::fs::remove_file(&input_path)?;
            }
            res
        } else {
            // Writing to stdout in split mode (requires streaming the output chunks)
            let mut stdout = io::stdout();
            let input_data = std::fs::read(&input_path)?;
            let mut cursor = io::Cursor::new(input_data);
            stream_mode::compress_stream(&mut cursor, &mut stdout, compressor.as_ref().as_ref(), threads_count, tui_state.clone())
        }
    };

    if let Some(ref state) = tui_state {
        state.lock().unwrap().is_complete = true;
    }

    run_res
}

fn run_app(args: Args, compressor: Arc<Box<dyn Compressor + Send + Sync>>) -> Result<(), ZipError> {
    VERBOSE.store(args.verbose, Ordering::Relaxed);
    log_verbose!("Starting zipmt-rust utility...");
    log_verbose!("Selected compression algorithm: {}", args.algo);

    let is_stdin = args.input_file.is_none() || args.input_file.as_deref() == Some("-");
    let threads_count = args.threads.unwrap_or(0);

    if args.test {
        log_verbose!("Running in Verification/Integrity Test mode on input: {:?}", args.input_file);
        // Verification mode
        if is_stdin {
            return Err(ZipError::Verification("Cannot verify stream from standard input".into()));
        }
        let input_path = Path::new(args.input_file.as_ref().unwrap());
        if !input_path.exists() {
            return Err(ZipError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Input file not found: {:?}", input_path),
            )));
        }
        log_verbose!("Reading target compressed file: {:?}", input_path);
        let input_data = std::fs::read(input_path)?;
        log_verbose!("Decompressing and verifying stream integrity...");
        compressor.verify(&input_data)?;
        eprintln!("Verification succeeded for {:?}", input_path);
        return Ok(());
    }

    // Determine if TUI mode should run based on fallback checks and args.tui flag
    let force_tui = std::env::var("ZIPMT_FORCE_TUI").is_ok();
    let is_stdout_terminal = std::io::stdout().is_terminal();
    let is_stderr_terminal = std::io::stderr().is_terminal();

    let writing_to_stdout = args.stdout || (is_stdin && args.output.is_none());

    let run_tui = force_tui || (
        is_stderr_terminal
        && !(writing_to_stdout && is_stdout_terminal)
    );


    // Initialize TuiState if TUI mode is active
    let tui_state = if run_tui {
        TUI_ACTIVE.store(true, Ordering::Relaxed);
        let state = if is_stdin {
            Arc::new(Mutex::new(tui::TuiState::new_stream(
                if threads_count > 0 { threads_count * 2 } else { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) * 2 },
                0
            )))
        } else {
            let input_path = Path::new(args.input_file.as_ref().unwrap());
            let file_size = std::fs::metadata(input_path).map(|m| m.len() as usize).unwrap_or(0);
            let chunks_count = if threads_count > 0 { threads_count } else { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) };
            Arc::new(Mutex::new(tui::TuiState::new_split(chunks_count, file_size)))
        };
        Some(state)
    } else {
        None
    };

    if let Some(state) = tui_state {
        // Spawn compression thread
        let state_clone = state.clone();
        let args_clone = args.clone();
        let comp_handle = std::thread::spawn(move || {
            run_compression(args_clone, compressor, Some(state_clone))
        });

        // Run TUI loop on main thread
        tui::run_tui_on_main_thread(state, comp_handle)
    } else {
        run_compression(args, compressor, None)
    }
}
