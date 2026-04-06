## What is this?

A very simple 32-bit processor written in Verilog with a kind of
[Transport Triggered
Architecture](https://en.wikipedia.org/wiki/Transport_triggered_architecture).

### What's it for?

I built it just to play with the concept, learn some HDL stuff, and to
potentially use it as the basis for domain specific coprocessors for
other hobby projects of mine.  In particular I'd like to use it for
support for language interpreters for other hobby FPGA projects.

### What can it do?

It has 32 32-bit registers, a 32-bit program counter, separate
program and data buses, 8 independent integer ALU lanes, and 8
hardware stacks (64 words each). All memory is word-addressed (each
address = one 32-bit word).

Unlike a conventional processor there is really only one kind of
instruction: *move a value from a source unit to a destination unit*.
Computation is a side effect of routing data through functional units.
It is the responsibility of the program author to be aware of which
ALUs, etc. are currently in use and schedule accordingly.

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

| Unit | As source | As destination |
|------|-----------|----------------|
| `REGISTER` | Read register N (imm[4:0]) | Write register N |
| `ALU_LEFT` | Read ALU lane N left input (imm[2:0]) | Set ALU lane N left input |
| `ALU_RIGHT` | Read ALU lane N right input | Set ALU lane N right input |
| `ALU_OPERATOR` | -- | Set ALU lane N operation |
| `ALU_RESULT` | Read ALU lane N result | -- |
| `MEMORY_IMMEDIATE` | Load from 12-bit address | Store to 12-bit address |
| `MEMORY_OPERAND` | Load from 32-bit address (next word) | Store to 32-bit address |
| `REGISTER_POINTER` | Load from address in register N | Store to address in register N |
| `ABS_IMMEDIATE` | Literal 12-bit value | -- |
| `ABS_OPERAND` | Literal 32-bit value (next word) | -- |
| `PC` | Read program counter | Jump (set program counter) |
| `STACK_PUSH_POP` | Pop from stack N (imm[2:0]) | Push to stack N |
| `STACK_INDEX` | Peek at offset in stack N | Poke at offset in stack N |

The assembler in `src/assembler.rs` is the authoritative reference for
encoding details.

### ALU operations

Each ALU lane holds a left operand (A), right operand (B), and an
operator. You configure a lane by moving values into its inputs and
operator, then read the result back. The 16 operations are:

`NOP`, `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `EQL`, `SL` (shift left),
`SR` (shift right), `SRA` (arithmetic shift right), `NOT` (unary,
B ignored), `AND`, `OR`, `XOR`, `GT`, `LT`

Comparisons (`EQL`, `GT`, `LT`) produce 0 or 1.

### What can't it do yet?

  * I'd like to add support for interrupts.
  * I'd like to add read/writes of bytewise. For now have to use
    bitmasking via an ALU for this.
  * Who knows? I aim for exotic fun.

### Building, running

The project now uses Rust with the Marlin library for simulation:

  * `cargo test` runs the full test suite including integration tests
    and property-based tests
  * `cargo run -- --cycles 200` runs the Marlin-backed `simtop`
    wrapper with boot ROM and external SRAM modeling
  * `cargo run -- --trace-file simtop.vcd` writes a VCD trace for
    debugging
  * A simple fusesoc core file is present for FPGA synthesis
  
### But this sucks, because <XXXX>?

  * Well I'm not as smart as you! This is a toy.
  * But ... Contributions welcome.
