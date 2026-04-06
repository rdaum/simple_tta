## What is this?

A 36-bit tagged soft processor core with a [Transport Triggered
Architecture](https://en.wikipedia.org/wiki/Transport_triggered_architecture),
written in synthesizable Verilog. It targets FPGAs as a
programmable coprocessor for language runtimes, with native
support for tagged values, hardware stacks, and GC write barriers.

### What's it for?

It started as a learning project, but has grown into something
that could maybe be useful as a small, predictable compute core
for running interpreters or JIT-compiled bytecode on an FPGA.

My idea was to permit the deployment of Lua, Lisp, or WebAssembly on a soft core that fits in a few thousand gates, with
hardware support for the primitives those runtimes need (tagged pointers, type dispatch, cons cell access, garbage
collection barriers).

The accompanying Rust tooling includes an assembler and a dataflow graph
compiler that handles ALU lane scheduling and label resolution,
making it practical to generate TTA programs from higher-level
representations.

### What can it do?

**Hardware resources:** 32 tagged registers, 8 independent
integer ALU lanes, 8 hardware stacks (32 words each), a 32-entry
GC write barrier FIFO, separate instruction and data buses, a
2-entry instruction queue with prefetch, and a condition register
for branching. All configurable via Verilog parameters. All
memory is word-addressed.

**Programming model:** there is really only one kind of
instruction: *move a value from a source unit to a destination
unit*. Computation is a side effect of routing data through
functional units. The programmer (or compiler) explicitly
schedules operations across ALU lanes — there is no hardware
hazard detection or out-of-order execution.

**36-bit tagged values:** every register, stack slot, and memory
word is 36 bits — a 32-bit value plus a 4-bit sidecar tag in
bits [35:32]. The tag is *alongside* the value, not inside it,
so addresses are always clean 32-bit integers with no masking.
4 tag bits give 16 types — enough for a real Lisp or Lua runtime.
A single-move DEREF loads from memory using the 32-bit value as
an address, enabling `(car x)` in one instruction. Type dispatch
is two instructions (read tag → branch). The ALU operates on the
32-bit value portion and preserves the left operand's tag in the
result. Tags align naturally with FPGA block RAM (36-bit native
width on Xilinx 7-series).

**Predication:** any instruction can be conditionally executed
based on the condition register, without a branch. This avoids
pipeline stalls for simple conditional moves and type dispatch.

**Synthesizable:** the design passes Yosys synthesis and
Verilator lint with zero warnings. All sequential logic uses
non-blocking assignments. CI runs tests, lint, and synthesis on
every push.

### Instruction encoding

Every instruction starts with a 32-bit opcode word:

```
 31  28 27 26 25    18 17    10 9    5 4     0
+------+-----+-------+-------+------+-------+
| rsvd |pred |  di   |  si   | dst  |  src  |
+------+-----+-------+-------+------+-------+
  4 b   2 b    8 b     8 b    5 b     5 b
```

The 5-bit source and destination unit fields select *what* to
read from and write to (32 slots, 24 currently used). The 8-bit
immediate fields carry unit-specific parameters (register index,
ALU lane, stack ID, small address/literal, etc.). The 2-bit
predicate field enables conditional execution without branching.

When a unit needs a full 32-bit value that won't fit in 8 bits
(memory operand addresses, large literals), an extra word follows
the opcode in the instruction stream. Instructions are therefore
1, 2, or 3 words long depending on whether the source and/or
destination require an extended operand.

### Transport units

| # | Unit | As source | As destination |
|---|------|-----------|----------------|
| 0 | `NONE` | Yields 0 | Discards |
| 1 | `STACK` | Pop from stack N | Push to stack N |
| 2 | `STACK_INDEX` | Peek at offset in stack N | Poke at offset in stack N |
| 3 | `REG` | Read register N (raw) | Write register N (raw) |
| 4 | `REG_VALUE` | Read 32-bit value (tag zeroed) | Write value, preserve tag |
| 5 | `REG_TAG` | Read 4-bit tag (zero-extended) | Write tag, preserve value |
| 6 | `REG_DEREF` | Load mem[value+offset] | Store to mem[value+offset] |
| 7 | `ALU_LEFT` | Read ALU lane N left input | Set ALU lane N left input |
| 8 | `ALU_RIGHT` | Read ALU lane N right input | Set ALU lane N right input |
| 9 | `ALU_OP` | -- | Set ALU lane N operation |
| 10 | `ALU_RESULT` | Read ALU lane N result | -- |
| 11 | `MEM_IMM` | Load from 8-bit address | Store to 8-bit address |
| 12 | `MEM_OP` | Load from 32-bit address (next word) | Store to 32-bit address |
| 13 | `IMM` | Literal 8-bit value (0-255) | -- |
| 14 | `OPERAND` | Literal 32-bit value (next word) | -- |
| 15 | `PC` | Read program counter | Jump (set PC) |
| 16 | `PC_COND` | -- | Jump only if condition is set |
| 17 | `COND` | Read condition (0 or 1) | Set condition (nonzero = true) |
| 18 | `BARRIER` | Pop barrier FIFO (GC drain) | Push to barrier FIFO |
| 19 | `MEM_BYTE` | Byte load (32-bit addr, imm=offset) | Byte store (32-bit addr, imm=offset) |
| 20 | `STACK_POP_VALUE` | Pop with tag bits zeroed | -- |
| 21 | `STACK_POP_TAG` | Pop with tag bits only | -- |
| 22 | `STACK_PEEK_VALUE` | Peek with tag bits zeroed | -- |
| 23 | `STACK_PEEK_TAG` | Peek with tag bits only | -- |
| 24-31 | *free* | 8 slots for future units | |

### ALU operations

Each ALU lane holds a left operand (A), right operand (B), and an
operator. You configure a lane by moving values into its inputs and
operator, then read the result back. Arithmetic operates on the
32-bit value portion only; the 4-bit tag from the left (A) operand
is preserved in the result. Most operations are combinational —
available immediately with no extra clock cycle. The 16 operations
are:

`NOP`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `EQL`, `SL` (shift left),
`SR` (shift right), `SRA` (arithmetic shift right), `NOT` (unary,
B ignored), `AND`, `OR`, `XOR`, `GT`, `LT`

Comparisons (`EQL`, `GT`, `LT`) produce 0 or 1.

`MUL`, `DIV`, and `MOD` are handled by a shared multi-cycle unit
(32 cycles each) rather than combinational logic, keeping the ALU
lanes small. A single muldiv unit is shared across all 8 lanes —
the result is computed when the lane's result port is read. The ISA
encoding is unchanged; the only difference is timing.

### Branching and predication

The processor has a 1-bit condition register and three mechanisms
for conditional execution:

* **Unconditional jump:** move a target address to `PC`.
* **Conditional branch:** set the condition register via
  `COND`, then move a target address to `PC_COND`. The jump
  is only taken if the condition register is nonzero.
* **Predication:** any instruction can carry a predicate flag
  (`if_set` or `if_clear`). When the condition doesn't match,
  the instruction completes in one cycle with no side effects
  and no pipeline stall.

A compare-and-branch sequence:

```
42  → alu[0].left       ; set up comparison
10  → alu[0].right
GT  → alu[0].operator   ; 42 > 10 = 1
alu[0].result → cond    ; latch result into condition register
LABEL → pc_cond         ; jump if condition is set
```

Predication eliminates branches for simple conditional moves:

```
alu[0].result → cond
value → reg[0]  [if_set]    ; only executes if cond is set, no stall
```

### Tagged registers

Every register is 36 bits: a 32-bit value in bits [31:0] and a
4-bit tag in bits [35:32]. Four dedicated unit types control how
the tag and value are accessed:

| Unit | Read | Write |
|------|------|-------|
| `REG` (3) | Full 36-bit tagged word | Full 36-bit tagged word |
| `REG_VALUE` (4) | 32-bit value only (tag zeroed) | Preserve tag, replace value |
| `REG_TAG` (5) | Tag only (zero-extended to 36 bits) | Preserve value, replace tag |
| `REG_DEREF` (6) | Load mem[value + offset] | Store to mem[value + offset] |

`REG_DEREF` uses the full 32-bit value as a memory address — no
tag stripping needed, since the tag is in the sidecar bits, not
in the value. imm[7:5] provides a word offset (0-7) for struct
field access. This enables single-move cons cell access:

```
; r0 holds value=20 with tag=1 (cons)
reg[r0, DEREF+0] → reg[1]   ; car — load mem[20]
reg[r0, DEREF+1] → reg[2]   ; cdr — load mem[21]
```

Type dispatch becomes two instructions:

```
reg[r0, TAG] → cond          ; extract 4-bit tag, set condition
HANDLER → pc_cond            ; branch if tag is nonzero
```

**Stacks** support VALUE and TAG modes via dedicated unit types
(`STACK_POP_VALUE`, `STACK_POP_TAG`, `STACK_PEEK_VALUE`,
`STACK_PEEK_TAG`), enabling type dispatch directly from the
stack without an intermediate register:

```
peek_tag[stack0, offset0] → cond   ; check type of TOS
HANDLER → pc_cond                  ; branch on type
```

### Write barrier (hardware GC support)

The `BARRIER` unit is a 32-entry hardware FIFO for garbage
collection support. The mutator logs addresses of pointer
stores; the GC drains the FIFO to find dirty regions.

```
src_value → mem[addr]          ; store a pointer (normal)
addr      → barrier            ; log the address for GC
```

The GC drains the barrier by popping:

```
barrier → reg[0]               ; next dirty address
```

Combined with tagged registers (TAG mode for type checking,
DEREF for pointer chasing), this provides the core primitives
for hardware-assisted GC in e.g. a Lisp or Lua runtime.

### Byte memory access

The `MEM_BYTE` unit provides single-byte loads and stores using
a 32-bit operand address and an imm[1:0] byte offset (0-3).
Reads zero-extend the selected byte to 32 bits; writes strobe
only the selected byte lane.

```
; Write 0x42 to byte 2 of word at address 100
0x42 → mem_byte[addr=100, offset=2]
; Read byte 2 back
mem_byte[addr=100, offset=2] → reg[0]   ; r0 = 0x00000042
```

### Dataflow compiler

The `sideeffect-asm` crate includes a dataflow graph compiler
(`crates/sideeffect-asm/src/dataflow.rs`) for programmatic code
generation. Build a graph of operations with data dependencies,
and the compiler emits scheduled TTA move sequences:

```rust
let mut g = Graph::new();
let a = g.constant(42);
let b = g.constant(10);
let sum = g.add(a, b);
g.store_mem(100, sum);
let moves = g.compile();  // → Vec<Instr>
```

**Interleaving scheduler.** Independent ALU operations are
batched and their setup moves interleaved across lanes — all
left operands first, then all rights, then all operators. This
maximizes the benefit of 8 ALU lanes without manual scheduling.

**Labels.** Branch targets use `label()` / `place_label()` /
`branch_cond_label()` with automatic address resolution after
instruction layout:

```rust
let skip = g.label();
g.set_cond(cmp);
g.branch_cond_label(skip);
// ... else path ...
g.place_label(skip);
// ... then path ...
```

### Microarchitecture

The core has three stages: **sequencer** (fetch), **decoder**
(combinational), and **execute**.

**Instruction queue.** The sequencer fetches instructions into a
2-entry FIFO that runs ahead of execute, hiding bus latency for
sequential code. Each queue entry captures the instruction's PC
so that `UNIT_PC` reads return the correct value regardless of
how far ahead the fetch has progressed.

**Fetch stall policy.** The sequencer will not fetch past a
control-flow instruction (`PC` or `PC_COND` as destination). It
stalls until execute accepts the branch, at which point either a
flush occurs (taken) or sequential fetch resumes (not taken). This
means the queue never contains wrong-path instructions —
correctness is structural, not flush-dependent.

**Fused execute.** When both the source and destination resolve in
a single cycle (register, ALU, immediate, condition, PC), the
execute stage completes both phases in one cycle with no state
transition. Multi-cycle sources (memory loads, stack pops) go
through separate wait states.

**Valid/accept handshake.** The sequencer and execute communicate
via a level-based valid/accept protocol:

* `instr_valid` — high when the queue has a complete instruction
* `instr_accept` — combinational from execute, fires for one
  cycle when execute is idle

On accept, the sequencer dequeues the head entry and promotes it
to the decoder-facing outputs. Execute latches `exec_active` and
begins processing on the next cycle.

**PC semantics.** `UNIT_PC` as a source returns
`instruction_address + instruction_word_count` — the address of
the next sequential instruction. This is the PC value captured in
the queue entry, not the fetch address (which may have advanced
further).

**Synthesizable.** All sequential logic uses non-blocking
assignments (`<=`). The design is correct for FPGA synthesis, not
just Verilator simulation.

### What's next?

* Interrupts
* 1-cycle fused pipeline (forwarding infrastructure is in place,
  activation blocked on one edge case)
* Lua bytecode interpreter or JIT as a proof-of-concept runtime
* Wider instruction bus / instruction cache for real FPGA memory
* Jump table unit for computed gotos (type dispatch, eval)

### Cycle counts

Measured from the Verilator simulation with the instruction queue
warm (fetch latency hidden).

| Instruction pattern | Cycles | Notes |
|---------------------|--------|-------|
| imm → reg | 2 | Fused src+dst |
| reg → reg | 2 | Fused src+dst |
| imm → ALU input/op | 2 | Fused src+dst |
| ALU result → reg | 2 | Combinational ALU, fused |
| MUL/DIV/MOD result → reg | ~34 | 32-cycle multi-cycle unit |
| reg → mem (write) | 3 | Fire-and-forget bus write |
| mem → reg (read) | 4 | Bus read wait state |
| operand → reg | 2 | 2-word instruction, fused |
| imm → cond | 2 | Fused |
| pc_cond (not taken) | 2 | 2-word, fused |
| reg[TAG] → reg | 2 | Tag extract, fused |
| reg[DEREF] → reg | 4 | Bus read via register value + offset |
| mem_byte → reg | 4 | Byte read + zero-extend |
| push (via operand) | 5 | 2-word + stack handshake |
| pop → reg | 5 | Stack handshake + arming cycle |
| pop_tag → reg | 5 | Same as pop, tag mask applied |
| predicated skip | 1 | Condition doesn't match, no-op |

The common case — register/immediate/ALU moves — is 2 cycles.
Memory and stack operations pay extra for bus or stack handshakes.
With the 2-entry instruction queue, the fetch cost is fully hidden
for sequential code; branches stall the fetch until resolved.

### Resource utilization

Gate counts from Yosys synthesis (technology-mapped cells,
excluding block RAM):

| Module | Cells | Notes |
|--------|------:|-------|
| execute | 12,600 | Main FSM, muxing, 36-bit data path |
| alu_unit ×8 | 13,500 | 8 combinational ALU lanes (32-bit math + tag pass-through) |
| sequencer | 3,000 | Instruction queue + fetch FSM |
| muldiv_unit | 1,900 | Shared multi-cycle MUL/DIV/MOD |
| stack_unit | 1,800 | 8×32-word stack controller (36-bit words) |
| register_unit ×32 | 1,200 | 32 registers (36-bit flip-flops) |
| barrier_unit | 350 | 32-entry GC write barrier FIFO (36-bit) |
| **Total** | **~34.5k** | |

These counts are for the `tta` core only (excluding the FPGA
wrapper and boot ROM). The design fits on a Xilinx 7-series
part like the CMod A35T (33,280 LUTs), with stack and register
memories mapping to block RAM. The 36-bit data width aligns
with FPGA block RAM native width (36 bits on Xilinx 7-series).
All dimensions (register count, ALU lanes, stack count/depth,
barrier depth) are configurable via Verilog parameters.

### Project structure

The Rust code is a Cargo workspace with two crates:

* **`crates/sideeffect-asm`** — assembler and dataflow compiler. Pure
  Rust, no simulator dependencies. Use this to generate TTA
  programs without a hardware simulator.
* **`crates/sideeffect-sim`** — Verilator/Marlin simulator runtime, CLI,
  and all tests. Depends on `sideeffect-asm` and re-exports its types.

HDL sources live in `rtl/`, with top-level simulation wrappers
`tta_tb.sv` and `simtop.sv` at the repo root.

### Building, running

* `cargo test` runs the full test suite (107 tests: unit,
  integration, and property-based)
* `cargo run -p sideeffect-sim -- --cycles 200` runs the Marlin-backed
  `simtop` wrapper with boot ROM and external SRAM modeling
* `cargo run -p sideeffect-sim -- --trace-file simtop.vcd` writes a VCD
  trace for debugging
* A simple fusesoc core file is present for FPGA synthesis

### But this sucks, because <XXXX>?

* Well I'm not as smart as you! This is a toy.
* But ... Contributions welcome.
* Run `cargo test` and `verilator --lint-only -Wall` before
  submitting.
