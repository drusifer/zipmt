# Worker Ratio and I/O Divider

Worker progress now marks encoder output as intermediate or final. The panel
shows `--.--x` while output is buffered and a bounded fixed-width two-decimal
ratio after finalization. The native chart zero guide is now a faint continuous
Braille divider.

All 41 unit tests, 7 integration tests, documentation tests, Stream/Split
snapshots, and debug build pass.
