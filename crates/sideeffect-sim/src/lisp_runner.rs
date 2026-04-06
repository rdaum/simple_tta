use std::collections::HashMap;
use std::path::Path;

use marlin::verilator::VerilatorRuntime;
use sideeffect_sim::{create_tta_runtime, instr, TtaTestbench, Unit};

const MAX_CYCLES: u32 = 100_000;
/// Boot loop lives at a high address so user code can always start at 0.
const BOOT_ADDR: u32 = 0xF000;

/// Boot code: jump to boot loop at addr 0, boot loop at BOOT_ADDR.
fn boot_code() -> HashMap<u32, u32> {
    let mut mem = HashMap::new();

    // addr 0: jump to boot loop (2 words)
    let jump = instr()
        .src(Unit::UNIT_ABS_OPERAND).soperand(BOOT_ADDR)
        .dst(Unit::UNIT_PC)
        .assemble();
    for (i, &w) in jump.iter().enumerate() {
        mem.insert(i as u32, w);
    }

    // Boot loop at BOOT_ADDR:
    //   BOOT_ADDR+0: mailbox → call       ; block, call user code
    //   BOOT_ADDR+1: r1 → mailbox         ; send result to host
    //   BOOT_ADDR+2..3: operand(BOOT_ADDR) → pc  ; loop
    let boot = vec![
        instr().src_mailbox().dst_call(),
        instr().src(Unit::UNIT_REGISTER).si(1).dst_mailbox(),
        instr().src(Unit::UNIT_ABS_OPERAND).soperand(BOOT_ADDR).dst(Unit::UNIT_PC),
    ];
    let mut addr = BOOT_ADDR;
    for i in &boot {
        for w in i.assemble() {
            mem.insert(addr, w);
            addr += 1;
        }
    }

    mem
}

/// Persistent simulator state for the REPL.
struct PersistentSim<'a> {
    tta: TtaTestbench<'a>,
    instruction_memory: HashMap<u32, u32>,
    data_memory: HashMap<u32, u64>,
    cycle_count: u32,
}

impl<'a> PersistentSim<'a> {
    fn new(runtime: &'a VerilatorRuntime) -> Self {
        let mut tta = runtime
            .create_model_simple::<TtaTestbench>()
            .expect("create model");
        let instruction_memory = boot_code();
        let data_memory = HashMap::new();

        // Reset
        tta.rst_i = 1;
        tta.clk_i = 0;
        tta.instr_ready_i = 0;
        tta.data_ready_i = 0;
        tta.instr_data_read_i = 0;
        tta.data_data_read_i = 0;
        tta.mailbox_valid_i = 0;
        tta.mailbox_data_i = 0;
        tta.eval();

        let mut sim = Self {
            tta,
            instruction_memory,
            data_memory,
            cycle_count: 0,
        };

        // Release reset
        sim.step();
        sim.tta.rst_i = 0;
        sim.step();

        // Run a few cycles so CPU reaches the mailbox wait
        for _ in 0..20 {
            sim.step();
        }

        sim
    }

    fn step(&mut self) {
        self.tta.clk_i = 1;
        if self.tta.instr_valid_o != 0 {
            let addr = self.tta.instr_addr_o;
            self.tta.instr_data_read_i = *self.instruction_memory.get(&addr).unwrap_or(&0);
            self.tta.instr_ready_i = 1;
        } else {
            self.tta.instr_ready_i = 0;
        }
        if self.tta.data_valid_o != 0 {
            let addr = self.tta.data_addr_o;
            let wstrb = self.tta.data_wstrb_o as u8;
            if wstrb != 0 {
                let existing = *self.data_memory.get(&addr).unwrap_or(&0);
                let write_val = self.tta.data_data_write_o;
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
                self.tta.data_data_read_i = *self.data_memory.get(&addr).unwrap_or(&0);
            }
            self.tta.data_ready_i = 1;
        } else {
            self.tta.data_ready_i = 0;
        }
        self.tta.eval();
        self.tta.clk_i = 0;
        self.tta.eval();
        self.cycle_count += 1;
    }

    /// Load compiled code at the next free address and send the entry
    /// address via mailbox. Runs until the CPU writes the result back
    /// to the mailbox output. Returns (value, tag, cycles).
    fn run_code(&mut self, code: &[u32]) -> Result<(u32, u8, u32), String> {
        // Always load user code starting at address 0.
        // The boot loop jump at addr 0-1 gets overwritten — that's fine,
        // it only runs once at startup. The boot loop itself is at BOOT_ADDR.
        for (i, &word) in code.iter().enumerate() {
            self.instruction_memory.insert(i as u32, word);
        }

        // Send entry address (0) via mailbox
        self.tta.mailbox_data_i = 0;
        self.tta.mailbox_valid_i = 1;

        // Step until ack
        for _ in 0..10 {
            self.step();
            if self.tta.mailbox_ack_o != 0 {
                break;
            }
        }
        self.tta.mailbox_valid_i = 0;

        // Run until mailbox_out_valid fires (result returned)
        let start_cycle = self.cycle_count;
        for _ in 0..MAX_CYCLES {
            self.step();
            if self.tta.mailbox_out_valid_o != 0 {
                let result = self.tta.mailbox_out_o;
                let value = (result & 0xFFFF_FFFF) as u32;
                let tag = ((result >> 32) & 0xF) as u8;
                let cycles = self.cycle_count - start_cycle;
                // Let the CPU settle back into mailbox wait
                for _ in 0..10 {
                    self.step();
                }
                return Ok((value, tag, cycles));
            }
        }
        Err("Timed out waiting for result".into())
    }
}

