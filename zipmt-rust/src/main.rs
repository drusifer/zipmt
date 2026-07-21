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

    let compressor = match resolve_compressor(&args.algo, args.level) {
        Ok(compressor) => compressor,
        Err(message) => {
            eprintln!("Error: {message}");
            std::process::exit(1);
        }
    };

    if let Err(error) = install_signal_handler() {
        eprintln!("Warning: Failed to set signal handler: {error}");
    }

    match run_app(args, compressor) {
        Ok(_) => std::process::exit(0),
        Err(error) => {
            eprintln!("Error: {error}");
            cleanup_registered_output();
            std::process::exit(exit_code_for_error(&error));
        }
    }
}

fn resolve_compressor(
    algorithm: &str,
    level: u32,
) -> Result<Arc<dyn Compressor + Send + Sync>, String> {
    match algorithm {
        "gz" => Ok(Arc::new(GzipCompressor { level })),
        "bz2" => Ok(Arc::new(Bzip2Compressor { level })),
        "xz" => Ok(Arc::new(XzCompressor { level })),
        other => Err(format!(
            "Unknown algorithm '{}'. Supported: xz, bz2, gz",
            other
        )),
    }
}

fn install_signal_handler() -> Result<(), ctrlc::Error> {
    let cleanup_mutex = get_output_path_mutex().clone();
    ctrlc::set_handler(move || {
        eprintln!("\nReceived interrupt. Aborting and cleaning up...");
        let guard = cleanup_mutex.lock().unwrap();
        if let Some(ref path) = *guard
            && path.exists()
        {
            let _ = std::fs::remove_file(path);
        }
        std::process::exit(2);
    })
}

fn cleanup_registered_output() {
    let guard = get_output_path_mutex().lock().unwrap();
    if let Some(ref path) = *guard
        && path.exists()
    {
        let _ = std::fs::remove_file(path);
    }
}

fn exit_code_for_error(error: &ZipError) -> i32 {
    match error {
        ZipError::Verification(_) => 3,
        _ => 2,
    }
}

struct RunPlan {
    input_source: InputSource,
    output_destination: OutputDestination,
    input_path: Option<PathBuf>,
    threads: usize,
    run_tui: bool,
}

struct OutputGuard {
    path: Option<PathBuf>,
    armed: bool,
}

impl OutputGuard {
    fn new(destination: &OutputDestination) -> Self {
        let path = match destination {
            OutputDestination::File(path) => Some(path.clone()),
            OutputDestination::Stdout => None,
        };
        *get_output_path_mutex().lock().unwrap() = path.clone();
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
        *get_output_path_mutex().lock().unwrap() = None;
    }
}

impl Drop for OutputGuard {
    fn drop(&mut self) {
        if self.armed
            && let Some(path) = &self.path
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn verify_input(args: &Args, compressor: &dyn Compressor) -> Result<(), ZipError> {
    let Some(input) = args.input_file.as_deref().filter(|input| *input != "-") else {
        return Err(ZipError::Verification(
            "Cannot verify stream from standard input".into(),
        ));
    };
    let input_path = Path::new(input);
    if !input_path.exists() {
        return Err(ZipError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Input file not found: {:?}", input_path),
        )));
    }
    compressor.verify(&std::fs::read(input_path)?)?;
    eprintln!("Verification succeeded for {:?}", input_path);
    Ok(())
}

fn default_output_path(input: &str, algorithm: &str) -> PathBuf {
    let extension = match algorithm {
        "gz" => "gz",
        "bz2" => "bz2",
        _ => "xz",
    };
    let mut path = PathBuf::from(input);
    let new_extension = path.extension().map_or_else(
        || extension.to_string(),
        |existing| format!("{}.{}", existing.to_string_lossy(), extension),
    );
    path.set_extension(new_extension);
    path
}

