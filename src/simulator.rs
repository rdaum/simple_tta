use camino::Utf8Path;
use eyre::Result;
use marlin::{
    verilator::{VerilatorRuntime, VerilatorRuntimeOptions},
    verilog::prelude::*,
};

// Define our TTA testbench module (includes all dependencies)
#[verilog(src = "tta_tb.sv", name = "tta_tb")]
pub struct TtaTestbench;

pub fn create_tta_runtime() -> Result<VerilatorRuntime> {
    let include_paths = [Utf8Path::new("rtl"), Utf8Path::new(".")];
    let src_files = [
        Utf8Path::new("tta_tb.sv"),
        Utf8Path::new("rtl/tta.sv"),
        Utf8Path::new("rtl/bus_if.sv"),
        Utf8Path::new("rtl/sequencer.sv"),
        Utf8Path::new("rtl/decoder.sv"),
        Utf8Path::new("rtl/execute.sv"),
        Utf8Path::new("rtl/register_unit.sv"),
        Utf8Path::new("rtl/alu_unit.sv"),
    ];

    VerilatorRuntime::new(
        Utf8Path::new("artifacts"),
        &src_files,
        &include_paths,
        [],
        VerilatorRuntimeOptions::default_logging(),
    )
    .map_err(|e| eyre::eyre!("Failed to create runtime: {}", e))
}

pub fn test_basic_reset_sequence(tta: &mut TtaTestbench) -> Result<()> {
    println!("🔄 Testing reset sequence...");

    // Put TTA in reset
    tta.rst_i = 1;
    tta.clk_i = 0;
    tta.eval();

    // Release reset and clock
    tta.rst_i = 0;
    tta.clk_i = 1;
    tta.eval();

    println!("✅ Reset sequence completed");
    Ok(())
}