/// Compile a Lisp expression to TTA code (without boot loop or result store —
/// the boot loop handles calling and result collection).
fn compile_function(source: &str) -> Result<(Vec<u32>, String), String> {
    let prog = sideeffect_lisp::compile(source).map_err(|e| e.to_string())?;
    let mut code = prog.code;
    // The compiler emits a trailing halt (none→none). Replace it with a return:
    // pop stack[1] → PC (return to boot loop)
    code.pop();
    code.extend(instr().src(Unit::UNIT_STACK_PUSH_POP).si(1).dst(Unit::UNIT_PC).assemble());

    let disasm = sideeffect_asm::disassemble_to_string(&code);
    Ok((code, disasm))
}

/// Ensure CWD is `crates/sideeffect-sim/` so the simulator's relative
/// paths to RTL sources resolve correctly.
fn ensure_cwd() {
    if Path::new("../../rtl/tta.sv").exists() {
        return;
    }
    let mut dir = std::env::current_dir().unwrap();
    loop {
        let candidate = dir.join("crates/sideeffect-sim");
        if candidate.join("../../rtl/tta.sv").exists() {
            std::env::set_current_dir(&candidate).unwrap();
            return;
        }
        if !dir.pop() {
            break;
        }
    }
    if Path::new("rtl/tta.sv").exists() {
        std::env::set_current_dir("crates/sideeffect-sim").unwrap();
        return;
    }
    eprintln!("warning: could not find project root; RTL paths may not resolve");
}

fn format_result(value: u32, tag: u8) -> String {
    match tag {
        0 => format!("{}", value as i32),
        1 => format!("<cons @{}>", value),
        2 => format!("<symbol #{}>", value),
        3 => "nil".to_string(),
        4 => format!("<lambda @{}>", value),
        5 => format!("<builtin #{}>", value),
        t => format!("<unknown tag={} val={}>", t, value),
    }
}

fn main() {
    ensure_cwd();
    let args: Vec<String> = std::env::args().collect();

    let runtime = create_tta_runtime().expect("Failed to create Verilator runtime");

    // Single expression mode
    if args.len() > 1 {
        let source = args[1..].join(" ");
        let mut sim = PersistentSim::new(&runtime);
        match compile_function(&source) {
            Ok((code, disasm)) => {
                let words = code.len();
                eprint!("{}", disasm);
                match sim.run_code(&code) {
                    Ok((value, tag, cycles)) => {
                        println!("{}", format_result(value, tag));
                        eprintln!("({} cycles, {} words)", cycles, words);
                    }
                    Err(e) => eprintln!("runtime error: {}", e),
                }
            }
            Err(e) => {
                eprintln!("compile error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // REPL mode — persistent hardware state across evaluations
    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to initialize readline: {}", e);
            std::process::exit(1);
        }
    };

    let mut sim = PersistentSim::new(&runtime);
    let mut accumulated_defines = Vec::<String>::new();

    println!("sideeffect lisp — type expressions, Ctrl-D to exit");
    println!("Hardware: 36-bit tagged TTA, Verilator simulation");
    println!("Persistent state: heap, registers, and stacks survive between inputs\n");

    loop {
        let line = match rl.readline("λ> ") {
            Ok(line) => {
                let trimmed = line.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(&trimmed);
                trimmed
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("bye");
                break;
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                continue;
            }
            Err(e) => {
                eprintln!("readline error: {}", e);
                break;
            }
        };

        let is_define = line.trim_start().starts_with("(define ");

        let mut full_source = String::new();
        let needs_begin = !accumulated_defines.is_empty() || is_define;

        if needs_begin {
            full_source.push_str("(begin\n");
            for def in &accumulated_defines {
                full_source.push_str("  ");
                full_source.push_str(def);
                full_source.push('\n');
            }
            full_source.push_str("  ");
            full_source.push_str(&line);
            full_source.push_str("\n)");
        } else {
            full_source = line.clone();
        }

        match compile_function(&full_source) {
            Ok((code, disasm)) => {
                let words = code.len();
                eprint!("{}", disasm);
                match sim.run_code(&code) {
                    Ok((value, tag, cycles)) => {
                        println!("=> {}", format_result(value, tag));
                        eprintln!("   ({} cycles, {} words)", cycles, words);
                        if is_define {
                            accumulated_defines.push(line);
                        }
                    }
                    Err(e) => eprintln!("runtime error: {}", e),
                }
            }
            Err(e) => eprintln!("compile error: {}", e),
        }
    }
}
