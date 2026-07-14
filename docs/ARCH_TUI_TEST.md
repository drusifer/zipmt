# TUI Snapshot Testing Architecture

This document describes the design for decoupling layout rendering from stdout/stderr targets and integrating snapshot assertions into `zipmt-rust`.

## Refactored Rendering Signature

We decouple `draw_tui` by abstracting the output target:

```rust
// In src/tui.rs:
fn draw_tui(state: &TuiState, target: &mut dyn std::io::Write) {
    // Replacement of eprint! and eprintln! with write! and writeln! on target
    writeln!(target, "=== [zipmt-rust] Concurrency Progress ===").unwrap();
    // ...
}
```

- **Production execution**:
  ```rust
  draw_tui(&guard, &mut std::io::stderr());
  ```
- **Test execution**:
  ```rust
  let mut buffer = Vec::new();
  draw_tui(&mock_state, &mut buffer);
  ```

## Snapshot Assertions with Insta

1. **Stripping ANSI Escape Codes**: To prevent raw cursor move (`\x1B[H`) and screen clear (`\x1B[J`) codes from cluttering our snapshot files, tests will run the output through a regex filter to strip ANSI codes before snapshots:
   ```rust
   fn strip_ansi(input: &str) -> String {
       let re = regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap();
       re.replace_all(input, "").into_owned()
   }
   ```
2. **Snapshot Testing**: `insta::assert_snapshot!` compares the resulting plain-text grid against the saved visual layout, asserting correct borders, bars, and math.
