Compilation instructions and user manual for zipmt C and Go CLI compression utilities.

TLDR:
    Problem: Missing setup and usage guidelines for the Go implementation.
    Solution: Added detailed build guides, CLI flag references, and usage examples for both C and Go versions.
    Breaking Changes: No. Go version does not delete files by default (unlike C version).

# zipmt User Guide & Compilation Reference

This document provides instructions for compiling, installing, and using both the C and Go implementations of the `zipmt` multi-threaded compression utility.

---

## 1. C Implementation: Build & Run

The C version depends on GLib 2.0, libbz2, and zlib libraries, and utilizes OpenMP.

### Prerequisites (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install build-essential libglib2.0-dev libbz2-dev zlib1g-dev
```

### Building `zipmt`
1. Navigate to the C source directory:
   ```bash
   cd src
   ```
2. Build the binary:
   ```bash
   make
   ```
   This compiles `zipmt.c` with optimization flags (`-O3`), OpenMP support (`-fopenmp`), and produces the `zipmt` executable.

### Installation
```bash
sudo make install    # Installs to /usr/bin/zipmt
sudo make uninstall  # Removes from /usr/bin/zipmt
make clean           # Cleans build artifacts
```

---

## 2. Go Implementation: Build & Run

The Go version requires Go 1.20 or newer.

### Prerequisites
Make sure Go is installed on your system. To verify:
```bash
go version
```

### Building `zipmt-go`
1. Navigate to the Go directory:
   ```bash
   cd zipmt-go
   ```
2. Download dependencies and compile the binary:
   ```bash
   go build -o zipmt-go main.go
   ```
   This produces the executable `zipmt-go` (or `zipmt-go.exe` on Windows).

---

## 3. Command Line CLI Reference

Both implementations provide different command-line arguments and flags:

### C Version Arguments
```
zipmt [OPTIONS] <file>
```
* `<file>`: The name of the file to compress. Use `"-"` to indicate standard input.

| Option | Long Option | Description |
|--------|-------------|-------------|
| `-t <int>` | `--threads=<int>` | Number of threads to use. (Default: 4). |
| `-v` | `--verbose` | Show progress and execution statistics. |
| `-o <file>` | `--outfile=<file>` | Name of the output file. (Defaults to `<file>.[bz2\|gz]`). |
| `-c` | `--stdout` | Write compressed output to standard output. |
| `-s` | `--stream` | Compress using the stream-based method. |
| `-m` | `--omp` | Compress using OpenMP instead of GLib (stream mode only). |
| `-z` | `--zip` | Compress using gzip algorithm instead of default bzip2. |
| `-k` | `--keep` | Do not delete the input file when compression succeeds. |

### Go Version Flags
```
zipmt-go [FLAGS]
```

| Flag | Value Type | Description |
|------|------------|-------------|
| `-input` | `string` | The input file name to compress. (Default: Stdin). |
| `-out` | `string` | The output file name to write to. Use `"-"` for stdout. (Default: `<input>.<algo>`). |
| `-algo` | `string` | Compression format. Must be one of `[xz, bz2, gz]`. (Default: `xz`). |
| `-t` | `boolean` | Run verification test mode on the input file. |

---

## 4. Usage Examples

### Compress a File (C Version)
Compresses `database.sql` into `database.sql.bz2` using 4 threads. 
> [!WARNING]
> By default, the C version will delete the input file (`database.sql`) after successful compression!
```bash
# Deletes database.sql on success
zipmt database.sql

# Keeps database.sql
zipmt -k database.sql
```

### Compress a File (Go Version)
Compresses `database.sql` into `database.sql.xz` using Go channel pipelines.
> [!NOTE]
> The Go implementation **never** deletes the input file by default.
```bash
# Generates database.sql.xz; preserves database.sql
./zipmt-go -input database.sql

# Compress using bzip2 format
./zipmt-go -algo bz2 -input database.sql
```

### Pipe Compression (Go Version)
Read data from stdin and write compressed gzip output to stdout:
```bash
cat large_data.csv | ./zipmt-go -algo gz -out - > compressed.gz
```

### Test File Validity (Go Version)
Runs a decompression verify pass to check if the file is valid:
```bash
./zipmt-go -t -algo xz -input compressed_file.xz
```

---

## 5. Behavioral Differences & Safety Caveats

1. **File Preservation:**
   - **C Version:** Automatically deletes the original source file unless `-k` / `--keep` is specified.
   - **Go Version:** Always preserves the original source file.
2. **Default Algorithms:**
   - **C Version:** Defaults to `bzip2`.
   - **Go Version:** Defaults to `xz`.
3. **Format Support Matrix:**
   - **C Version:** Supports `bzip2` (Split & Stream modes) and `gzip` (Split mode only). Does not support `xz`.
   - **Go Version:** Supports `xz`, `bz2`, and `gzip` (all in memory pipeline streams).
