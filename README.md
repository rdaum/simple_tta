## What is this?

A 32-bit soft processor core with a [Transport Triggered
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
integer ALU lanes, 8 hardware stacks (64 words each), a 32-entry
GC write barrier FIFO, separate instruction and data buses, a
2-entry instruction queue with prefetch, and a condition register
for branching. All memory is word-addressed.

**Programming model:** there is really only one kind of
instruction: *move a value from a source unit to a destination
unit*. Computation is a side effect of routing data through
functional units. The programmer (or compiler) explicitly
schedules operations across ALU lanes — there is no hardware
hazard detection or out-of-order execution.

**Tagged values:** registers and stacks natively support tagged
values (2-bit low tags by default). A single-move DEREF mode
strips the tag, adds an offset, and loads from memory — enabling
`(car x)` in one instruction. Type dispatch is two instructions
(peek tag → branch).

**Synthesizable:** the design passes Yosys synthesis and
Verilator lint with zero warnings. All sequential logic uses
non-blocking assignments. CI runs tests, lint, and synthesis on
every push.

### Instruction encoding

Every instruction starts with a 32-bit opcode word:

```
 31       20 19   16 15        4 3     0
+-----------+-------+-----------+-------+
| dst_imm   |dst_unt| src_imm   |src_unt|
+-----------+-------+-----------+-------+
     12 b      4 b      12 b      4 b
```

The 4-bit source and destination unit fields select *what* to read
from and write to. The 12-bit immediate fields carry
unit-specific parameters (register index, ALU lane, stack ID, a
small address, etc.).

When a unit needs a full 32-bit value that won't fit in 12 bits
(memory operand addresses, large literals), an extra word follows
the opcode in the instruction stream. Instructions are therefore
1, 2, or 3 words long depending on whether the source and/or
destination require an extended operand.

### Transport units

| Unit               | As source                               | As destination                         |
|--------------------|-----------------------------------------|----------------------------------------|
| `REGISTER`         | Read register N (mode-aware, see below) | Write register N (mode-aware)          |
| `ALU_LEFT`         | Read ALU lane N left input (imm[2:0])   | Set ALU lane N left input              |
| `ALU_RIGHT`        | Read ALU lane N right input             | Set ALU lane N right input             |
| `ALU_OPERATOR`     | --                                      | Set ALU lane N operation               |
| `ALU_RESULT`       | Read ALU lane N result                  | --                                     |
| `MEMORY_IMMEDIATE` | Load from 12-bit address                | Store to 12-bit address                |
| `MEMORY_OPERAND`   | Load from 32-bit address (next word)    | Store to 32-bit address                |
| `WRITE_BARRIER`    | Pop barrier FIFO (GC drain)             | Push to barrier FIFO (log address)     |
| `ABS_IMMEDIATE`    | Literal 12-bit value                    | --                                     |
| `ABS_OPERAND`      | Literal 32-bit value (next word)        | --                                     |
| `PC`               | Read program counter                    | Jump (set program counter)             |
| `COND`             | Read condition register (0 or 1)        | Set condition (nonzero = true)         |
| `PC_COND`          | --                                      | Jump only if condition register is set |
| `STACK_PUSH_POP`   | Pop from stack N (mode-aware)           | Push to stack N                        |
| `STACK_INDEX`      | Peek at offset in stack N (mode-aware)  | Poke at offset in stack N              |

All 16 unit codes are assigned. The assembler in
`crates/sideeffect-asm/src/assembler.rs` is the authoritative reference
for encoding details.

### ALU operations

Each ALU lane holds a left operand (A), right operand (B), and an
operator. You configure a lane by moving values into its inputs and
operator, then read the result back. ALU results are combinational
— available immediately with no extra clock cycle. The 16
operations are:

`NOP`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `EQL`, `SL` (shift left),
`SR` (shift right), `SRA` (arithmetic shift right), `NOT` (unary,
B ignored), `AND`, `OR`, `XOR`, `GT`, `LT`

Comparisons (`EQL`, `GT`, `LT`) produce 0 or 1.

### Branching

The processor has a 1-bit condition register and two branch
mechanisms:

* **Unconditional jump:** move a target address to `PC`.
* **Conditional branch:** set the condition register via
  `COND`, then move a target address to `PC_COND`. The jump
  is only taken if the condition register is nonzero.

A compare-and-branch sequence looks like:

```
42  → alu[0].left       ; set up comparison
10  → alu[0].right
GT  → alu[0].operator   ; 42 > 10 = 1
alu[0].result → cond    ; latch result into condition register
LABEL → pc_cond         ; jump if condition is set
```

The condition register is resolved in a prior instruction, so the
pipeline can forward the single-bit result with minimal stall.

### Tagged registers

Registers natively support tagged values, where a small type tag
lives in the low bits of every 32-bit word (2-bit tags by default,
configurable via `TAG_WIDTH`). The `REGISTER` unit's immediate
field encodes an access mode in bits [6:5]:

```
 9   7  6  5  4       0
+-----+-----+---------+
|deref| mode| reg idx |
|offs |     |         |
+-----+-----+---------+
 3 b   2 b     5 b
```

| Mode  | Bits [6:5] | Read                               | Write                                  |
|-------|------------|------------------------------------|----------------------------------------|
| RAW   | 00         | Full 32-bit word                   | Full 32-bit word                       |
| VALUE | 01         | Word with tag bits zeroed          | Preserve tag, set payload              |
| TAG   | 10         | Tag bits only (zero-extended)      | Preserve payload, set tag              |
| DEREF | 11         | Strip tag, load mem[addr + offset] | Strip tag, store to mem[addr + offset] |

DEREF mode uses bits [9:7] as a word offset (0-7) from the
untagged address. This enables single-move cons cell access:

```
; r0 holds a tagged cons pointer (tag=1, addr=20)
reg[r0, DEREF+0] → reg[1]   ; car — load mem[20]
reg[r0, DEREF+1] → reg[2]   ; cdr — load mem[21]
```

Type dispatch becomes two instructions:

```
reg[r0, TAG] → cond          ; extract tag, set condition
HANDLER → pc_cond            ; branch if tag is nonzero
```

**Stacks** support the same RAW/VALUE/TAG modes on pop and peek
(not DEREF). Mode bits are in imm[4:3] for push/pop and
imm[10:9] for indexed access. This enables type dispatch directly
from the stack without an intermediate register:

```
peek[stack0, offset0, TAG] → cond   ; check type of TOS
HANDLER → pc_cond                    ; branch on type
```

### Write barrier (hardware GC support)

The `WRITE_BARRIER` unit is a 32-entry hardware FIFO for
garbage collection support. The mutator logs addresses of
pointer stores; the GC drains the FIFO to find dirty regions.

```
src_value → mem[addr]          ; store a pointer (normal)
addr      → write_barrier      ; log the address for GC
```

The GC drains the barrier by popping:

```
write_barrier → reg[0]         ; next dirty address
```

Combined with tagged registers (TAG mode for type checking,
DEREF for pointer chasing), this provides the core primitives
for hardware-assisted GC in e.g. a Lisp or Lua runtime.

### Sub-word memory access

`MEMORY_OPERAND` supports byte and halfword
loads/stores. The 12-bit immediate field encodes the access width
and byte offset:

```
 11  10  9   8  7          0
+------+------+------------+
|width | offs | addr / reg |
+------+------+------------+
  2 b    2 b      8 b
```

* `width`: 00 = word (32-bit), 01 = byte, 10 = halfword
* `offs`: byte offset within the 32-bit word (0-3 for byte,
  0 or 2 for halfword)

On writes, only the selected byte lane(s) are strobed. On reads,
the selected bytes are zero-extended to 32 bits. This adds no
extra clock cycles — the lane selection is pure combinational
logic in the existing data path.

`MEMORY_IMMEDIATE` always performs full-word access (the full
12-bit immediate is used as a word address).

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

### Cycle counts

Measured from the Verilator simulation with the instruction queue
warm (fetch latency hidden).

| Instruction pattern | Cycles | Notes                           |
|---------------------|--------|---------------------------------|
| imm → reg           | 2      | Fused src+dst                   |
| reg → reg           | 2      | Fused src+dst                   |
| imm → ALU input/op  | 2      | Fused src+dst                   |
| ALU result → reg    | 2      | Combinational ALU, fused        |
| reg → mem (write)   | 3      | Fire-and-forget bus write       |
| mem → reg (read)    | 4      | Bus read wait state             |
| abs_operand → reg   | 2      | 2-word instruction, fused       |
| imm → cond          | 2      | Fused                           |
| pc_cond (not taken) | 2      | 2-word, fused                   |
| reg[TAG] → reg      | 4      | Tag extract                     |
| reg[DEREF] → reg    | 2      | Bus read through tagged pointer |
| push (via operand)  | 5      | 2-word + stack handshake        |
| pop → reg           | 5      | Stack handshake + arming cycle  |

The common case — register/immediate/ALU moves — is 2 cycles.
Memory and stack operations pay extra for bus or stack handshakes.
With the 2-entry instruction queue, the fetch cost is fully hidden
for sequential code; branches stall the fetch until resolved.

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

* `cargo test` runs the full test suite (113 tests: unit,
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
