use crate::assembler::Unit;

fn unit_name(u: u8) -> &'static str {
    match u {
        0 => "none",
        1 => "stack",
        2 => "stack_idx",
        3 => "reg",
        4 => "reg.val",
        5 => "reg.tag",
        6 => "deref",
        7 => "alu.l",
        8 => "alu.r",
        9 => "alu.op",
        10 => "alu.res",
        11 => "mem_imm",
        12 => "mem_op",
        13 => "imm",
        14 => "operand",
        15 => "pc",
        16 => "pc_cond",
        17 => "cond",
        18 => "barrier",
        19 => "mem_byte",
        20 => "pop.val",
        21 => "pop.tag",
        22 => "peek.val",
        23 => "peek.tag",
        24 => "tag_cmp",
        25 => "alloc",
        26 => "alloc_ptr",
        27 => "call",
        _ => "???",
    }
}

fn alu_op_name(op: u8) -> &'static str {
    match op {
        0 => "NOP",
        1 => "ADD",
        2 => "SUB",
        3 => "MUL",
        4 => "DIV",
        5 => "MOD",
        6 => "EQL",
        7 => "SL",
        8 => "SR",
        9 => "SRA",
        10 => "NOT",
        11 => "AND",
        12 => "OR",
        13 => "XOR",
        14 => "GT",
        15 => "LT",
        _ => "???",
    }
}

fn needs_src_operand(src: u8) -> bool {
    src == Unit::UNIT_MEMORY_OPERAND as u8
        || src == Unit::UNIT_ABS_OPERAND as u8
        || src == Unit::UNIT_MEM_BYTE as u8
}

fn needs_dst_operand(dst: u8) -> bool {
    dst == Unit::UNIT_MEMORY_OPERAND as u8
        || dst == Unit::UNIT_ABS_OPERAND as u8
        || dst == Unit::UNIT_MEM_BYTE as u8
}

/// Format the source side of an instruction.
fn fmt_src(src: u8, si: u8, operand: Option<u32>) -> String {
    match src {
        0 => "0".into(),
        1 => format!("pop[{}]", si & 0x7),
        2 => format!("peek[{}, #{}]", si & 0x7, (si >> 3) & 0x1F),
        3 => format!("r{}", si & 0x1F),
        4 => format!("r{}.val", si & 0x1F),
        5 => format!("r{}.tag", si & 0x1F),
        6 => format!("*r{}+{}", si & 0x1F, (si >> 5) & 0x7),
        7 => format!("alu[{}].l", si & 0x7),
        8 => format!("alu[{}].r", si & 0x7),
        10 => format!("alu[{}]", si & 0x7),
        13 => format!("{}", si),
        14 => {
            if let Some(op) = operand {
                format!("0x{:x}", op)
            } else {
                format!("operand({})", si)
            }
        }
        15 => "pc".into(),
        17 => "cond".into(),
        18 => "barrier.pop".into(),
        11 => format!("mem[{}]", si),
        12 => {
            if let Some(op) = operand {
                format!("mem[0x{:x}]", op)
            } else {
                "mem[op]".into()
            }
        }
        19 => {
            if let Some(op) = operand {
                format!("byte[0x{:x}+{}]", op, si & 0x3)
            } else {
                format!("byte[op+{}]", si & 0x3)
            }
        }
        20 => format!("pop.val[{}]", si & 0x7),
        21 => format!("pop.tag[{}]", si & 0x7),
        22 => format!("peek.val[{}, #{}]", si & 0x7, (si >> 3) & 0x1F),
        23 => format!("peek.tag[{}, #{}]", si & 0x7, (si >> 3) & 0x1F),
        26 => format!("alloc_ptr<{}>", si & 0xF),
        _ => format!("{}[{}]", unit_name(src), si),
    }
}

/// Format the destination side of an instruction.
fn fmt_dst(dst: u8, di: u8, operand: Option<u32>) -> String {
    match dst {
        0 => "_".into(),
        1 => format!("push[{}]", di & 0x7),
        2 => format!("poke[{}, #{}]", di & 0x7, (di >> 3) & 0x1F),
        3 => format!("r{}", di & 0x1F),
        4 => format!("r{}.val", di & 0x1F),
        5 => format!("r{}.tag", di & 0x1F),
        6 => format!("*r{}+{}", di & 0x1F, (di >> 5) & 0x7),
        7 => format!("alu[{}].l", di & 0x7),
        8 => format!("alu[{}].r", di & 0x7),
        9 => {
            let op_val = di & 0xF;
            format!("alu[{}].op={}", di & 0x7, alu_op_name(op_val))
        }
        11 => format!("mem[{}]", di),
        12 => {
            if let Some(op) = operand {
                format!("mem[0x{:x}]", op)
            } else {
                "mem[op]".into()
            }
        }
        15 => "pc".into(),
        16 => "pc_cond".into(),
        17 => "cond".into(),
        18 => "barrier.push".into(),
        19 => {
            if let Some(op) = operand {
                format!("byte[0x{:x}+{}]", op, di & 0x3)
            } else {
                format!("byte[op+{}]", di & 0x3)
            }
        }
        24 => format!("tag_cmp[{}]", di & 0xF),
        25 => "alloc".into(),
        27 => "call".into(),
        _ => format!("{}[{}]", unit_name(dst), di),
    }
}

