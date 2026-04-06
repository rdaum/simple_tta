# Repository Guidelines

## Project Structure & Module Organization
Cargo workspace with two crates:

- **`crates/sideeffect-asm`**: assembler (`assembler.rs`) and dataflow compiler (`dataflow.rs`). Pure Rust, no simulator dependencies.
- **`crates/sideeffect-sim`**: Verilator/Marlin simulator (`simulator.rs`), CLI (`main.rs`), integration tests, and property tests. Depends on `sideeffect-asm` and re-exports its types.

HDL sources live in `rtl/`, with top-level simulation wrappers `tta_tb.sv` and `simtop.sv` at the repo root.

RTL modules: `decoder.sv` (combinational), `register_unit.sv`, `alu_unit.sv` (combinational), `stack_unit.sv`, `barrier_unit.sv` (write barrier FIFO), `blkram.sv`, `sequencer.sv` (instruction queue + prefetch), `execute.sv` (main FSM), `tta.sv` (top-level core), `cmod_a35t_top.sv` (FPGA wrapper). Shared definitions in `common.vh`.

## Build, Test, and Development Commands
Use Cargo for day-to-day work:

- `cargo build` builds all crates.
- `cargo run -p sideeffect-sim -- --cycles 200` runs the `simtop` Marlin simulator.
- `cargo run -p sideeffect-sim -- --trace-file simtop.vcd` writes a VCD trace while simulating.
- `cargo test` runs all tests across both crates (~113 tests).
- `cargo fmt` formats Rust code before review.

HDL verification (not run by cargo):

- `verilator --lint-only -Wall -sv -Irtl rtl/common.vh rtl/decoder.sv rtl/register_unit.sv rtl/alu_unit.sv rtl/stack_unit.sv rtl/barrier_unit.sv rtl/blkram.sv rtl/sequencer.sv rtl/execute.sv rtl/tta.sv rtl/cmod_a35t_top.sv --top-module cmod_a35t_top` — lint check, must produce zero warnings.
- `yosys -p "read_verilog -sv rtl/common.vh rtl/decoder.sv ... rtl/cmod_a35t_top.sv; synth -top cmod_a35t_top"` — synthesis check (slow, ~1-2 min). The RTL avoids SystemVerilog features Yosys 0.33 doesn't support (no interfaces, no user types in ports, no `return` in functions).

## Coding Style & Naming Conventions
Follow standard Rust formatting with 4-space indentation and `cargo fmt`. Use `snake_case` for functions, modules, and test helpers. Keep types and traits in `UpperCamelCase`. Preserve existing hardware-facing enum names such as `ALU_ADD` and `UNIT_REGISTER`; they intentionally mirror encoded instruction units. Prefer small helpers and explicit assertions when encoding instruction invariants.

RTL: use non-blocking assignments (`<=`) in all sequential logic. Blocking assignments (`=`) only for combinational intermediates (`always_comb` blocks). Module ports use plain `logic [N:0]` types, not typedef'd enums (Yosys compatibility). Typedef enums in `common.vh` are for internal use only.

## Testing Guidelines
Add or update tests for every behavior change. Keep integration coverage in `crates/sideeffect-sim/tests/tta_integration_tests.rs` and use `proptest` patterns in `crates/sideeffect-sim/tests/tta_property_tests.rs` for invariant-style checks. Name tests descriptively with `test_` or `prop_` prefixes to match the current suite. Run `cargo test` locally before opening a PR. Run `verilator --lint-only -Wall` after any RTL change.

## Commit & Pull Request Guidelines
Prefer short, imperative commit subjects. PRs should explain the behavioral change, note affected areas (`crates/`, `rtl/`), and include test results.
