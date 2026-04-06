use marlin::verilator::{VerilatedModelConfig, VerilatorRuntime};
use std::collections::HashMap;

use sideeffect_sim::{create_tta_runtime, instr, TtaTestbench, Unit};
use sideeffect_sim::dataflow::Graph;

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

        // Drive instruction bus: assert ready only when responding to valid.
        if tta.instr_valid_o != 0 {
            let addr = tta.instr_addr_o;
            tta.instr_data_read_i = *self.instruction_memory.get(&addr).unwrap_or(&0);
            tta.instr_ready_i = 1;
        } else {
            tta.instr_ready_i = 0;
        }

        // Drive data bus: assert ready only when responding to valid.
        if tta.data_valid_o != 0 {
            let addr = tta.data_addr_o;
            let wstrb = tta.data_wstrb_o as u8;
            if wstrb != 0 {
                // Write operation with per-byte strobes
                let existing = *self.data_memory.get(&addr).unwrap_or(&0);
                let mut bytes = existing.to_le_bytes();
                let write_bytes = tta.data_data_write_o.to_le_bytes();
                for i in 0..4 {
                    if (wstrb & (1 << i)) != 0 {
                        bytes[i] = write_bytes[i];
                    }
                }
                self.data_memory.insert(addr, u32::from_le_bytes(bytes));
            } else {
                // Read operation
                tta.data_data_read_i = *self.data_memory.get(&addr).unwrap_or(&0);
            }
            tta.data_ready_i = 1;
        } else {
            tta.data_ready_i = 0;
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

    /// Step until instr_done_o pulses high, returning the number of cycles taken.
    /// Returns None if max_cycles is reached without done.
    fn run_until_done<'a>(&mut self, tta: &mut TtaTestbench<'a>, max_cycles: u32) -> Option<u32> {
        for i in 0..max_cycles {
            self.step(tta);
            if tta.instr_done_o != 0 {
                return Some(i + 1);
            }
        }
        None
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load the test program:
        // 1. Move absolute immediate 666 to register 0
        // 2. Move register 0 to memory address 123
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(666)
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // 1. Store 666 to memory[124] (tag-aligned address)
        // 2. Store 124 to register[1] (pointer, tag bits = 0)
        // 3. Load from register[1] via DEREF to memory[200]
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(666)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(124),
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(124)
                .dst(Unit::UNIT_REGISTER)
                .di(1),
            instr()
                .src_deref(1, 0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(200),
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
        assert_eq!(helper.get_data_memory(200), 666);
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
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
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
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Load program for ALU addition: 666 + 111 = 777
        let program = vec![
            // Load 666 into ALU left input
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(666)
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

        // Open VCD trace file
        let mut vcd = tta.open_vcd("alu_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for up to 17 cycles (as in C++ test)
        let cycles_used = helper.run_for_cycles(&mut tta, 17);
        assert!(cycles_used <= 17, "Test used more than 17 cycles");

        vcd.dump(cycles_used as u64);

        // Verify the result: 666 + 111 = 777
        assert_eq!(helper.get_data_memory(123), 777);

        Ok(())
    }

    #[test]
    fn test_simple_register_move() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test simple register move followed by store to memory
        let program = vec![
            // Move immediate 42 to register 5
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(42)
                .dst(Unit::UNIT_REGISTER)
                .di(5),
            // Store register 5 to memory address 200
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(5)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(200),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("Register move instruction: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD trace file
        let mut vcd = tta.open_vcd("simple_register_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for enough cycles like the ALU test
        let cycles_used = helper.run_for_cycles(&mut tta, 17);
        println!("Register move used {} cycles", cycles_used);
        assert!(cycles_used <= 17, "Test used more than 17 cycles");

        vcd.dump(cycles_used as u64);

        // Verify the result: value should be stored at memory address 200
        let result = helper.get_data_memory(200);
        println!("Register move result: expected 42, got {}", result);
        assert_eq!(
            result, 42,
            "Register move should store 42 at memory address 200"
        );
        Ok(())
    }

    #[test]
    fn test_simple_alu_single() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test simple ALU operation in same pattern as stack test
        let program = vec![
            // Load immediate 42 to ALU left input
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(42)
                .dst(Unit::UNIT_ALU_LEFT)
                .di(0),
            // Read ALU left value to register 6
            instr()
                .src(Unit::UNIT_ALU_LEFT)
                .si(0)
                .dst(Unit::UNIT_REGISTER)
                .di(6),
            // Store register 6 to memory address 400 for verification
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(6)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(400),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("ALU single instruction: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD trace file
        let mut vcd = tta.open_vcd("simple_alu_single_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for enough cycles
        let cycles_used = helper.run_for_cycles(&mut tta, 20);
        println!("ALU single used {} cycles", cycles_used);
        assert!(cycles_used <= 20, "Test used more than 20 cycles");

        vcd.dump(cycles_used as u64);

        // Verify the result: value should be stored at memory address 400
        let result = helper.get_data_memory(400);
        println!("ALU single result: expected 42, got {}", result);
        assert_eq!(
            result, 42,
            "ALU single should store 42 at memory address 400"
        );
        Ok(())
    }

    #[test]
    fn test_manual_stack_lifo() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Manual LIFO test: push [0, 1], then pop and verify order
        let program = vec![
            // Push value 0 to stack 0
            instr().push_immediate(0, 0),
            // Push value 1 to stack 0
            instr().push_immediate(0, 1),
            // Pop first value (should be 1) to register 0
            instr().pop_to_reg(0, 0),
            // Store register 0 to memory 300 (first pop result)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(0)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(300),
            // Pop second value (should be 0) to register 1
            instr().pop_to_reg(0, 1),
            // Store register 1 to memory 301 (second pop result)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(1)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(301),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("Manual LIFO instruction: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // Run the test
        let cycles_used = helper.run_for_cycles(&mut tta, 50);
        println!("Manual LIFO test used {} cycles", cycles_used);

        // Check results
        let first_pop = helper.get_data_memory(300); // Should be 1 (last pushed)
        let second_pop = helper.get_data_memory(301); // Should be 0 (first pushed)

        println!("First pop result (should be 1): {}", first_pop);
        println!("Second pop result (should be 0): {}", second_pop);

        assert_eq!(first_pop, 1, "First pop should return last pushed value");
        assert_eq!(second_pop, 0, "Second pop should return first pushed value");

        Ok(())
    }

    #[test]
    fn test_reproduce_property_bug() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Reproduce exact property test pattern with values [0, 1]
        let values = vec![0u32, 1u32];
        let stack_id = 0u8;
        let mut program = Vec::new();

        // Push all values onto the stack (same as property test)
        for &value in &values {
            program.push(instr().push_immediate(stack_id, value));
        }

        // Pop all values from the stack and store to memory (same as property test)
        for (i, _) in values.iter().enumerate().rev() {
            let result_addr = 200 + i;
            println!("Pop iteration i={}, storing at address {}", i, result_addr);
            program.push(instr().pop_to_reg(stack_id, 0)); // Pop to register 0
            program.push(
                instr()
                    .src(Unit::UNIT_REGISTER)
                    .si(0)
                    .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                    .di(result_addr as u8),
            );
        }

        let mut machine_code = Vec::new();
        for instr in program {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 50);

        // Check what's actually stored
        println!("Memory at address 200: {}", helper.get_data_memory(200));
        println!("Memory at address 201: {}", helper.get_data_memory(201));

        // Verify LIFO using property test logic
        for (i, &expected_value) in values.iter().enumerate() {
            let result_addr = 200 + (values.len() - 1 - i); // Reverse order for LIFO
            let result = helper.get_data_memory(result_addr as u32);
            println!(
                "Position {}, expected value {}, address {}, got {}",
                i, expected_value, result_addr, result
            );
        }

        Ok(())
    }

    #[test]
    fn test_debug_stack_peek_issue() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Debug the failing case: values [1, 0, 0], peek_offset = 2
        let program = vec![
            // Push 1 (first, should be at bottom, stack position 0)
            instr().push_immediate(0, 1),
            // Push 0 (middle, stack position 1)
            instr().push_immediate(0, 0),
            // Push 0 (top, stack position 2)
            instr().push_immediate(0, 0),
            // Peek at offset 2 (should get value from bottom, which is 1)
            instr().stack_peek(0, 2, 5),
            // Store peek result to memory
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(5)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(400),
        ];

        let mut machine_code = Vec::new();
        for (i, instr) in program.iter().enumerate() {
            let assembled = instr.assemble();
            println!("Instruction {}: {:?}", i, assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD for debugging
        let mut vcd = tta.open_vcd("debug_stack_peek.vcd");

        helper.run_until_reset_released(&mut tta)?;
        let cycles_used = helper.run_for_cycles(&mut tta, 50);

        vcd.dump(cycles_used as u64);

        println!("Debug stack peek used {} cycles", cycles_used);

        // Check the result
        let peek_result = helper.get_data_memory(400);
        println!("Peek at offset 2: expected 1, got {}", peek_result);

        // The fact that peek works means the pushes worked
        // Let's just verify the peek result is correct
        assert_eq!(
            peek_result, 1,
            "Peek at offset 2 should return the first pushed value"
        );

        Ok(())
    }

    #[test]
    fn test_debug_stack_poke() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Simple poke test: push values, then verify basic stack functionality first
        let program = vec![
            // First, test that normal push/pop works
            instr().push_immediate(0, 42),
            instr().pop_to_reg(0, 10),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(10)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(500),
            // Then test register load
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(99)
                .dst(Unit::UNIT_REGISTER)
                .di(7),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(7)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(501),
            // Now test poke: push 0, load 1 into reg, poke
            instr().push_immediate(0, 0),
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(1)
                .dst(Unit::UNIT_REGISTER)
                .di(7),
            instr().stack_poke(0, 0, 7),
            instr().pop_to_reg(0, 11),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(11)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(600),
        ];

        let mut machine_code = Vec::new();
        for (i, instr) in program.iter().enumerate() {
            let assembled = instr.assemble();
            println!("Poke test instruction {}: {:?}", i, assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD for debugging
        let mut vcd = tta.open_vcd("debug_stack_poke.vcd");

        helper.run_until_reset_released(&mut tta)?;
        let cycles_used = helper.run_for_cycles(&mut tta, 50);

        vcd.dump(cycles_used as u64);

        println!("Simple poke test used {} cycles", cycles_used);

        // Check results
        let push_pop_result = helper.get_data_memory(500);
        let register_test = helper.get_data_memory(501);
        let poke_result = helper.get_data_memory(600);

        println!("Push/pop test: expected 42, got {}", push_pop_result);
        println!("Register test: expected 99, got {}", register_test);
        println!("Poke test: expected 1, got {}", poke_result);

        assert_eq!(push_pop_result, 42, "Basic push/pop should work");
        assert_eq!(register_test, 99, "Register load should work");
        assert_eq!(
            poke_result, 1,
            "Stack poke should change the value from 0 to 1"
        );

        Ok(())
    }

    #[test]
    fn test_stack_peek_only() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test stack peek operation
        let program = vec![
            instr().push_immediate(0, 99), // Push 99 to stack 0
            instr().push_immediate(0, 77), // Push 77 to stack 0 (now stack has [99, 77] with 77 on top)
            instr().stack_peek(0, 0, 6),   // Peek stack 0 offset 0 (should get 77) into reg 6
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(6)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(400), // Store reg 6 to mem 400
            instr().stack_peek(0, 1, 7),   // Peek stack 0 offset 1 (should get 99) into reg 7
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(7)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(401), // Store reg 7 to mem 401
        ];

        let mut machine_code = Vec::new();
        for instr in program.iter() {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);

        // Reset and run
        helper.reset(&mut tta);
        tta.rst_i = 0;

        let cycles = helper.run_for_cycles(&mut tta, 100);
        println!("Stack peek test used {} cycles", cycles);

        let peek_top = *helper.data_memory.get(&400).unwrap_or(&0); // Should be 77
        let peek_second = *helper.data_memory.get(&401).unwrap_or(&0); // Should be 99

        println!("Stack peek offset 0 (top): {} (expected 77)", peek_top);
        println!(
            "Stack peek offset 1 (second): {} (expected 99)",
            peek_second
        );

        assert_eq!(
            peek_top, 77,
            "Stack peek offset 0 should return 77 (top of stack)"
        );
        assert_eq!(
            peek_second, 99,
            "Stack peek offset 1 should return 99 (second on stack)"
        );
        Ok(())
    }

    #[test]
    fn test_minimal_poke() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test A: Basic push/peek without poke
        println!("=== Test A: Basic push/peek (no poke) ===");
        let program_a = vec![
            instr().push_immediate(0, 99), // Push 99 to stack 0
            instr().stack_peek(0, 0, 6),   // Peek stack 0 offset 0 into reg 6
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(6)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(300), // Store reg 6 to mem 300
        ];

        let mut machine_code = Vec::new();
        for instr in program_a.iter() {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);

        // Reset and run
        helper.reset(&mut tta);
        tta.rst_i = 0;

        helper.run_for_cycles(&mut tta, 50);
        let basic_result = *helper.data_memory.get(&300).unwrap_or(&0);
        println!("Basic push/peek result: {}", basic_result);

        // Reset for next test - create completely new instances
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the second TTA instance
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        println!("\n=== Test B: Push/poke/peek ===");

        // Test B: Push, poke, peek
        let program = vec![
            instr().push_immediate(0, 99), // Push 99 to stack 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(77)
                .dst(Unit::UNIT_REGISTER)
                .di(5), // Load 77 into reg 5
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(5)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(199), // Debug: store reg 5 to mem 199
            instr().stack_poke(0, 0, 5),   // Poke reg 5 (77) into stack 0 offset 0
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(5)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(198), // Debug: store reg 5 to mem 198 after poke
            instr().stack_peek(0, 0, 6),   // Peek stack 0 offset 0 into reg 6
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(6)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(200), // Store reg 6 to mem 200
        ];

        let mut machine_code = Vec::new();
        for instr in program.iter() {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);

        // Reset and run
        helper.reset(&mut tta);
        tta.rst_i = 0;

        let cycles = helper.run_for_cycles(&mut tta, 100);
        println!("Minimal poke test used {} cycles", cycles);

        // Debug: check what's in register 5 before and after poke
        let reg5_before = *helper.data_memory.get(&199).unwrap_or(&0);
        let reg5_after = *helper.data_memory.get(&198).unwrap_or(&0);
        let stack_result = *helper.data_memory.get(&200).unwrap_or(&0);

        println!("Reg 5 before poke: {}", reg5_before);
        println!("Reg 5 after poke: {}", reg5_after);
        println!("Stack peek result: {}", stack_result);

        // First check that basic stack works
        assert_eq!(basic_result, 99, "Basic push/peek should work");
        assert_eq!(reg5_before, 77, "Register 5 should contain 77 before poke");
        assert_eq!(
            stack_result, 77,
            "Poke should have changed the stack value from 99 to 77"
        );
        Ok(())
    }

    #[test]
    fn test_poke_vs_no_poke() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;

        println!("=== COMPARING POKE VS NO-POKE ===");

        // Test 1: Without poke - baseline
        println!("\n--- Test 1: Push then peek (no poke) ---");
        let mut tta1 = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper1 = TtaTestHelper::new();

        tta1.rst_i = 1;
        tta1.clk_i = 0;
        tta1.instr_ready_i = 1;
        tta1.data_ready_i = 1;
        tta1.instr_data_read_i = 0;
        tta1.data_data_read_i = 0;

        let no_poke_program = vec![
            instr().push_immediate(0, 555), // Push 555
            instr().stack_peek(0, 0, 8),    // Peek top (should be 555)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(8)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(800),
            instr().pop_to_reg(0, 9), // Pop (should be 555)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(9)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(801),
        ];

        let mut machine_code = Vec::new();
        for instr in no_poke_program {
            machine_code.extend(instr.assemble());
        }
        helper1.load_instructions(&machine_code, 0);
        let mut vcd1 = tta1.open_vcd("no_poke_test.vcd");
        helper1.run_until_reset_released(&mut tta1)?;
        helper1.run_for_cycles(&mut tta1, 30);
        vcd1.dump(30);

        let no_poke_peek = helper1.get_data_memory(800);
        let no_poke_pop = helper1.get_data_memory(801);
        println!(
            "No-poke: peek={} (exp 555), pop={} (exp 555)",
            no_poke_peek, no_poke_pop
        );

        // Test 2: With poke - should change the value
        println!("\n--- Test 2: Push, poke, then peek ---");
        let mut tta2 = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper2 = TtaTestHelper::new();

        tta2.rst_i = 1;
        tta2.clk_i = 0;
        tta2.instr_ready_i = 1;
        tta2.data_ready_i = 1;
        tta2.instr_data_read_i = 0;
        tta2.data_data_read_i = 0;

        let poke_program = vec![
            instr().push_immediate(0, 555), // Push 555 (original)
            // Load new value into register
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(777)
                .dst(Unit::UNIT_REGISTER)
                .di(15),
            // Poke the new value into stack top
            instr().stack_poke(0, 0, 15),
            // Now peek and pop to verify
            instr().stack_peek(0, 0, 8), // Should now be 777, not 555
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(8)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(802),
            instr().pop_to_reg(0, 9), // Should now be 777, not 555
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(9)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(803),
        ];

        machine_code.clear();
        for instr in poke_program {
            machine_code.extend(instr.assemble());
        }
        helper2.load_instructions(&machine_code, 0);
        let mut vcd2 = tta2.open_vcd("poke_test.vcd");
        helper2.run_until_reset_released(&mut tta2)?;
        helper2.run_for_cycles(&mut tta2, 50);
        vcd2.dump(50);

        let poke_peek = helper2.get_data_memory(802);
        let poke_pop = helper2.get_data_memory(803);
        println!(
            "With-poke: peek={} (exp 777), pop={} (exp 777)",
            poke_peek, poke_pop
        );

        // Analysis
        println!("\n--- ANALYSIS ---");
        if no_poke_peek == 555 && no_poke_pop == 555 {
            println!("✅ Baseline (no poke): Stack works correctly");
        } else {
            println!(
                "❌ Baseline (no poke): Stack broken - peek={}, pop={}",
                no_poke_peek, no_poke_pop
            );
        }

        if poke_peek == 777 && poke_pop == 777 {
            println!("✅ Poke test: Poke works correctly!");
        } else {
            println!(
                "❌ Poke test: Poke failed - peek={}, pop={}",
                poke_peek, poke_pop
            );
            println!("   This means poke operation is not writing to stack memory");
        }

        // Don't assert so we can see all results
        Ok(())
    }

    #[test]
    fn test_assembler_debug() {
        // Debug stack_poke assembler specifically
        let poke_instr = instr().stack_poke(0, 0, 15);
        let assembled = poke_instr.assemble();
        println!("stack_poke(0, 0, 15) assembled: {:?}", assembled);

        // Manually decode the first word
        let word = assembled[0];
        let src_unit = word & 0x1F;
        let dst_unit = (word >> 5) & 0x1F;
        let si = (word >> 10) & 0xFF;
        let di = (word >> 18) & 0xFF;

        println!(
            "  decoded src_unit: {} (should be 3 for UNIT_REGISTER)",
            src_unit
        );
        println!("  decoded si: {} (should be 15)", si);
        println!(
            "  decoded dst_unit: {} (should be 2 for UNIT_STACK_INDEX)",
            dst_unit
        );
        println!("  decoded di: {} (should be 0)", di);

        // Compare with manual instruction creation
        let manual = instr()
            .src(Unit::UNIT_REGISTER)
            .si(15)
            .dst(Unit::UNIT_STACK_INDEX)
            .di(0);
        let manual_assembled = manual.assemble();
        println!("Manual equivalent: {:?}", manual_assembled);

        // Also test the register load instruction
        let reg_instr = instr()
            .src(Unit::UNIT_ABS_OPERAND)
            .soperand(777)
            .dst(Unit::UNIT_REGISTER)
            .di(15);
        let reg_assembled = reg_instr.assemble();
        println!("Register load assembled: {:?}", reg_assembled);

        let reg_word = reg_assembled[0];
        let reg_si = (reg_word >> 10) & 0xFF;
        let reg_di = (reg_word >> 18) & 0xFF;
        println!("  reg si: {} (operand is in next word, not in si)", reg_si);
        println!("  reg di: {} (should be 15)", reg_di);
    }

    #[test]
    fn test_stack_push_then_peek() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test stack push then peek (indexed read) to verify data storage
        let program = vec![
            // Push value 99 to stack 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(99)
                .dst(Unit::UNIT_STACK_PUSH_POP)
                .di(0),
            // Peek at stack 0, offset 0 (top of stack) to register 7
            instr().stack_peek(0, 0, 7),
            // Store register 7 to memory address 350 for verification
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(7)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(350),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("Push-peek instruction: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD trace file
        // let mut vcd = tta.open_vcd("stack_push_peek_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for enough cycles
        let cycles_used = helper.run_for_cycles(&mut tta, 20);
        println!("Stack push/peek used {} cycles", cycles_used);
        assert!(cycles_used <= 20, "Test used more than 20 cycles");

        // vcd.dump(cycles_used as u64);

        // Verify the result: value should be stored at memory address 350
        let result = helper.get_data_memory(350);
        println!("Stack push/peek result: expected 99, got {}", result);
        assert_eq!(
            result, 99,
            "Stack push/peek should store 99 at memory address 350"
        );
        Ok(())
    }

    #[test]
    fn test_stack_single_push() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test single stack push followed by pop and verify
        let program = vec![
            // Push value 42 to stack 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(42)
                .dst(Unit::UNIT_STACK_PUSH_POP)
                .di(0),
            // Pop from stack 0 to register 6
            instr()
                .src(Unit::UNIT_STACK_PUSH_POP)
                .si(0)
                .dst(Unit::UNIT_REGISTER)
                .di(6),
            // Store register 6 to memory address 300 for verification
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(6)
                .dst(Unit::UNIT_MEMORY_OPERAND)
                .doperand(300),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("Single push instruction: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD trace file
        let mut vcd = tta.open_vcd("single_stack_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for enough cycles
        let cycles_used = helper.run_for_cycles(&mut tta, 20);
        println!("Stack push/pop used {} cycles", cycles_used);
        assert!(cycles_used <= 20, "Test used more than 20 cycles");

        vcd.dump(cycles_used as u64);

        // Verify the result: value should be stored at memory address 300
        let result = helper.get_data_memory(300);
        println!("Stack push/pop result: expected 42, got {}", result);
        assert_eq!(
            result, 42,
            "Stack push/pop should store 42 at memory address 300"
        );
        Ok(())
    }

    #[test]
    fn test_stack_push_pop() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        // Initialize the testbench
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Test complete stack push/pop cycle
        let program = vec![
            // Push value 42 to stack 0 (using ABS_IMMEDIATE since 42 fits in 12 bits)
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(42)
                .dst(Unit::UNIT_STACK_PUSH_POP)
                .di(0),
            // Pop from stack 0 to register 5
            instr()
                .src(Unit::UNIT_STACK_PUSH_POP)
                .si(0)
                .dst(Unit::UNIT_REGISTER)
                .di(5),
            // Store register 5 to memory address 100 for verification
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(5)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for instr in program {
            let assembled = instr.assemble();
            println!("Instruction assembled to: {:?}", assembled);
            machine_code.extend(assembled);
        }

        helper.load_instructions(&machine_code, 0);

        // Open VCD trace file
        let mut vcd = tta.open_vcd("stack_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;
        vcd.dump(0); // Dump after reset

        // Run for enough cycles to complete all operations
        let cycles_used = helper.run_for_cycles(&mut tta, 50);
        println!("Stack test used {} cycles", cycles_used);

        vcd.dump(cycles_used as u64); // Dump after execution

        // Verify the complete stack push/pop cycle worked
        let result = helper.get_data_memory(100);
        println!("Complete stack test result: expected 42, got {}", result);
        if result == 42 {
            println!("🎉 Stack push/pop cycle works perfectly!");
        } else {
            println!(
                "❌ Stack push/pop cycle failed - got {} instead of 42",
                result
            );
        }
        assert_eq!(
            result, 42,
            "Stack push/pop cycle should return the original value"
        );

        Ok(())
    }

    #[test]
    fn test_byte_read_via_register_pointer() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Pre-fill memory word at address 44 (tag-aligned) with 0x44332211
        helper.set_data_memory(44, 0x44332211);

        // Load address 44 into register 0, then read word via DEREF
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(44)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            // Read word from address in register 0 via DEREF
            instr()
                .src_deref(0, 0)
                .dst(Unit::UNIT_REGISTER)
                .di(1),
            // Store to memory for verification
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(1)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for i in program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let result = helper.get_data_memory(100);

        assert_eq!(
            result, 0x44332211,
            "Word read via DEREF from address 44 should return 0x44332211"
        );

        Ok(())
    }

    // --- Jump and conditional branch tests ---

    #[test]
    fn test_unconditional_jump() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Program: store 111, jump over the next instruction, store 333.
        // Address layout:
        //   0: store 111 to mem[100]
        //   1: jump to address 4 (skip the next instruction)    [2 words: opcode + operand]
        //   3: store 222 to mem[100]  (should be SKIPPED)
        //   4: store 333 to mem[101]
        let program = vec![
            // addr 0: store 111 to mem[100]
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(111)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 1-2: jump to address 4
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(4)
                .dst(Unit::UNIT_PC),
            // addr 3: store 222 to mem[100] (should be skipped)
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(222)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 4: store 333 to mem[101]
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(333)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(101),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let mem100 = helper.get_data_memory(100);
        let mem101 = helper.get_data_memory(101);

        assert_eq!(mem100, 111, "mem[100] should be 111 (not overwritten by skipped instruction)");
        assert_eq!(mem101, 333, "mem[101] should be 333 (landed after jump)");

        Ok(())
    }

    #[test]
    fn test_conditional_branch_taken() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Set condition to nonzero (1), then conditional jump should be taken.
        //   0: set cond = 1 (via abs_immediate 1 → UNIT_COND)
        //   1: conditional jump to addr 4
        //   3: store 222 to mem[100]  (should be SKIPPED)
        //   4: store 333 to mem[100]
        let program = vec![
            // addr 0: set condition register = 1
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(1)
                .dst(Unit::UNIT_COND),
            // addr 1-2: conditional jump to addr 4
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(4)
                .dst(Unit::UNIT_PC_COND),
            // addr 3: store 222 (should be skipped)
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(222)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 4: store 333
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(333)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let result = helper.get_data_memory(100);
        assert_eq!(result, 333, "Conditional branch should be taken (cond=1), landing at addr 4");

        Ok(())
    }

    #[test]
    fn test_conditional_branch_not_taken() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Condition is zero, so conditional jump should NOT be taken.
        //   0: set cond = 0
        //   1: conditional jump to addr 4 (should fall through)
        //   3: store 222 to mem[100]  (should execute)
        //   4: store 333 to mem[101]
        let program = vec![
            // addr 0: set condition register = 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(0)
                .dst(Unit::UNIT_COND),
            // addr 1-2: conditional jump to addr 4 (not taken)
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(4)
                .dst(Unit::UNIT_PC_COND),
            // addr 3: store 222 (should execute — branch not taken)
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(222)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 4: store 333
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(333)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(101),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let mem100 = helper.get_data_memory(100);
        let mem101 = helper.get_data_memory(101);

        assert_eq!(mem100, 222, "Branch not taken — addr 3 should execute, storing 222");
        assert_eq!(mem101, 333, "Execution should continue to addr 4");

        Ok(())
    }

    #[test]
    fn test_compare_and_branch() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Compare 42 > 10, branch if true.
        //   0: 42 → alu[0].left
        //   1: 10 → alu[0].right
        //   2: GT → alu[0].operator
        //   3: alu[0].result → UNIT_COND
        //   4-5: conditional jump to addr 7
        //   6: store 999 to mem[100]   (should be SKIPPED since 42 > 10)
        //   7: store 123 to mem[100]
        let program = vec![
            // addr 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(42)
                .dst(Unit::UNIT_ALU_LEFT)
                .di(0),
            // addr 1
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(10)
                .dst(Unit::UNIT_ALU_RIGHT)
                .di(0),
            // addr 2
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(14) // ALU_GT = 0x00e = 14
                .dst(Unit::UNIT_ALU_OPERATOR)
                .di(0),
            // addr 3: read ALU result (1 = true) → condition register
            instr()
                .src(Unit::UNIT_ALU_RESULT)
                .si(0)
                .dst(Unit::UNIT_COND),
            // addr 4-5: conditional jump to addr 7
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(7)
                .dst(Unit::UNIT_PC_COND),
            // addr 6: store 999 (should be skipped)
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(999)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 7: store 123
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(123)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let result = helper.get_data_memory(100);
        assert_eq!(result, 123, "42 > 10 is true, branch should be taken, storing 123 not 999");

        Ok(())
    }

    // --- Tagged register tests ---

    #[test]
    fn test_reg_tag_read_write() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Store a tagged value 0xDEADBEE1 into register 0 (tag = 1, value = 0xDEADBEE0)
        // Then read tag and value separately.
        let program = vec![
            // addr 0-1: load tagged value into r0 (RAW mode)
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(0xDEADBEE1)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            // addr 2: read TAG of r0 → mem[100]
            instr()
                .src_reg_tag(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // addr 3: read VALUE of r0 → mem[101]
            instr()
                .src_reg_value(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(101),
            // addr 4: read RAW of r0 → mem[102]
            instr()
                .src_reg(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(102),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let tag = helper.get_data_memory(100);
        let value = helper.get_data_memory(101);
        let raw = helper.get_data_memory(102);

        assert_eq!(tag, 1, "Tag of 0xDEADBEE1 should be 1 (low 2 bits)");
        assert_eq!(value, 0xDEADBEE0, "Value should be 0xDEADBEE0 (tag bits zeroed)");
        assert_eq!(raw, 0xDEADBEE1, "Raw should be the full tagged word");

        Ok(())
    }

    #[test]
    fn test_reg_tag_write_preserves_payload() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Store 0xCAFE0000 into r0 (tag = 0).
        // Then write tag = 2 via TAG mode (should preserve payload).
        // Then read raw to verify.
        let program = vec![
            // Load 0xCAFE0000 into r0
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(0xCAFE0000)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            // Write tag = 2 to r0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(2)
                .dst_reg_tag(0),
            // Read raw r0 → mem[100]
            instr()
                .src_reg(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let result = helper.get_data_memory(100);
        assert_eq!(result, 0xCAFE0002, "Payload should be preserved, tag should be 2");

        Ok(())
    }

    #[test]
    fn test_reg_value_write_preserves_tag() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Store 0x00000003 into r0 (tag = 3, payload = 0).
        // Then write value 0xBEEF0000 via VALUE mode (should preserve tag).
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(3)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(0xBEEF0000)
                .dst_reg_value(0),
            instr()
                .src_reg(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let result = helper.get_data_memory(100);
        assert_eq!(result, 0xBEEF0003, "Tag 3 should be preserved, payload updated");

        Ok(())
    }

    #[test]
    fn test_reg_deref_car_cdr() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Simulate a cons cell at word address 20:
        //   mem[20] = 0x0000002A (car = 42)
        //   mem[21] = 0x00000063 (cdr = 99)
        helper.set_data_memory(20, 42);
        helper.set_data_memory(21, 99);

        // Load tagged cons pointer into r0: address 20 | tag 1 = 0x00000051
        // (20 << 0 is 20, but with 2-bit tags, address 20 must be tag-aligned:
        //  20 = 0x14, low 2 bits = 0, so tagged = 0x14 | 1 = 0x15 = 21... no)
        //
        // Wait — the address IS the word address, and the tag lives in the low
        // bits. So the tagged pointer is (word_address | tag). For word address
        // 20 = 0x14, the low 2 bits are 0, so 0x14 | 1 = 0x15. Stripping the
        // tag: 0x15 & ~3 = 0x14 = 20. Correct.
        let cons_ptr = 20u32 | 1; // word address 20, tag 1 (cons)

        let program = vec![
            // Load tagged pointer into r0
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(cons_ptr)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            // DEREF r0 offset 0 (car) → mem[100]
            instr()
                .src_deref(0, 0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(100),
            // DEREF r0 offset 1 (cdr) → mem[101]
            instr()
                .src_deref(0, 1)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(101),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let car = helper.get_data_memory(100);
        let cdr = helper.get_data_memory(101);

        assert_eq!(car, 42, "car of cons cell at addr 20 should be 42");
        assert_eq!(cdr, 99, "cdr of cons cell at addr 20 should be 99");

        Ok(())
    }

    #[test]
    fn test_reg_deref_write() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Write car and cdr of a cons cell via DEREF destination.
        let cons_ptr = 24u32 | 1; // word address 24, tag 1

        let program = vec![
            // Load tagged pointer into r0
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(cons_ptr)
                .dst(Unit::UNIT_REGISTER)
                .di(0),
            // Write 777 to car (deref r0 + 0)
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(777)
                .dst_deref(0, 0),
            // Write 888 to cdr (deref r0 + 1)
            instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(888)
                .dst_deref(0, 1),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        let car = helper.get_data_memory(24);
        let cdr = helper.get_data_memory(25);

        assert_eq!(car, 777, "DEREF write at offset 0 should store 777 at word addr 24");
        assert_eq!(cdr, 888, "DEREF write at offset 1 should store 888 at word addr 25");

        Ok(())
    }

    // --- Cycle-precise prefetch tests ---

    #[test]
    fn test_prefetch_hides_fetch_latency() -> Result<(), Box<dyn std::error::Error>> {
        // Two back-to-back 1-word register moves. The second instruction
        // should be prefetched while the first executes, so the second
        // done should arrive faster than the first (no fetch stall).
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let program = vec![
            // addr 0: imm 42 → reg 0
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(42)
                .dst(Unit::UNIT_REGISTER).di(0),
            // addr 1: imm 99 → reg 1
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(99)
                .dst(Unit::UNIT_REGISTER).di(1),
            // addr 2: reg 0 → mem[100]
            instr()
                .src(Unit::UNIT_REGISTER).si(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // First instruction: cold start, no prefetch benefit.
        let cycles_1 = helper.run_until_done(&mut tta, 50)
            .expect("First instruction should complete");
        // Second instruction: should have been prefetched during first.
        let cycles_2 = helper.run_until_done(&mut tta, 50)
            .expect("Second instruction should complete");

        // The second instruction should complete in fewer cycles than
        // the first, because the fetch was overlapped with execute.
        // (We don't assert exact counts since they depend on bus timing,
        // but the second should not be slower than the first.)
        assert!(cycles_2 <= cycles_1,
            "Prefetched instruction should not take more cycles than the first \
             (first={}, second={})", cycles_1, cycles_2);

        // Verify correctness
        helper.run_for_cycles(&mut tta, 50);
        assert_eq!(helper.get_data_memory(100), 42);

        Ok(())
    }

    #[test]
    fn test_prefetch_survives_multicycle_execute() -> Result<(), Box<dyn std::error::Error>> {
        // First instruction is a memory load (multi-cycle execute).
        // Second instruction should be prefetched and waiting in the
        // sequencer's buffer while execute waits for data_bus.ready.
        // After the load completes, the second instruction should
        // execute without an additional fetch stall.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        helper.set_data_memory(50, 0xCAFE);

        let program = vec![
            // addr 0: mem[50] → reg[0] (multi-cycle: data bus read)
            instr()
                .src(Unit::UNIT_MEMORY_IMMEDIATE).si(50)
                .dst(Unit::UNIT_REGISTER).di(0),
            // addr 1: imm 0x42 → reg[1] (single-cycle, should be prefetched)
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(0x42)
                .dst(Unit::UNIT_REGISTER).di(1),
            // addr 2: reg[0] → mem[200]
            instr()
                .src(Unit::UNIT_REGISTER).si(0)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(200),
            // addr 3: reg[1] → mem[201]
            instr()
                .src(Unit::UNIT_REGISTER).si(1)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(201),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        // First instruction (memory load) — multi-cycle.
        let cycles_load = helper.run_until_done(&mut tta, 50)
            .expect("Memory load should complete");
        // Second instruction (immediate → register) — should be prefetched.
        let cycles_imm = helper.run_until_done(&mut tta, 50)
            .expect("Immediate store should complete");

        // The immediate-to-register move after a multi-cycle load should
        // be fast (prefetched), not slower than the load itself.
        assert!(cycles_imm <= cycles_load,
            "Prefetched instruction after multi-cycle load should be fast \
             (load={} cycles, imm={} cycles)", cycles_load, cycles_imm);

        // Run remaining instructions and verify.
        helper.run_for_cycles(&mut tta, 100);
        assert_eq!(helper.get_data_memory(200), 0xCAFE, "Memory load result");
        assert_eq!(helper.get_data_memory(201), 0x42, "Prefetched immediate result");

        Ok(())
    }

    #[test]
    fn test_branch_flush_clears_prefetch() -> Result<(), Box<dyn std::error::Error>> {
        // An unconditional jump should flush any prefetched instruction.
        // The instruction immediately after the jump (which may have been
        // prefetched) must NOT execute.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // The jump is a 2-word instruction. While execute processes it,
        // the sequencer may prefetch the wrong-path instruction.
        // The branch must discard that prefetch.
        //
        // addr 0-1: jump to addr 5
        // addr 2-4: store 0xBAD → mem[200] (wrong path, 3-word)
        // addr 5-6: store 0x600D → mem[100] (branch target, 2-word)
        let program = vec![
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(5)
                .dst(Unit::UNIT_PC),
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(0xBAD)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(200),
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(0x600D)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 100);

        assert_eq!(helper.get_data_memory(100), 0x600D,
            "Branch target should execute");
        assert_eq!(helper.get_data_memory(200), 0,
            "Wrong-path instruction must not execute (prefetch should be flushed)");

        Ok(())
    }

    // --- Instruction queue tests ---

    #[test]
    fn test_queue_fills_across_consecutive_1word() -> Result<(), Box<dyn std::error::Error>> {
        // Four consecutive 1-word instructions. The 2-entry queue should
        // allow the first two to be fetched while none have executed yet.
        // All four should produce correct results.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let program = vec![
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(11).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(22).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(33).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(102),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(44).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(103),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        assert_eq!(helper.get_data_memory(100), 11);
        assert_eq!(helper.get_data_memory(101), 22);
        assert_eq!(helper.get_data_memory(102), 33);
        assert_eq!(helper.get_data_memory(103), 44);

        Ok(())
    }

    #[test]
    fn test_queue_mixed_instruction_widths() -> Result<(), Box<dyn std::error::Error>> {
        // Mix of 1-word, 2-word, and 3-word instructions in sequence.
        // Tests that the queue correctly handles variable-width entries.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        helper.set_data_memory(50, 0xAAAA);

        let program = vec![
            // 1-word: imm → reg
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0x11)
                   .dst(Unit::UNIT_REGISTER).di(0),
            // 2-word: abs_operand → reg (2 words: opcode + operand)
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0x2222)
                   .dst(Unit::UNIT_REGISTER).di(1),
            // 1-word: reg → mem
            instr().src(Unit::UNIT_REGISTER).si(0)
                   .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            // 1-word: reg → mem
            instr().src(Unit::UNIT_REGISTER).si(1)
                   .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
            // 2-word: mem_operand → reg (source operand)
            instr().src_mem_op(50)
                   .dst(Unit::UNIT_REGISTER).di(2),
            // 1-word: reg → mem
            instr().src(Unit::UNIT_REGISTER).si(2)
                   .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(102),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 0x11, "1-word imm result");
        assert_eq!(helper.get_data_memory(101), 0x2222, "2-word operand result");
        assert_eq!(helper.get_data_memory(102), 0xAAAA, "mem_operand load result");

        Ok(())
    }

    #[test]
    fn test_queue_branch_flush_clears_both_entries() -> Result<(), Box<dyn std::error::Error>> {
        // The queue may have 2 entries filled when a branch executes.
        // Both must be flushed. Skipped instructions write to distinct
        // addresses so transient execution is observable.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0-1: jump to addr 6 (2-word)
        // addr 2:   store 0xBAD1 → mem[400] (wrong path, possibly queued)
        // addr 3:   store 0xBAD2 → mem[401] (wrong path, possibly queued)
        // addr 4:   store 0xBAD3 → mem[402] (wrong path)
        // addr 5:   nop padding
        // addr 6:   store 0xOK → mem[100]
        let program = vec![
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(6).dst(Unit::UNIT_PC),
            // These 1-word instructions may be fetched into both queue slots
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBA1).dst(Unit::UNIT_MEMORY_OPERAND).doperand(400),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBA2).dst(Unit::UNIT_MEMORY_OPERAND).doperand(401),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBA3).dst(Unit::UNIT_MEMORY_OPERAND).doperand(402),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_REGISTER).di(0),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0x999).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        assert_eq!(helper.get_data_memory(100), 0x999, "Branch target should execute");
        assert_eq!(helper.get_data_memory(400), 0, "Wrong-path entry 1 must not execute");
        assert_eq!(helper.get_data_memory(401), 0, "Wrong-path entry 2 must not execute");
        assert_eq!(helper.get_data_memory(402), 0, "Wrong-path entry 3 must not execute");

        Ok(())
    }

    #[test]
    fn test_queue_pc_correctness() -> Result<(), Box<dyn std::error::Error>> {
        // Verify UNIT_PC returns the correct value for each instruction
        // as it flows through the queue. Mixed 1-word and 2-word instructions.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0:   PC → mem[100]  (1-word, PC should be 1)
        // addr 1-2: abs_operand(0x42) → reg[0]  (2-word, PC should be 3)
        // addr 3:   PC → mem[101]  (1-word, PC should be 4)
        // addr 4:   PC → mem[102]  (1-word, PC should be 5)
        let program = vec![
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0x42).dst(Unit::UNIT_REGISTER).di(0),
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(102),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        assert_eq!(helper.get_data_memory(100), 1, "PC at addr 0 (1-word) should be 1");
        assert_eq!(helper.get_data_memory(101), 4, "PC at addr 3 (after 2-word at addr 1) should be 4");
        assert_eq!(helper.get_data_memory(102), 5, "PC at addr 4 should be 5");

        Ok(())
    }

    // --- Fetch policy tests (no wrong-path fetch past branches) ---

    #[test]
    fn test_cond_branch_not_taken_queue_partial() -> Result<(), Box<dyn std::error::Error>> {
        // Conditional branch (not taken) with instructions before and after.
        // The fetch FSM should stall at the branch, NOT prefetch past it.
        // After the branch resolves as not-taken, sequential fetch resumes.
        // The instruction after the branch writes to a distinct address —
        // if it had been wrongly prefetched ahead of the branch, we'd see
        // timing anomalies, but correctness is the main check here.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0: store 0x11 → mem[100] (queued before branch)
        // addr 1: set cond = 0
        // addr 2-3: branch to addr 6 (NOT taken, cond=0)
        // addr 4: store 0x22 → mem[101] (should execute, but NOT prefetched past branch)
        // addr 5: store 0x33 → mem[102]
        let program = vec![
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0x11).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_COND),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(6).dst(Unit::UNIT_PC_COND),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0x22).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0x33).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(102),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 0x11);
        assert_eq!(helper.get_data_memory(101), 0x22, "Not-taken branch: fall-through should execute");
        assert_eq!(helper.get_data_memory(102), 0x33);

        Ok(())
    }

    #[test]
    fn test_cond_branch_taken_with_queued_fallthrough() -> Result<(), Box<dyn std::error::Error>> {
        // A 1-word instruction is queued, then a conditional branch (taken).
        // The fetch policy should NOT have fetched past the branch into
        // the fall-through path. The fall-through instruction writes to
        // a distinct sink to detect wrong-path execution.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0: set cond = 1
        // addr 1-2: branch to addr 5 (TAKEN)
        // addr 3: store 0xBAD → mem[400] (wrong path — must NOT execute)
        // addr 4: nop
        // addr 5: store 0xOK → mem[100]
        let program = vec![
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(1).dst(Unit::UNIT_COND),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(5).dst(Unit::UNIT_PC_COND),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBAD).dst(Unit::UNIT_MEMORY_OPERAND).doperand(400),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_REGISTER).di(0),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0x999).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 0x999, "Branch target should execute");
        assert_eq!(helper.get_data_memory(400), 0,
            "Fall-through must never execute — fetch policy should prevent prefetching past branch");

        Ok(())
    }

    #[test]
    fn test_back_to_back_control_flow() -> Result<(), Box<dyn std::error::Error>> {
        // Two consecutive branches. The fetch stall policy should handle
        // each one independently — stall after the first, resume after
        // it resolves, stall after the second.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0-1:  jump to addr 6 (first branch, 2 words)
        // addr 2-4:  store 0xBA1 → mem[200] (skipped, 3 words)
        // addr 5:    nop (1 word)
        // addr 6-7:  jump to addr 12 (second branch, 2 words)
        // addr 8-10: store 0xBA2 → mem[201] (skipped, 3 words)
        // addr 11:   nop (1 word)
        // addr 12-13: store 0xACE → mem[100] (2 words)
        // addr 14:   PC → mem[101] (1 word, PC = 15)
        let program = vec![
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(6).dst(Unit::UNIT_PC),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBA1).dst(Unit::UNIT_MEMORY_OPERAND).doperand(200),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_REGISTER).di(0),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(12).dst(Unit::UNIT_PC),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xBA2).dst(Unit::UNIT_MEMORY_OPERAND).doperand(201),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_REGISTER).di(0),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0xACE).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 0xACE);
        assert_eq!(helper.get_data_memory(101), 15, "PC at addr 14 should be 15");
        assert_eq!(helper.get_data_memory(200), 0, "First branch wrong-path must not execute");
        assert_eq!(helper.get_data_memory(201), 0, "Second branch wrong-path must not execute");

        Ok(())
    }

    #[test]
    fn test_pc_exact_around_branch_with_queue() -> Result<(), Box<dyn std::error::Error>> {
        // Verify exact UNIT_PC values when a branch sits in the queue
        // alongside non-branch instructions.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // addr 0: PC → mem[100]        (PC = 1)
        // addr 1: set cond = 0
        // addr 2-3: pc_cond to addr 6  (not taken, 2-word, PC = 4)
        // addr 4: PC → mem[101]        (PC = 5)
        // addr 5: PC → mem[102]        (PC = 6)
        let program = vec![
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(0).dst(Unit::UNIT_COND),
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(6).dst(Unit::UNIT_PC_COND),
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(101),
            instr().src(Unit::UNIT_PC).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(102),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 1, "PC at addr 0 should be 1");
        assert_eq!(helper.get_data_memory(101), 5, "PC at addr 4 (after not-taken branch) should be 5");
        assert_eq!(helper.get_data_memory(102), 6, "PC at addr 5 should be 6");

        Ok(())
    }

    // --- Cycle measurement (prints to stdout with --nocapture) ---

    #[test]
    fn test_measure_cycle_counts() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        helper.set_data_memory(50, 0xCAFE);

        // Sequence of different instruction types, measuring each.
        let program = vec![
            // 0: imm → reg (1-word, fused)
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(42).dst(Unit::UNIT_REGISTER).di(0),
            // 1: reg → reg (1-word, fused)
            instr().src(Unit::UNIT_REGISTER).si(0).dst(Unit::UNIT_REGISTER).di(1),
            // 2: imm → ALU left (1-word, fused)
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(10).dst(Unit::UNIT_ALU_LEFT).di(0),
            // 3: imm → ALU right (1-word, fused)
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(20).dst(Unit::UNIT_ALU_RIGHT).di(0),
            // 4: imm → ALU op (1-word, fused)
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(1).dst(Unit::UNIT_ALU_OPERATOR).di(0),
            // 5: ALU result → reg (1-word, fused)
            instr().src(Unit::UNIT_ALU_RESULT).si(0).dst(Unit::UNIT_REGISTER).di(2),
            // 6: reg → mem_imm (1-word, fused — fire-and-forget write)
            instr().src(Unit::UNIT_REGISTER).si(0).dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            // 7: mem_imm → reg (1-word, multi-cycle — bus read)
            instr().src(Unit::UNIT_MEMORY_IMMEDIATE).si(50).dst(Unit::UNIT_REGISTER).di(3),
            // 8-9: abs_operand → reg (2-word, fused)
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(0x1234).dst(Unit::UNIT_REGISTER).di(4),
            // 10-11: push immediate (2-word, stack dst)
            instr().push_immediate(0, 0xBEEF),
            // 12: pop → reg (1-word, stack src)
            instr().pop_to_reg(0, 5),
            // 13: imm → cond (1-word, fused)
            instr().src(Unit::UNIT_ABS_IMMEDIATE).si(1).dst(Unit::UNIT_COND),
            // 14-15: conditional branch not-taken (2-word)
            instr().src(Unit::UNIT_ABS_OPERAND).soperand(99).dst(Unit::UNIT_PC_COND),
            // 16: reg TAG → reg (1-word, fused)
            instr().src_reg_tag(0).dst(Unit::UNIT_REGISTER).di(6),
            // 17: reg DEREF → reg (1-word, multi-cycle — bus read via tagged ptr)
            // (need a valid tagged pointer first — use reg 0 which has 42)
            // Actually 42 has tag=2, payload=40. mem[40] might be 0. That's fine for timing.
            instr().src_deref(0, 0).dst(Unit::UNIT_REGISTER).di(7),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;

        let labels = [
            "imm→reg (cold start)",
            "reg→reg (queued)",
            "imm→alu_left",
            "imm→alu_right",
            "imm→alu_op",
            "alu_result→reg",
            "reg→mem_imm (write)",
            "mem_imm→reg (read)",
            "abs_operand→reg (2-word)",
            "push_immediate (2-word)",
            "pop→reg (stack src)",
            "imm→cond",
            "pc_cond not-taken (2-word)",
            "reg[TAG]→reg",
            "reg[DEREF]→reg (bus read)",
        ];

        println!("\n=== Cycle counts per instruction ===");
        for label in &labels {
            let cycles = helper.run_until_done(&mut tta, 100)
                .unwrap_or(0);
            println!("  {:>4} cycles  {}", cycles, label);
        }
        println!();

        Ok(())
    }

    // --- Write barrier tests ---

    #[test]
    fn test_write_barrier_push_pop() -> Result<(), Box<dyn std::error::Error>> {
        // Push three addresses to the write barrier, then pop them back
        // and verify FIFO order.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let program = vec![
            // Push three addresses to barrier
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(100)
                .dst(Unit::UNIT_WRITE_BARRIER),
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(200)
                .dst(Unit::UNIT_WRITE_BARRIER),
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(300)
                .dst(Unit::UNIT_WRITE_BARRIER),
            // Pop them back to memory for verification (FIFO order)
            instr()
                .src(Unit::UNIT_WRITE_BARRIER)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(400),
            instr()
                .src(Unit::UNIT_WRITE_BARRIER)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(401),
            instr()
                .src(Unit::UNIT_WRITE_BARRIER)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(402),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(400), 100, "First barrier entry should be 100");
        assert_eq!(helper.get_data_memory(401), 200, "Second barrier entry should be 200");
        assert_eq!(helper.get_data_memory(402), 300, "Third barrier entry should be 300");

        Ok(())
    }

    #[test]
    fn test_write_barrier_gc_pattern() -> Result<(), Box<dyn std::error::Error>> {
        // Simulate a GC write barrier pattern:
        // 1. Store a pointer to memory (normal store)
        // 2. Log the address to the barrier
        // 3. Later, drain the barrier to find dirty addresses
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let program = vec![
            // Mutator: write pointer 0xCAFE to mem[100]
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(0xCAFE)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(100),
            // Mutator: log address 100 to write barrier
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(100)
                .dst(Unit::UNIT_WRITE_BARRIER),
            // Mutator: write pointer 0xBEEF to mem[200]
            instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(0xBEEF)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(200),
            // Mutator: log address 200 to write barrier
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(200)
                .dst(Unit::UNIT_WRITE_BARRIER),
            // GC: drain barrier — pop first dirty address to reg[0]
            instr()
                .src(Unit::UNIT_WRITE_BARRIER)
                .dst(Unit::UNIT_REGISTER).di(0),
            // GC: store dirty address to mem[500] for verification
            instr()
                .src(Unit::UNIT_REGISTER).si(0)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(500),
            // GC: pop second dirty address
            instr()
                .src(Unit::UNIT_WRITE_BARRIER)
                .dst(Unit::UNIT_REGISTER).di(1),
            instr()
                .src(Unit::UNIT_REGISTER).si(1)
                .dst(Unit::UNIT_MEMORY_OPERAND).doperand(501),
        ];

        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 400);

        // Verify the stores happened
        assert_eq!(helper.get_data_memory(100), 0xCAFE);
        assert_eq!(helper.get_data_memory(200), 0xBEEF);
        // Verify the barrier logged the correct addresses
        assert_eq!(helper.get_data_memory(500), 100, "First dirty address should be 100");
        assert_eq!(helper.get_data_memory(501), 200, "Second dirty address should be 200");

        Ok(())
    }

    // --- Dataflow compiler integration tests ---

    #[test]
    fn test_dataflow_add_constants() -> Result<(), Box<dyn std::error::Error>> {
        // Use the dataflow graph to compile 42 + 10, store to mem[100].
        // Then run through the hardware and verify.
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let mut g = Graph::new();
        let a = g.constant(42);
        let b = g.constant(10);
        let sum = g.add(a, b);
        g.store_mem(100, sum);

        let program = g.compile();
        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 200);

        assert_eq!(helper.get_data_memory(100), 52, "42 + 10 = 52");
        Ok(())
    }

    #[test]
    fn test_dataflow_chained_computation() -> Result<(), Box<dyn std::error::Error>> {
        // (10 + 20) * 5 = 150
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        let mut g = Graph::new();
        let a = g.constant(10);
        let b = g.constant(20);
        let sum = g.add(a, b);
        let c = g.constant(5);
        let prod = g.mul(sum, c);
        g.store_mem(100, prod);

        let program = g.compile();
        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 150, "(10+20)*5 = 150");
        Ok(())
    }

    #[test]
    fn test_dataflow_compare_and_branch() -> Result<(), Box<dyn std::error::Error>> {
        // if 42 > 10: store 0x999 to mem[100], skip the 0xBAD store
        let runtime = create_runtime()?;
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .map_err(|e| format!("Failed to create model: {:?}", e))?;
        let mut helper = TtaTestHelper::new();

        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;

        // Build with dataflow graph + labels — no manual word counting.
        let mut g = Graph::new();
        let skip = g.label();

        let a = g.constant(42);
        let b = g.constant(10);
        let cmp = g.gt(a, b);
        g.set_cond(cmp);
        g.branch_cond_label(skip);

        // Else path (skipped when 42 > 10):
        let bad = g.constant(0xBAD);
        g.store_mem(400, bad); // distinct sink address

        // Then path:
        g.place_label(skip);
        let good = g.constant(0x999);
        g.store_mem(100, good);

        let program = g.compile();
        let mut machine_code = Vec::new();
        for i in &program {
            machine_code.extend(i.assemble());
        }
        helper.load_instructions(&machine_code, 0);
        helper.run_until_reset_released(&mut tta)?;
        helper.run_for_cycles(&mut tta, 300);

        assert_eq!(helper.get_data_memory(100), 0x999,
            "42 > 10 is true, should branch to then-path");
        assert_eq!(helper.get_data_memory(400), 0,
            "Else path should be skipped");
        Ok(())
    }
}
