# Repository Guidelines

## Project Structure & Module Organization
The active implementation is Rust-first. Core library code lives in `src/`: `assembler.rs` encodes instructions, `simulator.rs` wraps the Marlin/Verilator runtime, `lib.rs` exports the public API, and `main.rs` runs the `simtop` simulator. End-to-end and property tests live in `tests/` as `tta_integration_tests.rs` and `tta_property_tests.rs`. HDL sources live in `rtl/`, with top-level simulation wrappers in `tta_tb.sv` and `simtop.sv`.

## Build, Test, and Development Commands
Use Cargo for day-to-day work:

- `cargo build` builds the Rust library and CLI.
- `cargo run -- --cycles 200` runs the `simtop` Marlin simulator.
- `cargo run -- --trace-file simtop.vcd` writes a VCD trace while simulating.
- `cargo test` runs unit, integration, and property-based tests.
- `cargo fmt` formats Rust code before review.

## Coding Style & Naming Conventions
Follow standard Rust formatting with 4-space indentation and `cargo fmt`. Use `snake_case` for functions, modules, and test helpers. Keep types and traits in `UpperCamelCase`. Preserve existing hardware-facing enum names such as `ALU_ADD` and `UNIT_REGISTER`; they intentionally mirror encoded instruction units. Prefer small helpers and explicit assertions when encoding instruction invariants.

## Testing Guidelines
Add or update tests for every behavior change. Keep integration coverage in `tests/tta_integration_tests.rs` and use `proptest` patterns in `tests/tta_property_tests.rs` for invariant-style checks. Name tests descriptively with `test_` or `prop_` prefixes to match the current suite. Run `cargo test` locally before opening a PR.

## Commit & Pull Request Guidelines
Recent history mixes plain summaries and bracketed prefixes, for example `Implement 256-byte (64 word) stacks` and `[chore] rm gunk file`. Prefer short, imperative commit subjects; optional prefixes like `[chore]` are fine when they add clarity. PRs should explain the behavioral change, note affected areas (`src/`, `tests/`, `rtl/`), and include test results. Add screenshots only if a UI or waveform-related artifact materially helps review.
