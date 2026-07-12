Lessons learned catalog detailing dependency compilation, GLib concurrency deprecation, and safety warnings.

TLDR:
    Impact: Highlights critical setup steps, API warnings, and high-risk CLI behaviors.
    Next Steps: Periodically update as build issues or bug discoveries occur.

# Project Lessons Learned (LESSONS.md)

This document indexes critical lessons learned during the development, compilation, and maintenance of `zipmt`.

---

## 1. Lesson: Missing Development Headers for bzip2 Linkage
- **Date:** 2026-07-12
- > **Tags:** #Build #Dependencies #C
- **Context:** Compiling the C codebase on a clean environment fails immediately with `bzlib.h: No such file or directory`.
- **The Issue:** Developers assume that having `bzip2` installed in the OS is sufficient. However, the compiler requires the header files (`bzlib.h`) and static/shared library binaries which are only packaged in the development version of the library.
- **The Solution:** Install `libbz2-dev` (Debian/Ubuntu) or `bzip2-devel` (RHEL/CentOS) before compiling.
- **The Rule:** Always document and install development packages (`-dev` or `-devel`) for all compiled C library dependencies (GLib, bzip2, zlib).

---

## 2. Lesson: GLib Concurrency API Deprecation Warnings
- **Date:** 2026-07-12
- > **Tags:** #API #GLib #Concurrency
- **Context:** The code calls `g_thread_init(NULL)` during initialization.
- **The Issue:** In GLib version 2.32 and newer, the threading system is automatically initialized. The `g_thread_init` function is deprecated, causing compile-time warnings and potential compatibility issues with newer versions of GCC/GLib.
- **The Solution:** Remove the call to `g_thread_init` when compiling against GLib >= 2.32, or handle compilation flags dynamically.
- **The Rule:** Periodically audit legacy thread-initialization code when upgrading platform dependency versions to avoid compiler deprecation warnings.

---

## 3. Lesson: Deletion of Source Material by Default (UX Safety Hazard)
- **Date:** 2026-07-12
- > **Tags:** #Safety #UX #CLI
- **Context:** Test runs of `zipmt` on active project data resulted in the deletion of input files without confirmation.
- **The Issue:** The default mode of operation deletes the source file upon successful compression, which can lead to accidental data loss if the user forgets to supply `-k` or is unaware of this behavior.
- **The Solution:** Prominently place CAUTION and WARNING alerts in all user guides, READMEs, and help outputs.
- **The Rule:** Any utility that performs destructive operations (like deleting original files) must have highly visible warnings in the user interface and documentation.
