# User Stories & Acceptance Criteria: TUI Snapshot Testing

## Epic: TUI layout verification

### Story 1: TUI snapshot assertions
- **As a** developer maintaining `zipmt-rust`
- **I want to** run unit tests that verify the exact layout structure of Split and Stream modes
- **So that** I can detect accidental visual formatting regression or alignment bugs.

#### Acceptance Criteria
1. Add `insta` to `Cargo.toml` under `[dev-dependencies]`.
2. Refactor TUI drawing code to write to any generic output buffer implementing `std::io::Write`.
3. Add snapshot tests validating Split Mode and Stream Mode grids.
4. All unit tests pass under `cargo test` and `make test-rust`.
