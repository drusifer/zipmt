# Current Task

**Status:** Go ZipWriter data-copy fix complete / handed to Trin
**Assigned to:** Neo

Corrected the reversed copy direction in `ZipWriter.Write`, added a regression
for caller-buffer immutability and gzip round-trip content, added repeatable Go
format/test Make targets, and removed the obsolete README safety warning.
Focused and non-BZ2 Go tests pass; the independent legacy BZ2 assertion remains.

---
*Last updated: 2026-07-21T13:01:00-04:00*
