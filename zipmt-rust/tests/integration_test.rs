use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn get_bin_path() -> std::path::PathBuf {
    // Find binary relative to current test environment
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("zipmt-rust");
    path
}

#[test]
fn test_integration_split_mode_gzip() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.gz");

    let original_data =
        b"This is some repetitive data for integration testing split mode. ".repeat(5000);
    std::fs::write(&input_path, &original_data).unwrap();

    // Run: zipmt-rust input.txt -o output.gz -a gz
    let output = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .arg("-j")
        .arg("2")
        .output()
        .expect("Failed to execute zipmt-rust");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify decompression
    let compressed_data = std::fs::read(&output_path).unwrap();
    let mut decoder = flate2::read::MultiGzDecoder::new(&compressed_data[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();

    assert_eq!(
        original_data.to_vec(),
        decompressed,
        "Decompressed content mismatch"
    );

    // Verify source input still exists (preservation default)
    assert!(input_path.exists(), "Source file was deleted by default");
}

#[test]
fn test_integration_stream_mode_bzip2() {
    let _dir = tempdir().unwrap();
    let bin = get_bin_path();

    let original_data =
        b"Streaming some data through stdin and stdout using bzip2 parallel. ".repeat(2000);

    // Run: cat input | zipmt-rust - -a bz2
    let mut child = Command::new(&bin)
        .arg("-")
        .arg("-a")
        .arg("bz2")
        .arg("-j")
        .arg("2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn zipmt-rust");

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&original_data).unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Stream command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify decompression
    let mut decoder = bzip2::read::BzDecoder::new(&output.stdout[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();

    assert_eq!(
        original_data.to_vec(),
        decompressed,
        "Stream decompression mismatch"
    );
}

#[test]
fn test_integration_file_to_stdout_streaming_gzip() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();
    let input_path = dir.path().join("input.txt");
    let original_data =
        b"Streaming a file directly to stdout without buffering the whole input. ".repeat(5000);
    std::fs::write(&input_path, &original_data).unwrap();

    let output = Command::new(&bin)
        .arg(&input_path)
        .arg("--stdout")
        .arg("-a")
        .arg("gz")
        .arg("-j")
        .arg("2")
        .output()
        .expect("Failed to execute file-to-stdout compression");

    assert!(
        output.status.success(),
        "File-to-stdout command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut decoder = flate2::read::MultiGzDecoder::new(&output.stdout[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    assert_eq!(original_data, decompressed);
}

#[test]
fn test_integration_stdin_to_file_streaming_xz() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();
    let output_path = dir.path().join("output.xz");
    let original_data =
        b"Streaming standard input to a file must preserve exact output order. ".repeat(5000);

    let mut child = Command::new(&bin)
        .arg("-")
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("xz")
        .arg("-j")
        .arg("2")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn stdin-to-file compression");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(&original_data)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Stdin-to-file command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let compressed = std::fs::File::open(&output_path).unwrap();
    let mut decoder = xz2::read::XzDecoder::new(compressed);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    assert_eq!(original_data, decompressed);
}

#[cfg(target_os = "linux")]
fn process_rss_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(rss_kib * 1024)
}

