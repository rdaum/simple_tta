use camino::Utf8Path;
use eyre::Result;
use marlin::{
    verilator::{VerilatorRuntime, VerilatorRuntimeOptions},
    verilog::prelude::*,
};

// Define our TTA testbench module (includes all dependencies)
#[verilog(src = "../../tta_tb.sv", name = "tta_tb")]
pub struct TtaTestbench;

// Define the SoC-style simulation top with boot ROM and external SRAM.
#[verilog(src = "../../simtop.sv", name = "simtop")]
pub struct SimTop;

pub fn create_tta_runtime() -> Result<VerilatorRuntime> {
    let include_paths = [Utf8Path::new("../../rtl"), Utf8Path::new("../../")];
    let src_files = [
        Utf8Path::new("../../tta_tb.sv"),
        Utf8Path::new("../../rtl/tta.sv"),
        Utf8Path::new("../../rtl/sequencer.sv"),
        Utf8Path::new("../../rtl/decoder.sv"),
        Utf8Path::new("../../rtl/execute.sv"),
        Utf8Path::new("../../rtl/register_unit.sv"),
        Utf8Path::new("../../rtl/alu_unit.sv"),
        Utf8Path::new("../../rtl/muldiv_unit.sv"),
        Utf8Path::new("../../rtl/stack_unit.sv"),
        Utf8Path::new("../../rtl/barrier_unit.sv"),
    ];

    VerilatorRuntime::new(
        Utf8Path::new("../../artifacts"),
        &src_files,
        &include_paths,
        [],
        VerilatorRuntimeOptions::default_logging(),
    )
    .map_err(|e| eyre::eyre!("Failed to create runtime: {}", e))
}

pub fn create_simtop_runtime() -> Result<VerilatorRuntime> {
    let include_paths = [Utf8Path::new("../../rtl"), Utf8Path::new("../../")];
    let src_files = [
        Utf8Path::new("../../simtop.sv"),
        Utf8Path::new("../../rtl/tta.sv"),
        Utf8Path::new("../../rtl/sequencer.sv"),
        Utf8Path::new("../../rtl/decoder.sv"),
        Utf8Path::new("../../rtl/execute.sv"),
        Utf8Path::new("../../rtl/register_unit.sv"),
        Utf8Path::new("../../rtl/alu_unit.sv"),
        Utf8Path::new("../../rtl/muldiv_unit.sv"),
        Utf8Path::new("../../rtl/stack_unit.sv"),
        Utf8Path::new("../../rtl/barrier_unit.sv"),
        Utf8Path::new("../../rtl/blkram.sv"),
    ];

    VerilatorRuntime::new(
        Utf8Path::new("../../artifacts"),
        &src_files,
        &include_paths,
        [],
        VerilatorRuntimeOptions::default_logging(),
    )
    .map_err(|e| eyre::eyre!("Failed to create simtop runtime: {}", e))
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

pub struct SramSim {
    mem: Vec<u64>, // 36-bit tagged words stored in u64
}

impl SramSim {
    pub fn new(words: usize) -> Self {
        Self {
            mem: vec![0; words],
        }
    }

    pub fn read(&self, addr: usize) -> u64 {
        self.mem.get(addr).copied().unwrap_or(0)
    }

    pub fn write(&mut self, addr: usize, data: u64, wstrb: u8) {
        let Some(slot) = self.mem.get_mut(addr) else {
            return;
        };

        // Byte strobes affect the low 32 bits (value portion).
        // Tag bits [35:32] are always written when any strobe is active.
        let old_val = *slot as u32;
        let new_val = data as u32;
        let mut bytes = old_val.to_le_bytes();
        let write_bytes = new_val.to_le_bytes();
        for idx in 0..4 {
            if (wstrb & (1 << idx)) != 0 {
                bytes[idx] = write_bytes[idx];
            }
        }
        let val32 = u32::from_le_bytes(bytes) as u64;
        let tag = if wstrb != 0 { data & 0xF_0000_0000 } else { *slot & 0xF_0000_0000 };
        *slot = tag | val32;
    }
}

pub struct SimTopHarness<'a> {
    model: SimTop<'a>,
    sram: SramSim,
    ticks: u64,
}

impl<'a> SimTopHarness<'a> {
    pub fn new(model: SimTop<'a>, sram_words: usize) -> Self {
        Self {
            model,
            sram: SramSim::new(sram_words),
            ticks: 0,
        }
    }

    pub fn model(&self) -> &SimTop<'a> {
        &self.model
    }

    pub fn model_mut(&mut self) -> &mut SimTop<'a> {
        &mut self.model
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    fn drive_sram(&mut self) {
        if self.model.sram_valid_o != 0 {
            let addr = self.model.sram_addr_o as usize;
            let wstrb = self.model.sram_wstrb_o as u8;

            if wstrb != 0 {
                self.sram.write(addr, self.model.sram_data_o, wstrb);
            }

            self.model.sram_data_i = self.sram.read(addr);
            self.model.sram_ready_i = 1;
        } else {
            self.model.sram_data_i = 0;
            self.model.sram_ready_i = 0;
        }
    }

    pub fn reset(&mut self, reset_cycles: u32) {
        self.model.rst_i = 1;
        self.model.sysclk_i = 0;
        self.model.sram_data_i = 0;
        self.model.sram_ready_i = 0;
        self.model.uart_rxd_i = 1;
        self.model.eval();

        for _ in 0..reset_cycles {
            self.step_cycle();
        }

        self.model.rst_i = 0;
    }

    pub fn step_cycle(&mut self) {
        self.model.sysclk_i = 1;
        self.drive_sram();
        self.model.eval();
        self.ticks += 1;

        self.model.sysclk_i = 0;
        self.drive_sram();
        self.model.eval();
        self.ticks += 1;
    }

    pub fn run_cycles(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.step_cycle();
        }
    }
}
