Multi-threaded compression utility for bzip2 and gzip written in C.

TLDR:
    Goal: Provide parallel bzip2/gzip compression on multi-core systems.
    Status: Complete and documented, utilizing GLib and OpenMP.
    Action: Use -k/--keep to prevent default source file deletion.

# zipmt - Multi-threaded Compression Utility

`zipmt` is a high-performance command-line compression utility written in C. It supports multi-threaded parallel compression using either **bzip2** or **gzip** algorithms, utilizing GLib and OpenMP to accelerate workloads on multi-core systems.

> [!CAUTION]
> **Data Deletion warning:** By default, `zipmt` will delete the input source file upon successful compression. To prevent this, always run with the `-k` / `--keep` flag, or write to standard output using `-c`.

---

## 📖 Documentation Index

To help you get started and understand the codebase, the following documentation is available:

- **[User Guide & Compilation](file:///home/drusifer/Projects/zipmt/docs/USAGE.md)**: Setup, build dependencies, command-line arguments, and usage examples.
- **[Architecture Overview](file:///home/drusifer/Projects/zipmt/docs/ARCH.md)**: System design, thread modes (Split vs. Stream), data structures, pipeline flow, and technical debt.
- **[Repository Mindmap](file:///home/drusifer/Projects/zipmt/MINDMAP.md)**: Code structure, module indexing, and file responsibilities.
- **[BobProtocol Agent Directory](file:///home/drusifer/Projects/zipmt/agents/DOCUMENTATION_INDEX.md)**: Index of AI personas, skills, templates, and team state files.

---

## ⚡ Quick Start

### 1. Build from Source
Ensure dependencies (`libglib2.0-dev`, `libbz2-dev`, `zlib1g-dev`) are installed, then compile the utility:
```bash
cd src
make
```

### 2. Compress a File (Keeping the Original)
```bash
./zipmt -k my_large_file.db
```

### 3. Compress a File using 8 Threads (Using Gzip)
```bash
./zipmt -t 8 -z -k my_large_file.db
```

### 4. Compress a Stream from stdin to stdout
```bash
tar -cf - ./my_folder | ./zipmt -s -c - > my_folder.tar.bz2
```

---

## 🛠️ Project Structure

```
├── README.md               # Project entry point and overview
├── MINDMAP.md              # Code architecture and file catalog
├── Makefile                # Top-level makefile for tooling and tests
├── docs/                   # Global documentation
│   ├── ARCH.md             # System architecture and thread models
│   └── USAGE.md            # Installation and usage guide
├── src/                    # C Source files
│   ├── Makefile            # C build configuration
│   └── zipmt.c             # Main source code
└── agents/                 # BobProtocol agent ecosystem
    ├── CHAT.md             # Agent team communication log
    └── [persona].docs/     # Persona state and instruction folders
```