#[test]
#[cfg(target_os = "linux")]
fn test_integration_file_to_stdout_rss_is_bounded() {
    const INPUT_BYTES: u64 = 128 * 1024 * 1024;
    const MAX_RSS_BYTES: u64 = 96 * 1024 * 1024;

    let dir = tempdir().unwrap();
    let bin = get_bin_path();
    let input_path = dir.path().join("large-zero-input.bin");
    std::fs::File::create(&input_path)
        .unwrap()
        .set_len(INPUT_BYTES)
        .unwrap();

    let mut child = Command::new(&bin)
        .arg(&input_path)
        .arg("--stdout")
        .arg("-a")
        .arg("gz")
        .arg("-l")
        .arg("1")
        .arg("-j")
        .arg("2")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn RSS characterization");

    let mut compressed_stdout = child.stdout.take().unwrap();
    let drain = thread::spawn(move || {
        let mut compressed = Vec::new();
        compressed_stdout.read_to_end(&mut compressed).unwrap();
        compressed
    });

    let mut peak_rss = 0;
    while child.try_wait().unwrap().is_none() {
        if let Some(rss) = process_rss_bytes(child.id()) {
            peak_rss = peak_rss.max(rss);
        }
        thread::sleep(Duration::from_millis(5));
    }

    let output = child.wait_with_output().unwrap();
    let compressed = drain.join().unwrap();
    assert!(
        output.status.success(),
        "RSS command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(peak_rss > 0, "Failed to sample child RSS");
    assert!(
        peak_rss < MAX_RSS_BYTES,
        "File-to-stdout peak RSS {peak_rss} was not bounded below {MAX_RSS_BYTES}"
    );

    let mut decoder = flate2::read::MultiGzDecoder::new(&compressed[..]);
    let mut decoded_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = decoder.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        assert!(buffer[..read].iter().all(|byte| *byte == 0));
        decoded_bytes += read as u64;
    }
    assert_eq!(INPUT_BYTES, decoded_bytes);
}

#[test]
#[cfg(unix)]
fn test_integration_interrupt_removes_incomplete_output() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();
    let output_path = dir.path().join("interrupt-output.xz");

    let mut child = Command::new(&bin)
        .arg("-")
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("xz")
        .arg("-l")
        .arg("9")
        .arg("-j")
        .arg("1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn interrupt characterization");

    let mut child_stdin = child.stdin.take().unwrap();
    let feed = thread::spawn(move || {
        let buffer = vec![0x5a; 1024 * 1024];
        while child_stdin.write_all(&buffer).is_ok() {}
    });

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if output_path
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
        {
            break;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "Compression completed before interrupt characterization"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        output_path.exists(),
        "Output was never created before interrupt"
    );

    let signal = Command::new("kill")
        .arg("-INT")
        .arg(child.id().to_string())
        .status()
        .expect("Failed to send SIGINT");
    assert!(signal.success(), "kill -INT failed");

    let output = child.wait_with_output().unwrap();
    feed.join().unwrap();
    assert_eq!(Some(2), output.status.code());
    assert!(
        !output_path.exists(),
        "Incomplete output remained after interrupt"
    );
}

#[test]
fn test_integration_verification_and_corruption() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.xz");

    let original_data = b"Testing verification mode loops on xz format. ".repeat(1000);
    std::fs::write(&input_path, &original_data).unwrap();

    // Compress
    let output = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("xz")
        .output()
        .unwrap();
    assert!(output.status.success());

    // Test verification: zipmt-rust output.xz -t -a xz
    let verify_output = Command::new(&bin)
        .arg(&output_path)
        .arg("-t")
        .arg("-a")
        .arg("xz")
        .output()
        .unwrap();
    assert!(
        verify_output.status.success(),
        "Verification failed on valid xz: {}",
        String::from_utf8_lossy(&verify_output.stderr)
    );

    // Corrupt the output file
    let mut compressed_data = std::fs::read(&output_path).unwrap();
    if compressed_data.len() > 50 {
        // overwrite some middle bytes
        for byte in compressed_data.iter_mut().take(30).skip(20) {
            *byte ^= 0xFF;
        }
    }
    std::fs::write(&output_path, &compressed_data).unwrap();

    // Verify again, should fail with exit code 3
    let verify_corrupt = Command::new(&bin)
        .arg(&output_path)
        .arg("-t")
        .arg("-a")
        .arg("xz")
        .output()
        .unwrap();

    assert_eq!(
        verify_corrupt.status.code(),
        Some(3),
        "Verification succeeded or returned wrong code on corrupted file"
    );
}

