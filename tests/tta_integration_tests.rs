use marlin::verilator::VerilatorRuntime;
use std::collections::HashMap;

use tta_sim::{instr, Unit, TtaTestbench, create_tta_runtime};


fn create_runtime() -> Result<VerilatorRuntime, Box<dyn std::error::Error>> {
    Ok(create_tta_runtime()?)
}

/// Test infrastructure helper functions
struct TtaTestHelper {
    cycle_count: u32,
    instruction_memory: HashMap<u32, u32>,
    data_memory: HashMap<u32, u32>,
}

impl TtaTestHelper {
    fn new() -> Self {
        Self {
            cycle_count: 0,
            instruction_memory: HashMap::new(),
            data_memory: HashMap::new(),
        }
    }

    /// Reset the processor
    fn reset<'a>(&mut self, tta: &mut TtaTestbench<'a>) {
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.eval();
    }

    /// Single clock step
    fn step<'a>(&mut self, tta: &mut TtaTestbench<'a>) {
        // Rising edge
        tta.clk_i = 1;

        // Handle memory interface for instruction bus
        if tta.instr_valid_o != 0 {
            let addr = tta.instr_addr_o;
            tta.instr_data_read_i = *self.instruction_memory.get(&addr).unwrap_or(&0);
        }

        // Handle memory interface for data bus
        if tta.data_valid_o != 0 {
            let addr = tta.data_addr_o;
            if tta.data_wstrb_o != 0 {
                // Write operation
                self.data_memory.insert(addr, tta.data_data_write_o);
            } else {
                // Read operation
                tta.data_data_read_i = *self.data_memory.get(&addr).unwrap_or(&0);
            }
        }

        tta.eval();

        // Falling edge
        tta.clk_i = 0;
        tta.eval();

        self.cycle_count += 1;
    }

    /// Run until reset is released
    fn run_until_reset_released<'a>(
        &mut self,
        tta: &mut TtaTestbench<'a>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.reset(tta);
        self.step(tta);
        tta.rst_i = 0;
        self.step(tta);
        Ok(())
    }

    /// Run for a specific number of cycles
    fn run_for_cycles<'a>(&mut self, tta: &mut TtaTestbench<'a>, cycles: u32) -> u32 {
        let start_cycle = self.cycle_count;
        for _ in 0..cycles {
            if tta.rst_i == 0 {
                self.step(tta);
            } else {
                break;
            }
        }
        self.cycle_count - start_cycle
    }

    /// Load instructions into instruction memory
    fn load_instructions(&mut self, instructions: &[u32], start_addr: u32) {
        for (i, &instr) in instructions.iter().enumerate() {
            self.instruction_memory.insert(start_addr + i as u32, instr);
        }
    }

    /// Set data memory value
    fn set_data_memory(&mut self, addr: u32, value: u32) {
        self.data_memory.insert(addr, value);
    }

    /// Get data memory value
    fn get_data_memory(&self, addr: u32) -> u32 {
        *self.data_memory.get(&addr).unwrap_or(&0)
    }

    /// Check if instruction is done
    fn is_instruction_done<'a>(&self, tta: &TtaTestbench<'a>) -> bool {
        tta.instr_done_o != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        helper.run_until_reset_released(&mut tta)?;
        assert_eq!(tta.rst_i, 0);
        Ok(())
    }

    #[test]
    fn test_register_set_abs_memory_set_abs() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load the test program:
        // 1. Move absolute immediate 666 to register 0
        // 2. Move register 0 to memory address 123
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(666)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(123),
        ];

        // Convert instructions to machine code
        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Run for up to 8 cycles
        let cycles_used = helper.run_for_cycles(&mut tta, 8);
        assert!(cycles_used <= 8, "Test used more than 8 cycles");

        // Verify the result
        assert_eq!(tta.rst_i, 0);
        assert_eq!(helper.get_data_memory(123), 666);

        Ok(())
    }

    #[test]
    fn test_mem_immediate_to_mem_immediate() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program: copy from memory[123] to memory[124]
        let program = vec![instr()
            .src(Unit::UNIT_MEMORY_IMMEDIATE)
            .si(123)
            .dst(Unit::UNIT_MEMORY_IMMEDIATE)
            .di(124)];

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Set up initial memory state
        helper.set_data_memory(123, 666);

        // Run for up to 25 cycles (as in C++ test)
        helper.run_for_cycles(&mut tta, 25);

        // Verify the result
        assert_eq!(helper.get_data_memory(124), 666);
        Ok(())
    }

    #[test]
    fn test_mem_operand_to_mem_operand() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program: copy from memory operand to memory operand
        let program = vec![instr()
            .src(Unit::UNIT_MEMORY_OPERAND)
            .soperand(123)
            .dst(Unit::UNIT_MEMORY_OPERAND)
            .doperand(124)];

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Set up initial memory state
        helper.set_data_memory(123, 666);

        // Run for up to 25 cycles
        helper.run_for_cycles(&mut tta, 25);

        // Verify the result
        assert_eq!(helper.get_data_memory(124), 666);
        Ok(())
    }

    #[test]
    fn test_pointer_val_to_mem_immediate() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program equivalent to C++ test:
        // 1. Store 666 to memory[123]
        // 2. Store 123 to register[1] (pointer)
        // 3. Load from register[1] pointer to memory[124]
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(666)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(123),
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(123)
                .dst(Unit::UNIT_REGISTER)
                .di(1),
            instr()
                .src(Unit::UNIT_REGISTER_POINTER)
                .si(1)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(124),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Run for up to 100 cycles
        helper.run_for_cycles(&mut tta, 100);

        // Verify the result
        assert_eq!(helper.get_data_memory(124), 666);
        Ok(())
    }

    #[test]
    fn test_mem_operand_to_register_to_memory_operand() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program: memory operand -> register -> memory operand
        let program = vec![
            instr()
                .src(Unit::UNIT_MEMORY_OPERAND)
                .soperand(123)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(0)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(124),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Set up initial memory state
        helper.set_data_memory(123, 666);

        // Run for up to 25 cycles
        helper.run_for_cycles(&mut tta, 25);

        // Verify the result
        assert_eq!(helper.get_data_memory(124), 666);
        Ok(())
    }

    #[test]
    fn test_alu_addition() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program for ALU addition: 666 + 111 = 777
        let program = vec![
            // Load 666 into ALU left input
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(666)
                .dst(Unit::UNIT_ALU_LEFT)
                .di(0),
            // Load 111 into ALU right input
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(111)
                .dst(Unit::UNIT_ALU_RIGHT)
                .di(0),
            // Set ALU operation to ADD
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(1) // ALU_ADD = 1
                .dst(Unit::UNIT_ALU_OPERATOR)
                .di(0),
            // Store ALU result to memory[123]
            instr()
                .src(Unit::UNIT_ALU_RESULT)
                .si(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(123),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Run for up to 17 cycles (as in C++ test)
        let cycles_used = helper.run_for_cycles(&mut tta, 17);
        assert!(cycles_used <= 17, "Test used more than 17 cycles");

        // Verify the result: 666 + 111 = 777
        assert_eq!(helper.get_data_memory(123), 777);
        assert!(helper.is_instruction_done(&tta));

        Ok(())
    }
}