/// Disassemble a slice of instruction words into human-readable lines.
/// Returns a Vec of (word_address, disassembly_string) pairs.
pub fn disassemble(code: &[u32]) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    let mut pc = 0usize;

    while pc < code.len() {
        let op = code[pc];
        let src = (op & 0x1F) as u8;
        let dst = ((op >> 5) & 0x1F) as u8;
        let si = ((op >> 10) & 0xFF) as u8;
        let di = ((op >> 18) & 0xFF) as u8;
        let flags = ((op >> 26) & 0x3F) as u8;

        let addr = pc as u32;
        let mut word_count = 1;

        let src_op = if needs_src_operand(src) && pc + word_count < code.len() {
            let op = Some(code[pc + word_count]);
            word_count += 1;
            op
        } else {
            None
        };

        let dst_op = if needs_dst_operand(dst) && pc + word_count < code.len() {
            let op = Some(code[pc + word_count]);
            word_count += 1;
            op
        } else {
            None
        };

        let src_str = fmt_src(src, si, src_op);
        let dst_str = fmt_dst(dst, di, dst_op);

        // ALU operator destination: show as "op → alu[N].op" but the
        // value being set is the src value, which for imm is si itself
        // Special case: if dst is alu.op and src is imm, show the op name
        let dst_str = if dst == 9 && src == 13 {
            format!("alu[{}].op={}", di & 0x7, alu_op_name(si))
        } else {
            dst_str
        };

        let mut line = format!("{} → {}", src_str, dst_str);

        if flags & 0x1 != 0 {
            line.push_str("  [if_set]");
        }
        if flags & 0x2 != 0 {
            line.push_str("  [if_clear]");
        }

        result.push((addr, line));
        pc += word_count;
    }

    result
}

/// Format disassembly as a multi-line string with addresses.
pub fn disassemble_to_string(code: &[u32]) -> String {
    let lines = disassemble(code);
    let mut out = String::new();
    for (addr, text) in &lines {
        out.push_str(&format!("  {:04x}: {}\n", addr, text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::{instr, Unit};
    use crate::ALUOp;

    #[test]
    fn test_disasm_reg_move() {
        let code = instr().src_reg(3).dst_reg(5).assemble();
        let lines = disassemble(&code);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].1, "r3 → r5");
    }

    #[test]
    fn test_disasm_imm_to_reg() {
        let code = instr()
            .src(Unit::UNIT_ABS_IMMEDIATE).si(42)
            .dst_reg(1)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "42 → r1");
    }

    #[test]
    fn test_disasm_operand() {
        let code = instr()
            .src(Unit::UNIT_ABS_OPERAND).soperand(0x1234)
            .dst_reg(0)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "0x1234 → r0");
    }

    #[test]
    fn test_disasm_alu() {
        let code = instr()
            .src(Unit::UNIT_ABS_IMMEDIATE).si(ALUOp::ALU_ADD as u8)
            .dst(Unit::UNIT_ALU_OPERATOR).di(0)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "1 → alu[0].op=ADD");
    }

    #[test]
    fn test_disasm_deref() {
        let code = instr()
            .src_deref(2, 1)
            .dst_reg(5)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "*r2+1 → r5");
    }

    #[test]
    fn test_disasm_predicated() {
        let code = instr()
            .src_reg(0).dst_reg(1)
            .predicate_if_set()
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "r0 → r1  [if_set]");
    }

    #[test]
    fn test_disasm_call() {
        let code = instr()
            .src(Unit::UNIT_ABS_OPERAND).soperand(100)
            .dst_call()
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "0x64 → call");
    }

    #[test]
    fn test_disasm_alloc() {
        let code = instr()
            .src_alloc_ptr(1)
            .dst_reg(2)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines[0].1, "alloc_ptr<1> → r2");
    }

    #[test]
    fn test_disasm_multi_word() {
        // 3-word instruction
        let code = instr()
            .src(Unit::UNIT_ABS_OPERAND).soperand(0xAA)
            .dst(Unit::UNIT_MEMORY_OPERAND).doperand(0xBB)
            .assemble();
        let lines = disassemble(&code);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].1, "0xaa → mem[0xbb]");
    }
}
