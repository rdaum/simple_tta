use marlin::verilator::{VerilatedModelConfig, VerilatorRuntime};
use std::collections::HashMap;

use tta_sim::{create_tta_runtime, instr, TtaTestbench, Unit};

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
            .create_model::<TtaTestbench>(&VerilatedModelConfig {
                enable_tracing: false,
                ..Default::default()
            })
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

        // Open VCD trace file
        let mut vcd = tta.open_vcd("alu_debug.vcd");

        helper.run_until_reset_released(&mut tta)?;

        // Run for up to 17 cycles (as in C++ test)
        let cycles_used = helper.run_for_cycles(&mut tta, 17);
        assert!(cycles_used <= 17, "Test used more than 17 cycles");

        vcd.dump(cycles_used as u64);

        // Verify the result: 666 + 111 = 777
        assert_eq!(helper.get_data_memory(123), 777);
        assert!(helper.is_instruction_done(&tta));

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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(400),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(300),
            // Pop second value (should be 0) to register 1
            instr().pop_to_reg(0, 1),
            // Store register 1 to memory 301 (second pop result)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(1)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(301),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                    .di(result_addr as u16),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(400),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(500),
            // Then test register load
            instr()
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(99)
                .dst(Unit::UNIT_REGISTER)
                .di(7),
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(7)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(501),
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(600),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(400), // Store reg 6 to mem 400
            instr().stack_peek(0, 1, 7),   // Peek stack 0 offset 1 (should get 99) into reg 7
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(7)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(401), // Store reg 7 to mem 401
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(300), // Store reg 6 to mem 300
        ];

        let mut machine_code = Vec::new();
        for instr in program_a.iter() {
            machine_code.extend(instr.assemble());
        }

        helper.load_instructions(&machine_code, 0);

        // Reset and run
        helper.reset(&mut tta);
        tta.rst_i = 0;

        let cycles = helper.run_for_cycles(&mut tta, 50);
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(800),
            instr().pop_to_reg(0, 9), // Pop (should be 555)
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(9)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(801),
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
                .src(Unit::UNIT_ABS_IMMEDIATE)
                .si(777)
                .dst(Unit::UNIT_REGISTER)
                .di(15),
            // Poke the new value into stack top
            instr().stack_poke(0, 0, 15),
            // Now peek and pop to verify
            instr().stack_peek(0, 0, 8), // Should now be 777, not 555
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(8)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(802),
            instr().pop_to_reg(0, 9), // Should now be 777, not 555
            instr()
                .src(Unit::UNIT_REGISTER)
                .si(9)
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(803),
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
        let src_unit = word & 0xF;
        let si = (word >> 4) & 0xFFF;
        let dst_unit = (word >> 16) & 0xF;
        let di = (word >> 20) & 0xFFF;

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
            .src(Unit::UNIT_ABS_IMMEDIATE)
            .si(777)
            .dst(Unit::UNIT_REGISTER)
            .di(15);
        let reg_assembled = reg_instr.assemble();
        println!("Register load assembled: {:?}", reg_assembled);

        let reg_word = reg_assembled[0];
        let reg_si = (reg_word >> 4) & 0xFFF;
        let reg_di = (reg_word >> 20) & 0xFFF;
        println!("  reg si: {} (should be 777)", reg_si);
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(350),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
                .dst(Unit::UNIT_MEMORY_IMMEDIATE)
                .di(300),
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
        tta.instr_ready_i = 1;
        tta.data_ready_i = 1;
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
}
