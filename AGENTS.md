# Repository Guidelines

## Project Structure & Module Organization
The active implementation is Rust-first. Core library code lives in `src/`: `assembler.rs` encodes instructions, `simulator.rs` wraps the Marlin/Verilator runtime, `lib.rs` exports the public API, and `main.rs` runs the `simtop` simulator. End-to-end and property tests live in `tests/` as `tta_integration_tests.rs` and `tta_property_tests.rs`. HDL sources live in `rtl/`, with top-level simulation wrappers in `tta_tb.sv` and `simtop.sv`.

RTL modules: `decoder.sv` (combinational), `register_unit.sv`, `alu_unit.sv` (combinational), `stack_unit.sv`, `barrier_unit.sv` (write barrier FIFO), `blkram.sv`, `sequencer.sv` (instruction queue + prefetch), `execute.sv` (main FSM), `tta.sv` (top-level core), `cmod_a35t_top.sv` (FPGA wrapper). Shared definitions in `common.vh`.

## Build, Test, and Development Commands
Use Cargo for day-to-day work:

- `cargo build` builds the Rust library and CLI.
- `cargo run -- --cycles 200` runs the `simtop` Marlin simulator.
- `cargo run -- --trace-file simtop.vcd` writes a VCD trace while simulating.
- `cargo test` runs unit, integration, and property-based tests (~103 tests).
- `cargo fmt` formats Rust code before review.

HDL verification (not run by cargo):

- `verilator --lint-only -Wall -sv -Irtl rtl/common.vh rtl/decoder.sv rtl/register_unit.sv rtl/alu_unit.sv rtl/stack_unit.sv rtl/barrier_unit.sv rtl/blkram.sv rtl/sequencer.sv rtl/execute.sv rtl/tta.sv rtl/cmod_a35t_top.sv --top-module cmod_a35t_top` — lint check, must produce zero warnings.
- `yosys -p "read_verilog -sv rtl/common.vh rtl/decoder.sv ... rtl/cmod_a35t_top.sv; synth -top cmod_a35t_top"` — synthesis check (slow, ~1-2 min). The RTL avoids SystemVerilog features Yosys 0.33 doesn't support (no interfaces, no user types in ports, no `return` in functions).

## Coding Style & Naming Conventions
Follow standard Rust formatting with 4-space indentation and `cargo fmt`. Use `snake_case` for functions, modules, and test helpers. Keep types and traits in `UpperCamelCase`. Preserve existing hardware-facing enum names such as `ALU_ADD` and `UNIT_REGISTER`; they intentionally mirror encoded instruction units. Prefer small helpers and explicit assertions when encoding instruction invariants.

RTL: use non-blocking assignments (`<=`) in all sequential logic. Blocking assignments (`=`) only for combinational intermediates (`automatic` variables, `always_comb` blocks). Module ports use plain `logic [N:0]` types, not typedef'd enums (Yosys compatibility). Typedef enums in `common.vh` are for internal use only.

## Testing Guidelines
Add or update tests for every behavior change. Keep integration coverage in `tests/tta_integration_tests.rs` and use `proptest` patterns in `tests/tta_property_tests.rs` for invariant-style checks. Name tests descriptively with `test_` or `prop_` prefixes to match the current suite. Run `cargo test` locally before opening a PR. Run `verilator --lint-only -Wall` after any RTL change.

## Commit & Pull Request Guidelines
Recent history mixes plain summaries and bracketed prefixes, for example `Implement 256-byte (64 word) stacks` and `[chore] rm gunk file`. Prefer short, imperative commit subjects; optional prefixes like `[chore]` are fine when they add clarity. PRs should explain the behavioral change, note affected areas (`src/`, `tests/`, `rtl/`), and include test results. Add screenshots only if a UI or waveform-related artifact materially helps review.
