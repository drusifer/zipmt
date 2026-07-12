Compilation instructions and user manual for zipmt CLI compression utility.

TLDR:
    Problem: Standard setup instructions are missing for building and using zipmt.
    Solution: Added detailed dependency lists, build commands, CLI flag references, and examples.
    Breaking Changes: No, but warning on default file deletion remains critical.

# zipmt User Guide & Compilation Reference

This document provides instructions for compiling, installing, and using the `zipmt` multi-threaded compression utility.

## 1. Compilation & Installation

`zipmt` depends on GLib 2.0, libbz2, and zlib libraries, and uses OpenMP for loop-level parallelization.

### Prerequisites (Ubuntu/Debian)
Ensure you have the required build tools and development libraries installed:
```bash
sudo apt-get update
sudo apt-get install build-essential libglib2.0-dev libbz2-dev zlib1g-dev
```

### Building `zipmt`
1. Navigate to the `src` directory:
   ```bash
   cd src
   ```
2. Build the binary using `make`:
   ```bash
   make
   ```
   This compiles `zipmt.c` with optimization flags (`-O3`), OpenMP support (`-fopenmp`), and produces the `zipmt` executable.

### Installing and Uninstalling
To install the compiled binary globally to `/usr/bin/`:
```bash
sudo make install
```
To remove the installed binary:
```bash
sudo make uninstall
```
To clean build artifacts:
```bash
make clean
```

---

## 2. Command Line Synopsis

```
zipmt [OPTIONS] <file>
```
* `<file>`: The name of the file to compress. Use `"-"` to indicate standard input.

### Command Line Options

| Short Option | Long Option | Description |
|--------------|-------------|-------------|
| `-t <int>` | `--threads=<int>` | Number of threads to use. (Default: 4). |
| `-v` | `--verbose` | Show progress and execution statistics. |
| `-o <file>` | `--outfile=<file>` | Name of the output file. (Defaults to `<file>.[bz2\|gz]`). |
| `-c` | `--stdout` | Write compressed output to standard output (redirect with `>`). |
| `-s` | `--stream` | Compress using the stream-based method. |
| `-m` | `--omp` | Compress using OpenMP instead of GLib Thread Pools (only valid in stream mode). |
| `-z` | `--zip` | Compress using gzip algorithm instead of default bzip2. |
| `-k` | `--keep` | Do not delete the input file when compression succeeds. |

---

## 3. Usage Examples

### Compress a File (Default bzip2, Split Mode)
Compresses `database.sql` into `database.sql.bz2` using 4 threads. 
> [!WARNING]
> By default, `zipmt` will delete the input file (`database.sql`) after successful compression!
```bash
zipmt database.sql
```

### Compress and Keep the Original File
To preserve the original input file after compression, use the `-k` or `--keep` flag:
```bash
zipmt -k database.sql
```

### Compress with a Specific Number of Threads
To utilize more CPU cores, specify the number of threads:
```bash
zipmt -t 8 -k database.sql
```

### Compress using Gzip (Split Mode Only)
Compresses `server.log` to `server.log.gz` using gzip algorithm:
```bash
zipmt -z -k server.log
```

### Streaming from Standard Input
To compress standard input, you must specify `"-"` as the file and use `-s` / `--stream` mode. You must also redirect output (`-c` or `-o`):
```bash
cat large_dataset.csv | zipmt -s -o large_dataset.csv.bz2 -
# OR using stdout redirect:
tar -cf - ./project | zipmt -s -c - > project.tar.bz2
```

### Stream Compression using OpenMP
To use OpenMP parallelization instead of GLib Thread Pools in stream mode:
```bash
zipmt -s -m -k large_file.bin
```

---

## 4. Behavioral Notes & Safety Warnings

> [!CAUTION]
> **Data Deletion Hazard:**
> Unlike standard command line utilities like `gzip` or `bzip2` which can be configured to keep or delete, `zipmt` **deletes the input file by default**. ALWAYS use the `-k` / `--keep` option if you want to preserve your source data, or use stdout `-c` to pipe results.

### Incompatible Option Combinations
- **Gzip in Stream Mode:** You cannot use gzip compression (`-z`) in stream mode (`-s`). It will exit with:
  `Error: you cannot use gzip compression in stream mode`
- **Output Clashes:** You cannot combine `-c` (stdout) and `-o` (explicit outfile). It will exit with:
  `You can not specify -c and -o together`
- **Stdin stream requirement:** Reading from stdin `"-"` automatically forces stream mode (`-s` is set to `TRUE`). You must specify `-o` or `-c` when reading from stdin.
