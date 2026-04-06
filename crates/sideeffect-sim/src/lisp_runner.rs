use std::collections::HashMap;

use sideeffect_sim::{create_tta_runtime, instr, TtaTestbench, Unit};

const RESULT_ADDR: u8 = 200;
const MAX_CYCLES: u32 = 50_000;

/// Returns (value, tag, cycles, code_words).
fn compile_and_run(source: &str) -> Result<(u32, u8, u32, usize), String> {
    let prog = sideeffect_lisp::compile(source).map_err(|e| e.to_string())?;
    let mut code = prog.code;
    // Remove trailing halt, add result store
    code.pop();
    code.extend(
        instr()
            .src(Unit::UNIT_REGISTER).si(1)
            .dst(Unit::UNIT_MEMORY_IMMEDIATE).di(RESULT_ADDR)
            .assemble(),
    );
    code.extend(instr().assemble()); // halt

    let runtime = create_tta_runtime().map_err(|e| format!("{:?}", e))?;
    let mut tta = runtime
        .create_model_simple::<TtaTestbench>()
        .map_err(|e| format!("{:?}", e))?;

    let mut instruction_memory: HashMap<u32, u32> = HashMap::new();
    let mut data_memory: HashMap<u32, u64> = HashMap::new();

    for (i, &word) in code.iter().enumerate() {
        instruction_memory.insert(i as u32, word);
    }

    // Reset
    tta.rst_i = 1;
    tta.clk_i = 0;
    tta.instr_ready_i = 0;
    tta.data_ready_i = 0;
    tta.instr_data_read_i = 0;
    tta.data_data_read_i = 0;
    tta.eval();

    // Release reset
    step(&mut tta, &instruction_memory, &mut data_memory);
    tta.rst_i = 0;
    step(&mut tta, &instruction_memory, &mut data_memory);

    // Run until result is written or max cycles
    let mut cycles = 0u32;
    let mut result_written = false;
    for _ in 0..MAX_CYCLES {
        step(&mut tta, &instruction_memory, &mut data_memory);
        cycles += 1;
        // Detect write to RESULT_ADDR
        if tta.data_valid_o != 0 && tta.data_wstrb_o != 0
            && tta.data_addr_o == RESULT_ADDR as u32
        {
            result_written = true;
        }
        // Give a few more cycles after the result write for the store to settle
        if result_written {
            for _ in 0..5 {
                step(&mut tta, &instruction_memory, &mut data_memory);
                cycles += 1;
            }
            break;
        }
    }

    let result = *data_memory.get(&(RESULT_ADDR as u32)).unwrap_or(&0);
    let value = (result & 0xFFFF_FFFF) as u32;
    let tag = ((result >> 32) & 0xF) as u8;
    Ok((value, tag, cycles, code.len()))
}

fn step<'a>(
    tta: &mut TtaTestbench<'a>,
    instr_mem: &HashMap<u32, u32>,
    data_mem: &mut HashMap<u32, u64>,
) {
    tta.clk_i = 1;
    if tta.instr_valid_o != 0 {
        let addr = tta.instr_addr_o;
        tta.instr_data_read_i = *instr_mem.get(&addr).unwrap_or(&0);
        tta.instr_ready_i = 1;
    } else {
        tta.instr_ready_i = 0;
    }
    if tta.data_valid_o != 0 {
        let addr = tta.data_addr_o;
        let wstrb = tta.data_wstrb_o as u8;
        if wstrb != 0 {
            let existing = *data_mem.get(&addr).unwrap_or(&0);
            let write_val = tta.data_data_write_o;
            let mut bytes = (existing as u32).to_le_bytes();
            let write_bytes = (write_val as u32).to_le_bytes();
            for i in 0..4 {
                if (wstrb & (1 << i)) != 0 {
                    bytes[i] = write_bytes[i];
                }
            }
            let tag = write_val & 0xF_0000_0000;
            data_mem.insert(addr, tag | u32::from_le_bytes(bytes) as u64);
        } else {
            tta.data_data_read_i = *data_mem.get(&addr).unwrap_or(&0);
        }
        tta.data_ready_i = 1;
    } else {
        tta.data_ready_i = 0;
    }
    tta.eval();
    tta.clk_i = 0;
    tta.eval();
}

fn format_result(value: u32, tag: u8) -> String {
    match tag {
        0 => {
            // Fixnum — check if it looks like a negative number (i32)
            let signed = value as i32;
            format!("{}", signed)
        }
        1 => format!("<cons @{}>", value),
        2 => format!("<symbol #{}>", value),
        3 => "nil".to_string(),
        4 => format!("<lambda @{}>", value),
        5 => format!("<builtin #{}>", value),
        t => format!("<unknown tag={} val={}>", t, value),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // If an expression is passed as argument, evaluate and exit
    if args.len() > 1 {
        let source = args[1..].join(" ");
        match compile_and_run(&source) {
            Ok((value, tag, cycles, words)) => {
                println!("{}", format_result(value, tag));
                eprintln!("({} cycles, {} words)", cycles, words);
            }
            Err(e) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // REPL mode
    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("Failed to initialize readline: {}", e);
            std::process::exit(1);
        }
    };

    let mut accumulated_defines = Vec::<String>::new();

    println!("sideeffect lisp — type expressions, Ctrl-D to exit");
    println!("Hardware: 36-bit tagged TTA, Verilator simulation\n");

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

        // Check if this is a define — if so, accumulate it
        let is_define = line.trim_start().starts_with("(define ");

        // Build full source: all accumulated defines + current expression
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

        match compile_and_run(&full_source) {
            Ok((value, tag, cycles, words)) => {
                println!("=> {}", format_result(value, tag));
                eprintln!("   ({} cycles, {} words)", cycles, words);

                // If it was a define, remember it
                if is_define {
                    accumulated_defines.push(line);
                }
            }
            Err(e) => {
                eprintln!("error: {}", e);
            }
        }
    }
}
