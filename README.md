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

It has 32 32-bit registers, a 32-bit program counter, separate program
and data buses, and 8 integer ALUs, and simple instructions for:

  * Reading and writing to memory and registers and memory pointed to
    by registers.
  * Setting and reading ALU values and operations
  * Setting and reading the program counter.
  
The instruction set is very simple and is best understood by reading
the assembler implementation in tests/assembler.rs

  * All instructions have a source unit (ALU, register, memory, or
    program counter), and a destination unit (same).
  * Each instruction can take a 12-bit immediate value, or when it
    makes sense, a 32-bit operand which follows in the program stream.

Unlike a normal instruction set it is the responsibility of the
program author to be aware of which ALUs, etc. are currently being
used, and schedule accordingly.

### What can't it do yet?

  * I'd like to add support for interrupts.
  * I intend on adding a LIFO stack. I would like this to be useful
    for writing virtual machines.
  * I'd like to add read/writes of bytewise. For now have to use
    bitmasking via an ALU for this.
  * Who knows? I aim for exotic fun.

### Building, running

The project now uses Rust with the Marlin library for simulation:

  * `cargo test` runs the full test suite including integration tests
    and property-based tests
  * `cargo run` starts a basic simulator
  * The old C++/cmake simulator is still present in simulator/ but no
    longer maintained
  * A simple fusesoc core file is present for FPGA synthesis
  
### But this sucks, because <XXXX>?

  * Well I'm not as smart as you! This is a toy.
  * But ... Contributions welcome.
