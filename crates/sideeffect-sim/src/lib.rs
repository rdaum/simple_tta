pub mod simulator;

// Re-export sideeffect-asm so downstream users only need one dependency.
pub use sideeffect_asm::{self, assembler, dataflow, instr, ALUOp, Instr, Unit};

pub use simulator::{
    create_simtop_runtime, create_tta_runtime, test_basic_reset_sequence, SimTop, SimTopHarness,
    SramSim, TtaTestbench,
};
