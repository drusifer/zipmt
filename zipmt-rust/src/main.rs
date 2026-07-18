#![deny(unsafe_code)]

use clap::Parser;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use zipmt_rust::compressor::{Bzip2Compressor, Compressor, GzipCompressor, XzCompressor, ZipError};
use zipmt_rust::pipeline::{CompressionPipeline, InputSource, OutputDestination, ProgressEvent};
use zipmt_rust::{
    DEFAULT_COMPRESSION_LEVEL, TUI_ACTIVE, VERBOSE, get_output_path_mutex, log_verbose, tui,
};

#[derive(Parser, Clone, Debug)]
#[command(
    name = "zipmt-rust",
    version,
    about = "Parallel Multi-Format Compression Tool in Rust"
)]
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

    /// Compression level (1-9, defaults to 9).
    #[arg(short = 'l', long, default_value_t = DEFAULT_COMPRESSION_LEVEL)]
    level: u32,

    /// Run in interactive TUI mode.
    #[arg(short = 'T', long)]
    tui: bool,

    /// Disable interactive TUI mode.
    #[arg(long)]
    no_tui: bool,
}

fn should_run_tui(
    tui_requested: bool,
    no_tui_requested: bool,
    force_tui: bool,
    stderr_is_tty: bool,
) -> bool {
    if no_tui_requested {
        return false;
    }

    force_tui || (tui_requested && stderr_is_tty)
}

fn main() {
    if std::env::var("TEST_QUERY_SIZE").is_ok() {
        let (cols, rows) = crate::tui::query_initial_size();
        println!("SIZE:{}x{}", cols, rows);
        return;
    }

    let args = Args::parse();

    let compressor: Arc<Box<dyn Compressor + Send + Sync>> = Arc::new(match args.algo.as_str() {
        "gz" => Box::new(GzipCompressor { level: args.level }),
        "bz2" => Box::new(Bzip2Compressor { level: args.level }),
        "xz" => Box::new(XzCompressor { level: args.level }),
        other => {
            eprintln!(
                "Error: Unknown algorithm '{}'. Supported: xz, bz2, gz",
                other
            );
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

fn run_app(args: Args, compressor: Arc<Box<dyn Compressor + Send + Sync>>) -> Result<(), ZipError> {
    VERBOSE.store(args.verbose, Ordering::Relaxed);
    log_verbose!("Starting zipmt-rust utility...");
    log_verbose!("Selected compression algorithm: {}", args.algo);

    let is_stdin = args.input_file.is_none() || args.input_file.as_deref() == Some("-");
    let threads_count = args.threads.unwrap_or(0);

    if args.test {
        log_verbose!(
            "Running in Verification/Integrity Test mode on input: {:?}",
            args.input_file
        );
        // Verification mode
        if is_stdin {
            return Err(ZipError::Verification(
                "Cannot verify stream from standard input".into(),
            ));
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

    // The TUI owns stderr, so piped stdin and redirected compression output are safe.
    // Only the terminal used by Crossterm needs to be interactive.
    let stderr_is_tty = std::io::stderr().is_terminal();
    let force_tui = std::env::var("ZIPMT_FORCE_TUI").is_ok();
    let run_tui = should_run_tui(args.tui, args.no_tui, force_tui, stderr_is_tty);

    // Determine input source and output destination
    let input_source = if is_stdin {
        InputSource::Stdin
    } else {
        InputSource::File(PathBuf::from(args.input_file.as_ref().unwrap()))
    };

    let output_dest = if args.stdout {
        OutputDestination::Stdout
    } else if let Some(ref out) = args.output {
        OutputDestination::File(PathBuf::from(out))
    } else if is_stdin {
        OutputDestination::Stdout
    } else {
        let ext = match args.algo.as_str() {
            "gz" => "gz",
            "bz2" => "bz2",
            _ => "xz",
        };
        let mut p = PathBuf::from(args.input_file.as_ref().unwrap());
        let new_ext = match p.extension() {
            Some(existing) => format!("{}.{}", existing.to_string_lossy(), ext),
            None => ext.to_string(),
        };
        p.set_extension(new_ext);
        OutputDestination::File(p)
    };

    // Register path for Ctrl-C cleanup
    if let OutputDestination::File(ref out_path) = output_dest {
        let mut guard = get_output_path_mutex().lock().unwrap();
        *guard = Some(out_path.clone());
    }

    // Setup the pipeline
    let pipeline = CompressionPipeline::with_level(compressor, threads_count, args.level);
    let (controller, rx, comp_handle) = pipeline.run(input_source, output_dest);

    // Set initial compression level in the controller
    controller.update_level(args.level);

    let res = if run_tui {
        TUI_ACTIVE.store(true, Ordering::Relaxed);
        let state = if is_stdin {
            let pool_size = if threads_count > 0 {
                threads_count
            } else {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            };
            Arc::new(Mutex::new(tui::TuiState::new_stream(
                pool_size * 2,
                0,
                pool_size,
                args.level,
            )))
        } else {
            let input_path = Path::new(args.input_file.as_ref().unwrap());
            let file_size = std::fs::metadata(input_path)
                .map(|m| m.len() as usize)
                .unwrap_or(0);
            let chunks_count = if threads_count > 0 {
                threads_count
            } else {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            };
            Arc::new(Mutex::new(tui::TuiState::new_split(
                chunks_count,
                file_size,
                args.level,
            )))
        };

        // Run TUI loop on main thread
        tui::run_tui_on_main_thread(state, rx, controller, comp_handle)
    } else {
        // Run raw mode (no TUI). We consume rx and then join the thread
        while let Ok(event) = rx.recv() {
            match event {
                ProgressEvent::Error(err) => {
                    return Err(err);
                }
                _ => {}
            }
        }
        match comp_handle.join() {
            Ok(res) => res,
            Err(_) => Err(ZipError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Compression thread panicked",
            ))),
        }
    };

    if res.is_ok() && args.delete && !is_stdin {
        let input_path = Path::new(args.input_file.as_ref().unwrap());
        log_verbose!(
            "--delete option active. Removing source file: {:?}",
            input_path
        );
        std::fs::remove_file(input_path)?;
    }

    res
}

#[cfg(test)]
mod tests {
    use super::{Args, should_run_tui};
    use clap::{CommandFactory, Parser};

    #[test]
    fn streaming_tui_runs_when_stderr_is_a_terminal() {
        assert!(should_run_tui(true, false, false, true));
    }

    #[test]
    fn tui_falls_back_when_stderr_is_redirected() {
        assert!(!should_run_tui(true, false, false, false));
    }

    #[test]
    fn no_tui_wins_over_cli_and_environment_overrides() {
        assert!(!should_run_tui(true, true, true, true));
    }

    #[test]
    fn force_tui_supports_noninteractive_test_backends() {
        assert!(should_run_tui(false, false, true, false));
    }

    #[test]
    fn default_compression_level_is_maximum() {
        let args = Args::parse_from(["zipmt-rust", "-"]);
        assert_eq!(args.level, crate::DEFAULT_COMPRESSION_LEVEL);
        assert_eq!(args.level, 9);

        let help = Args::command().render_long_help().to_string();
        assert!(help.contains("defaults to 9"));
    }
}
