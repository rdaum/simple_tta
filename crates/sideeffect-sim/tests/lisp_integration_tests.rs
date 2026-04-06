use std::collections::HashMap;

use sideeffect_sim::{create_tta_runtime, instr, TtaTestbench, Unit};

/// Address where the test harness reads the result.
const RESULT_ADDR: u8 = 200;

/// Compile a Lisp expression and append a "store ACC to mem[RESULT_ADDR]"
/// instruction so we can observe the result via data memory.
fn compile_with_result_store(source: &str) -> Vec<u32> {
    let prog = sideeffect_lisp::compile(source).expect("compile failed");
    let mut code = prog.code;
    // The compiler emits a halt (none→none) at the end. Replace it
    // with: reg[1] → mem_imm[RESULT_ADDR], then halt.
    // Remove the trailing halt
    code.pop();
    // Store ACC (r1) to data memory
    let store = instr()
        .src(Unit::UNIT_REGISTER).si(1) // REG_ACC = r1
        .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(RESULT_ADDR);
    code.extend(store.assemble());
    // Re-add halt
    code.extend(instr().assemble());
    code
}

/// Test helper — same as in tta_integration_tests.
struct TtaTestHelper {
    cycle_count: u32,
    instruction_memory: HashMap<u32, u32>,
    data_memory: HashMap<u32, u64>,
}

impl TtaTestHelper {
    fn new() -> Self {
        Self {
            cycle_count: 0,
            instruction_memory: HashMap::new(),
            data_memory: HashMap::new(),
        }
    }

    fn step<'a>(&mut self, tta: &mut TtaTestbench<'a>) {
        tta.clk_i = 1;
        if tta.instr_valid_o != 0 {
            let addr = tta.instr_addr_o;
            tta.instr_data_read_i = *self.instruction_memory.get(&addr).unwrap_or(&0);
            tta.instr_ready_i = 1;
        } else {
            tta.instr_ready_i = 0;
        }
        if tta.data_valid_o != 0 {
            let addr = tta.data_addr_o;
            let wstrb = tta.data_wstrb_o as u8;
            if wstrb != 0 {
                let existing = *self.data_memory.get(&addr).unwrap_or(&0);
                let write_val = tta.data_data_write_o;
                let mut bytes = (existing as u32).to_le_bytes();
                let write_bytes = (write_val as u32).to_le_bytes();
                for i in 0..4 {
                    if (wstrb & (1 << i)) != 0 {
                        bytes[i] = write_bytes[i];
                    }
                }
                let tag = write_val & 0xF_0000_0000;
                self.data_memory.insert(addr, tag | u32::from_le_bytes(bytes) as u64);
            } else {
                tta.data_data_read_i = *self.data_memory.get(&addr).unwrap_or(&0);
            }
            tta.data_ready_i = 1;
        } else {
            tta.data_ready_i = 0;
        }
        tta.eval();
        tta.clk_i = 0;
        tta.eval();
        self.cycle_count += 1;
    }

    fn load_instructions(&mut self, instructions: &[u32], start_addr: u32) {
        for (i, &instr) in instructions.iter().enumerate() {
            self.instruction_memory.insert(start_addr + i as u32, instr);
        }
    }

    fn run_until_reset_released<'a>(
        &mut self,
        tta: &mut TtaTestbench<'a>,
    ) {
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.eval();
        self.step(tta);
        tta.rst_i = 0;
        self.step(tta);
    }

    fn run_for_cycles<'a>(&mut self, tta: &mut TtaTestbench<'a>, cycles: u32) {
        for _ in 0..cycles {
            self.step(tta);
        }
    }

    fn get_data_memory(&self, addr: u32) -> u64 {
        *self.data_memory.get(&addr).unwrap_or(&0)
    }

    fn result_value(&self) -> u32 {
        (self.get_data_memory(RESULT_ADDR as u32) & 0xFFFF_FFFF) as u32
    }

    fn result_tag(&self) -> u8 {
        ((self.get_data_memory(RESULT_ADDR as u32) >> 32) & 0xF) as u8
    }
}

