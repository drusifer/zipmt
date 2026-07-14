use std::io::{Read, Write};
use std::process::{Command, Stdio};
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

    let original_data = b"This is some repetitive data for integration testing split mode. ".repeat(5000);
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

    assert!(output.status.success(), "Command failed: {}", String::from_utf8_lossy(&output.stderr));

    // Verify decompression
    let compressed_data = std::fs::read(&output_path).unwrap();
    let mut decoder = flate2::read::MultiGzDecoder::new(&compressed_data[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();

    assert_eq!(original_data.to_vec(), decompressed, "Decompressed content mismatch");

    // Verify source input still exists (preservation default)
    assert!(input_path.exists(), "Source file was deleted by default");
}

#[test]
fn test_integration_stream_mode_bzip2() {
    let _dir = tempdir().unwrap();
    let bin = get_bin_path();

    let original_data = b"Streaming some data through stdin and stdout using bzip2 parallel. ".repeat(2000);

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
    assert!(output.status.success(), "Stream command failed: {}", String::from_utf8_lossy(&output.stderr));

    // Verify decompression
    let mut decoder = bzip2::read::BzDecoder::new(&output.stdout[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();

    assert_eq!(original_data.to_vec(), decompressed, "Stream decompression mismatch");
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
    assert!(verify_output.status.success(), "Verification failed on valid xz: {}", String::from_utf8_lossy(&verify_output.stderr));

    // Corrupt the output file
    let mut compressed_data = std::fs::read(&output_path).unwrap();
    if compressed_data.len() > 50 {
        // overwrite some middle bytes
        for i in 20..30 {
            compressed_data[i] ^= 0xFF;
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
    
    assert_eq!(verify_corrupt.status.code(), Some(3), "Verification succeeded or returned wrong code on corrupted file");
}

#[test]
fn test_integration_delete_source() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.gz");

    let original_data = b"Temporary data that should be deleted after compression cycle.";
    std::fs::write(&input_path, &original_data).unwrap();

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
    assert!(!input_path.exists(), "Source input file was not deleted despite --delete flag");
}

#[test]
fn test_integration_tui_mode() {
    let dir = tempdir().unwrap();
    let bin = get_bin_path();

    let input_path = dir.path().join("input.txt");
    let output_path = dir.path().join("output.gz");

    let original_data = b"Some repetitive data to trigger TUI redraw loops. ".repeat(100);
    std::fs::write(&input_path, &original_data).unwrap();

    // Run with --tui / -T
    let output = Command::new(&bin)
        .arg(&input_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-a")
        .arg("gz")
        .arg("-T")
        .output()
        .unwrap();

    assert!(output.status.success(), "TUI command failed");
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr_str.contains("=== [zipmt-rust] Concurrency Progress"),
        "TUI header not found in stderr: {}",
        stderr_str
    );
}
