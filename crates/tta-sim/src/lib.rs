pub mod simulator;

// Re-export tta-asm so downstream users only need one dependency.
pub use tta_asm::{self, assembler, dataflow, instr, AccessWidth, ALUOp, Instr, RegMode, Unit};

pub use simulator::{
    create_simtop_runtime, create_tta_runtime, test_basic_reset_sequence, SimTop, SimTopHarness,
    SramSim, TtaTestbench,
};