fn run_lisp(source: &str, cycles: u32) -> TtaTestHelper {
    let runtime = create_tta_runtime().expect("create runtime");
    let mut tta = runtime
        .create_model_simple::<TtaTestbench>()
        .expect("create model");
    let mut helper = TtaTestHelper::new();

    tta.rst_i = 1;
    tta.clk_i = 0;
    tta.instr_ready_i = 0;
    tta.data_ready_i = 0;
    tta.instr_data_read_i = 0;
    tta.data_data_read_i = 0;
    tta.mailbox_valid_i = 0;
    tta.mailbox_data_i = 0;

    let code = compile_with_result_store(source);
    helper.load_instructions(&code, 0);
    helper.run_until_reset_released(&mut tta);
    helper.run_for_cycles(&mut tta, cycles);
    helper
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lisp_integer() {
        let h = run_lisp("42", 200);
        assert_eq!(h.result_value(), 42);
        assert_eq!(h.result_tag(), 0); // fixnum
    }

    #[test]
    fn test_lisp_addition() {
        let h = run_lisp("(+ 1 2)", 200);
        assert_eq!(h.result_value(), 3);
    }

    #[test]
    fn test_lisp_nested_arith() {
        let h = run_lisp("(+ (* 2 3) 4)", 500);
        assert_eq!(h.result_value(), 10);
    }

    #[test]
    fn test_lisp_subtraction() {
        let h = run_lisp("(- 10 3)", 200);
        assert_eq!(h.result_value(), 7);
    }

    #[test]
    fn test_lisp_comparison() {
        let h = run_lisp("(> 5 3)", 200);
        assert_eq!(h.result_value(), 1);
    }

    #[test]
    fn test_lisp_if_true() {
        let h = run_lisp("(if 1 42 99)", 300);
        assert_eq!(h.result_value(), 42);
    }

    #[test]
    fn test_lisp_if_false() {
        let h = run_lisp("(if 0 42 99)", 300);
        assert_eq!(h.result_value(), 99);
    }

    #[test]
    fn test_lisp_cons_car() {
        let h = run_lisp("(car (cons 42 99))", 500);
        assert_eq!(h.result_value(), 42);
    }

    #[test]
    fn test_lisp_cons_cdr() {
        let h = run_lisp("(cdr (cons 42 99))", 500);
        assert_eq!(h.result_value(), 99);
    }

    #[test]
    fn test_lisp_null_true() {
        let h = run_lisp("(null? ())", 300);
        assert_eq!(h.result_value(), 1);
    }

    #[test]
    fn test_lisp_null_false() {
        let h = run_lisp("(null? (cons 1 2))", 500);
        assert_eq!(h.result_value(), 0);
    }

    #[test]
    fn test_lisp_let_binding() {
        let h = run_lisp("(let ((x 10) (y 20)) (+ x y))", 500);
        assert_eq!(h.result_value(), 30);
    }

    #[test]
    fn test_lisp_define_and_call() {
        let h = run_lisp("(begin (define (add a b) (+ a b)) (add 3 4))", 500);
        assert_eq!(h.result_value(), 7);
    }

    #[test]
    fn test_lisp_define_recursive() {
        // Factorial: (fact 5) = 120
        let h = run_lisp(
            "(begin
               (define (fact n)
                 (if (= n 0) 1 (* n (fact (- n 1)))))
               (fact 5))",
            5000,
        );
        assert_eq!(h.result_value(), 120);
    }

    #[test]
    fn test_lisp_closure() {
        // Closure captures 'a' from enclosing let
        let h = run_lisp(
            "(let ((a 5))
               (let ((f (lambda (x) (+ x a))))
                 (f 10)))",
            1000,
        );
        assert_eq!(h.result_value(), 15);
    }

    #[test]
    fn test_lisp_higher_order() {
        // Apply a function to an argument
        let h = run_lisp(
            "(begin
               (define (apply-twice f x) (f (f x)))
               (define (inc n) (+ n 1))
               (apply-twice inc 5))",
            2000,
        );
        assert_eq!(h.result_value(), 7);
    }
}