fn resolve_run_plan(args: &Args) -> RunPlan {
    let input_path = args
        .input_file
        .as_deref()
        .filter(|input| *input != "-")
        .map(PathBuf::from);
    let input_source = input_path
        .clone()
        .map_or(InputSource::Stdin, InputSource::File);
    let output_destination = if args.stdout {
        OutputDestination::Stdout
    } else if let Some(output) = &args.output {
        OutputDestination::File(PathBuf::from(output))
    } else if let Some(input) = args.input_file.as_deref().filter(|input| *input != "-") {
        OutputDestination::File(default_output_path(input, &args.algo))
    } else {
        OutputDestination::Stdout
    };
    let run_tui = should_run_tui(
        args.tui,
        args.no_tui,
        std::env::var("ZIPMT_FORCE_TUI").is_ok(),
        std::io::stderr().is_terminal(),
    );
    RunPlan {
        input_source,
        output_destination,
        input_path,
        threads: args.threads.unwrap_or(0),
        run_tui,
    }
}

fn resolved_workers(threads: usize) -> usize {
    if threads > 0 {
        threads
    } else {
        std::thread::available_parallelism().map_or(4, |count| count.get())
    }
}

fn initial_tui_state(plan: &RunPlan, level: u32) -> Arc<Mutex<tui::TuiState>> {
    let workers = resolved_workers(plan.threads);
    if let Some(input_path) = &plan.input_path {
        let file_size = std::fs::metadata(input_path).map_or(0, |metadata| metadata.len() as usize);
        Arc::new(Mutex::new(tui::TuiState::new_split(
            workers, file_size, level,
        )))
    } else {
        Arc::new(Mutex::new(tui::TuiState::new_stream(
            workers * 2,
            0,
            workers,
            level,
        )))
    }
}

fn run_pipeline(
    plan: &RunPlan,
    level: u32,
    compressor: Arc<dyn Compressor + Send + Sync>,
) -> Result<(), ZipError> {
    let pipeline = CompressionPipeline::with_level(compressor, plan.threads, level);
    let (controller, receiver, handle) = pipeline.run(
        match &plan.input_source {
            InputSource::File(path) => InputSource::File(path.clone()),
            InputSource::Stdin => InputSource::Stdin,
        },
        match &plan.output_destination {
            OutputDestination::File(path) => OutputDestination::File(path.clone()),
            OutputDestination::Stdout => OutputDestination::Stdout,
        },
    );
    if plan.run_tui {
        TUI_ACTIVE.store(true, Ordering::Relaxed);
        return tui::run_tui_on_main_thread(
            initial_tui_state(plan, level),
            receiver,
            controller,
            handle,
        );
    }
    while let Ok(event) = receiver.recv() {
        if let ProgressEvent::Error(error) = event {
            return Err(error);
        }
    }
    handle.join().unwrap_or_else(|_| {
        Err(ZipError::Io(std::io::Error::other(
            "Compression thread panicked",
        )))
    })
}

fn run_app(args: Args, compressor: Arc<dyn Compressor + Send + Sync>) -> Result<(), ZipError> {
    VERBOSE.store(args.verbose, Ordering::Relaxed);
    log_verbose!("Starting zipmt-rust utility...");
    log_verbose!("Selected compression algorithm: {}", args.algo);

    if args.test {
        return verify_input(&args, compressor.as_ref());
    }

    let plan = resolve_run_plan(&args);
    let mut output_guard = OutputGuard::new(&plan.output_destination);
    run_pipeline(&plan, args.level, compressor)?;
    output_guard.disarm();
    if args.delete
        && let Some(input_path) = plan.input_path
    {
        std::fs::remove_file(input_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Args, exit_code_for_error, resolve_compressor, should_run_tui};
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

    #[test]
    fn compressor_resolution_is_typed_and_preserves_diagnostic() {
        assert!(resolve_compressor("xz", 5).is_ok());
        assert!(resolve_compressor("gz", 5).is_ok());
        assert!(resolve_compressor("bz2", 5).is_ok());
        assert_eq!(
            resolve_compressor("zip", 5).err().as_deref(),
            Some("Unknown algorithm 'zip'. Supported: xz, bz2, gz")
        );
    }

    #[test]
    fn verification_errors_have_the_distinct_exit_code() {
        assert_eq!(
            exit_code_for_error(&zipmt_rust::compressor::ZipError::Verification(
                "failed".into()
            )),
            3
        );
        assert_eq!(
            exit_code_for_error(&zipmt_rust::compressor::ZipError::Io(
                std::io::Error::other("failed")
            )),
            2
        );
    }
}
