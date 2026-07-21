# Go ZipWriter Copy Fix Summary

`ZipWriter.Write` used `copy(dataSlice, zeroedChunk)`, which overwrote caller
data with zeros and queued zero-filled chunks. The call is now
`copy(chunk, dataSlice)`, giving asynchronous workers an owned copy while
leaving caller memory unchanged.

Added `TestZipWriterPreservesInputData`, which failed before the fix and now
proves both input immutability and exact gzip decompression across several
chunks. Added `make test-go` and `make format-go` for repeatable validation.

Validation:

- focused corruption regression: pass;
- all Go tests except legacy `TestCompressBZ2`: pass;
- full suite: blocked by the independent BZ2 test expecting 23 bytes but
  receiving 25 bytes.
