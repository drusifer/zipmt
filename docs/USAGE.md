Build and usage guide for the Rust, Go, and C zipmt implementations.

TLDR:
    Goal: Build and safely operate zipmt for files, streams, automation, and interactive monitoring.
    Recommended: Use zipmt-rust; pass --no-tui in scripts and -T for the dashboard.
    Safety: Rust and Go preserve inputs by default; the legacy C version requires -k / --keep.

# zipmt user guide

## Contents

1. [Rust implementation](#1-rust-implementation-recommended)
2. [Rust command reference](#2-rust-command-reference)
3. [Rust TUI](#3-rust-tui)
4. [Go implementation](#4-go-implementation)
5. [C implementation](#5-c-implementation)
6. [Behavior and safety](#6-behavior-and-safety)

## 1. Rust implementation (recommended)

### Prerequisites

- A current stable Rust toolchain installed through `rustup`.
- Native build tools required by the compression crates on your platform.

### Build

```bash
cd zipmt-rust
cargo build --release
```

The resulting binary is `zipmt-rust/target/release/zipmt-rust` relative to the
repository root. The project-level equivalent is:

```bash
make build-rust
```

### File compression

File input uses the bounded Split pipeline.

```bash
# Default xz output: database.sql.xz
zipmt-rust/target/release/zipmt-rust database.sql

# Explicit output, algorithm, compression level, and workers
zipmt-rust/target/release/zipmt-rust \
  -a gz -l 6 -j 4 -o database.sql.gz database.sql
```

The input file is preserved by default. Add `--delete` only when the source
should be removed after a successful compression.

### Stream compression

Standard input uses the bounded Stream pipeline.

```bash
tar -cf - ./dataset \
  | zipmt-rust/target/release/zipmt-rust \
      --no-tui -a xz -j 4 -o dataset.tar.xz -
```

Use `-c` / `--stdout` to force compressed output to standard output:

```bash
cat database.sql \
  | zipmt-rust/target/release/zipmt-rust --no-tui -a gz -c - \
  > database.sql.gz
```

### Integrity verification

`--test` verifies an existing compressed file with the selected algorithm. It
does not support standard input.

```bash
zipmt-rust/target/release/zipmt-rust --test -a xz database.sql.xz
```

## 2. Rust command reference

```text
zipmt-rust [OPTIONS] [INPUT_FILE]
```

Use `-` or omit `INPUT_FILE` to read standard input.

| Option | Description |
|---|---|
| `-o, --output <FILE>` | Output path; defaults to the input name plus the selected format extension |
| `-a, --algo <xz\|bz2\|gz>` | Compression algorithm; default: `xz` |
| `-j, --threads <N>` | Worker count; default: available CPU parallelism |
| `-l, --level <1-9>` | Compression level; default: `9` |
| `-t, --test` | Verify an existing compressed input |
| `-d, --delete` | Delete a file input only after successful compression |
| `-c, --stdout` | Write compressed bytes to standard output |
| `-v, --verbose` | Print detailed progress when the TUI is inactive |
| `-T, --tui` | Request the interactive terminal dashboard |
| `--no-tui` | Disable the dashboard, including in forced-TUI environments |
| `-h, --help` | Show current CLI help |
| `-V, --version` | Show the version |

The TUI is opt-in and requires terminal-backed stderr. If stderr is redirected,
the command safely falls back to non-interactive output. `--no-tui` always wins.

## 3. Rust TUI

The dashboard requires at least 80 columns by 22 rows. File input displays
Split-mode slice progress; stdin displays Stream-mode worker and queue state.

### Split mode

![Split mode Rust TUI with slice status, mirrored I/O chart, logs, and controls](assets/rust-tui-split.svg)

### Stream mode

![Stream mode Rust TUI with worker cards, mirrored I/O chart, queues, logs, and controls](assets/rust-tui-stream.svg)

### Controls

| Key or input | Action |
|---|---|
| `P` | Pause or resume |
| `-` / `+` | Increase or decrease throttle delay |
| `I` | Switch between rate and cumulative I/O |
| `PgUp` / `PgDn` | Page through the visible slice or worker list |
| Mouse wheel over work panel | Page through slices or workers |
| Mouse wheel over logs | Scroll log history |
| `Home` / `End` | Jump to oldest or newest log messages |
| `Tab` | Move focus among controls available in the current mode |
| `↑` / `↓` | Adjust the focused control; scroll logs when no control is focused |
| `[` / `]` | Decrease or increase Stream compression level |
| Mouse click or drag | Adjust an available footer control |
| `Q` / `Esc` | Abort active compression or close completed results |
| `Enter` | Close completed results |

Split encoders load their compression level and worker pool at startup, so
those controls are labeled `FIXED`. Throttle and future chunk sizing remain
available. A completed interactive session freezes its final statistics until
closed.

The checked images are regenerated from the tested snapshot fixtures with:

```bash
make rust-tui-screenshots
```

## 4. Go implementation

The Go version requires Go 1.20 or newer.

```bash
cd zipmt-go
go build -o zipmt-go .
```

| Flag | Description |
|---|---|
| `-input <FILE>` | Input file; defaults to stdin |
| `-out <FILE>` | Output file; use `-` for stdout |
| `-algo <xz\|bz2\|gz>` | Compression format; default: `xz` |
| `-t` | Verify an existing compressed input |

```bash
# File to xz; preserves database.sql
./zipmt-go -input database.sql

# stdin to gzip on stdout
cat database.sql | ./zipmt-go -algo gz -out - > database.sql.gz
```

Repository validation targets:

```bash
make format-go
make test-go
```

## 5. C implementation

The legacy C version depends on GLib 2.0, libbz2, zlib, and OpenMP.

```bash
sudo apt-get install build-essential libglib2.0-dev libbz2-dev zlib1g-dev
cd src
make
```

```text
zipmt [OPTIONS] <file>
```

| Option | Description |
|---|---|
| `-t, --threads <N>` | Worker count; default: 4 |
| `-v, --verbose` | Show progress and statistics |
| `-o, --outfile <FILE>` | Output path |
| `-c, --stdout` | Write output to stdout |
| `-s, --stream` | Use stream mode |
| `-m, --omp` | Use OpenMP in supported stream paths |
| `-z, --zip` | Use gzip instead of bzip2 |
| `-k, --keep` | Preserve the input file |

Always use `-k` when the original file must be retained:

```bash
./zipmt -k database.sql
```

## 6. Behavior and safety

| Behavior | Rust | Go | C |
|---|---|---|---|
| Preserves input by default | Yes | Yes | No |
| Explicit source deletion | `--delete` after success | Not exposed | Avoid with `-k` |
| xz | Yes | Yes | No |
| bzip2 | Yes | Yes | Yes |
| gzip | Yes | Yes | Yes, mode-dependent |
| Interactive dashboard | Yes | No | No |
| File and stdin pipelines | Yes | Yes | Yes |

Rust removes incomplete output files on compression errors and Ctrl-C. For
automation, prefer `--no-tui`, explicit output paths, and integrity checks in
the surrounding workflow.