#[test]
fn test_integration_delete_source() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.gz");

    let original_data = b"Temporary data that should be deleted after compression cycle.";
    std::fs::write(&input_path, original_data).unwrap();

    // Run with --delete / -d
    let output = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .arg("-d")
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify output exists and input is deleted
    assert!(output_path.exists(), "Output file was not created");
    assert!(
        !input_path.exists(),
        "Source input file was not deleted despite --delete flag"
    );
}

#[test]
fn test_integration_tui_mode() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.gz");

    let original_data = b"Some repetitive data to trigger TUI redraw loops. ".repeat(100);
    std::fs::write(&input_path, &original_data).unwrap();

    // 1. Run by setting ZIPMT_FORCE_TUI env variable to bypass terminal/redirection checks (force TUI)
    let output_force = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .env("ZIPMT_FORCE_TUI", "1")
        .output()
        .unwrap();

    assert!(output_force.status.success(), "Forced TUI command failed");
    let stderr_force = String::from_utf8_lossy(&output_force.stderr);
    assert!(
        stderr_force.contains("ZIPMT PIPELINE CONTROLLER"),
        "TUI header not found in stderr under forced environment: {}",
        stderr_force
    );

    // 2. Run with -T arg, but no ZIPMT_FORCE_TUI. Because stderr is redirected,
    // Crossterm has no interactive terminal and should fall back to non-TUI.
    let output_opt_in = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .arg("-T")
        .env_remove("ZIPMT_FORCE_TUI")
        .output()
        .unwrap();

    assert!(output_opt_in.status.success(), "Opt-in TUI command failed");
    let stderr_opt_in = String::from_utf8_lossy(&output_opt_in.stderr);
    assert!(
        !stderr_opt_in.contains("ZIPMT PIPELINE CONTROLLER"),
        "TUI header should not be present (should fallback to non-TUI due to redirected stream)"
    );

    // 3. Run without -T and no ZIPMT_FORCE_TUI. Should default to non-TUI.
    let output_default = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .env_remove("ZIPMT_FORCE_TUI")
        .output()
        .unwrap();

    assert!(output_default.status.success(), "Default command failed");
    let stderr_default = String::from_utf8_lossy(&output_default.stderr);
    assert!(
        !stderr_default.contains("ZIPMT PIPELINE CONTROLLER"),
        "TUI header should not be present in default mode"
    );
}

#[test]
fn test_integration_tui_size_env_fallback() {
    let bin = get_bin_path();
    let output = Command::new(&bin)
        .env("TEST_QUERY_SIZE", "1")
        .env("COLUMNS", "137")
        .env("LINES", "42")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .unwrap();

    let out_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        out_str.trim().contains("SIZE:137x42"),
        "Expected to detect size 137x42, got: {}",
        out_str
    );
}

#[test]
fn test_integration_tui_size_tty_fallback() {
    let bin = get_bin_path();
    let mut cmd = Command::new(&bin);
    cmd.env("TEST_QUERY_SIZE", "1")
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .stdout(Stdio::piped());

    // If /dev/tty is available, we expect the output to match stty size
    if let Ok(file) = std::fs::File::open("/dev/tty")
        && let Ok(stty_output) = Command::new("stty").arg("size").stdin(file).output()
    {
        let out_str = String::from_utf8_lossy(&stty_output.stdout);
        let parts: Vec<&str> = out_str.split_whitespace().collect();
        if parts.len() == 2
            && let (Ok(expected_rows), Ok(expected_cols)) =
                (parts[0].parse::<u16>(), parts[1].parse::<u16>())
        {
            let output = cmd.output().unwrap();
            let out_str = String::from_utf8_lossy(&output.stdout);
            let expected_str = format!("SIZE:{}x{}", expected_cols, expected_rows);
            assert!(
                out_str.trim().contains(&expected_str),
                "Expected to detect size {}, got: {}",
                expected_str,
                out_str
            );
        }
    }
}
