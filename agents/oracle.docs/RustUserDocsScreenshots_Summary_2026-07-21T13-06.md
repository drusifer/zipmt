# Rust User Documentation and Screenshots

Updated `README.md` and `docs/USAGE.md` to present the Rust implementation as
the recommended CLI. The guides now cover build steps, file and stdin
pipelines, verification, all CLI flags, TUI eligibility and controls, input and
partial-output safety, and behavioral differences across Rust, Go, and C.

Added accessible Split and Stream SVG screenshots under `docs/assets/`. They
are generated from the checked 80x22 snapshot fixtures by
`scripts/render_tui_screenshots.py` through `make rust-tui-screenshots`, keeping
documentation visuals tied to regression-tested UI output.
