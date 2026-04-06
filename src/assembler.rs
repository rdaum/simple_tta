//! Rust implementation of the TTA assembler, equivalent to the C++ version

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum ALUOp {
    ALU_NOP = 0x000,
    ALU_ADD = 0x001,
    ALU_SUB = 0x002,
    ALU_MUL = 0x003,
    ALU_DIV = 0x004,
    ALU_MOD = 0x005,
    ALU_EQL = 0x006,
    ALU_SL = 0x007,
    ALU_SR = 0x008,
    ALU_SRA = 0x009,
    ALU_NOT = 0x00a,
    ALU_AND = 0x00b,
    ALU_OR = 0x00c,
    ALU_XOR = 0x00d,
    ALU_GT = 0x00e,
    ALU_LT = 0x00f,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum Unit {
    UNIT_NONE = 0,
    UNIT_STACK_PUSH_POP = 1,
    UNIT_STACK_INDEX = 2,
    UNIT_REGISTER = 3,
    UNIT_ALU_LEFT = 4,
    UNIT_ALU_RIGHT = 5,
    UNIT_ALU_OPERATOR = 6,
    UNIT_ALU_RESULT = 7,
    UNIT_MEMORY_IMMEDIATE = 8,
    UNIT_MEMORY_OPERAND = 9,
    UNIT_PC = 10,
    UNIT_ABS_IMMEDIATE = 11,
    UNIT_ABS_OPERAND = 12,
    UNIT_WRITE_BARRIER = 13,
    UNIT_COND = 14,
    UNIT_PC_COND = 15,
}

impl Unit {
    fn needs_operand(self) -> bool {
        matches!(self, Unit::UNIT_MEMORY_OPERAND | Unit::UNIT_ABS_OPERAND)
    }

}

/// Access width for sub-word memory operations.
///
/// Encoded in immediate bits [11:10]. Bits [9:8] carry the byte offset
/// within the 32-bit word (0-3 for byte, 0 or 2 for halfword).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum AccessWidth {
    /// Full 32-bit word (default, byte offset ignored)
    Word = 0b00,
    /// Single byte, zero-extended on read
    Byte = 0b01,
    /// 16-bit halfword, zero-extended on read
    HalfWord = 0b10,
}

/// Encode access width and byte offset into the upper bits of a 12-bit
/// immediate field.
///
/// For ACCESS_WORD: the full 12 bits are the address/register (backward
/// compatible — byte offset is ignored).
///
/// For ACCESS_BYTE / ACCESS_HALFWORD:
///   [11:10] = width, [9:8] = byte offset, [7:0] = address or register.
fn encode_mem_immediate(addr_or_reg: u16, width: AccessWidth, byte_offset: u8) -> u16 {
    if width == AccessWidth::Word {
        // Full 12-bit field used as address/register, no sub-word encoding.
        assert!(
            addr_or_reg < (1 << 12),
            "Word-mode address must fit in 12 bits"
        );
        return addr_or_reg;
    }
    assert!(byte_offset < 4, "Byte offset must be 0-3");
    if width == AccessWidth::HalfWord {
        assert!(
            byte_offset == 0 || byte_offset == 2,
            "Halfword offset must be 0 or 2"
        );
    }
    assert!(
        addr_or_reg < 256,
        "Sub-word memory-immediate address must fit in 8 bits"
    );
    let base = addr_or_reg & 0xFF; // bits [7:0]
    let off = (byte_offset as u16 & 0x3) << 8; // bits [9:8]
    let w = (width as u16 & 0x3) << 10; // bits [11:10]
    base | off | w
}

/// Register access mode for tagged-value support.
///
/// Encoded in immediate bits [6:5] of UNIT_REGISTER instructions.
/// Bits [4:0] are the register index, [9:7] are the word offset for
/// DEREF mode.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum RegMode {
    /// Full 32-bit read/write (backward compatible default)
    Raw = 0b00,
    /// Read: tag bits zeroed; Write: preserve tag, set payload
    Value = 0b01,
    /// Read: tag bits only; Write: preserve payload, set tag
    Tag = 0b10,
    /// Strip tag, add word offset, load/store via data bus
    Deref = 0b11,
}

/// Encode a register access with mode and optional DEREF word offset
/// into the 12-bit immediate field for UNIT_REGISTER.
///
///   [4:0] = register index, [6:5] = mode, [9:7] = deref offset
fn encode_reg_immediate(reg: u16, mode: RegMode, offset: u8) -> u16 {
    assert!(reg < 32, "Register index must be 0-31");
    assert!(offset < 8, "DEREF word offset must be 0-7");
    (reg & 0x1F) | ((mode as u16 & 0x3) << 5) | ((offset as u16 & 0x7) << 7)
}

#[derive(Debug, Clone)]
pub struct Instr {
    src_unit: Unit,
    si: u16, // 12-bit immediate
    dst_unit: Unit,
    di: u16, // 12-bit immediate
    soperand: Option<u32>,
    doperand: Option<u32>,
}

impl Default for Instr {
    fn default() -> Self {
        Self::new()
    }
}

impl Instr {
    pub fn new() -> Self {
        Self {
            src_unit: Unit::UNIT_NONE,
            si: 0,
            dst_unit: Unit::UNIT_NONE,
            di: 0,
            soperand: None,
            doperand: None,
        }
    }

    pub fn src(mut self, unit: Unit) -> Self {
        self.src_unit = unit;
        self
    }

    pub fn dst(mut self, unit: Unit) -> Self {
        self.dst_unit = unit;
        self
    }

    pub fn si(mut self, immediate: u16) -> Self {
        assert!(
            immediate < (1 << 12),
            "Source immediate must fit in 12 bits"
        );
        self.si = immediate;
        self
    }

    pub fn di(mut self, immediate: u16) -> Self {
        assert!(
            immediate < (1 << 12),
            "Destination immediate must fit in 12 bits"
        );
        self.di = immediate;
        self
    }

    pub fn soperand(mut self, operand: u32) -> Self {
        assert!(
            self.src_unit.needs_operand(),
            "Source unit doesn't use operand"
        );
        self.soperand = Some(operand);
        self
    }

    pub fn doperand(mut self, operand: u32) -> Self {
        assert!(
            self.dst_unit.needs_operand(),
            "Destination unit doesn't use operand"
        );
        self.doperand = Some(operand);
        self
    }

    fn uses_soperand(&self) -> bool {
        self.src_unit.needs_operand()
    }

    fn uses_doperand(&self) -> bool {
        self.dst_unit.needs_operand()
    }

    pub fn assemble(&self) -> Vec<u32> {
        assert_eq!(
            self.uses_soperand(),
            self.soperand.is_some(),
            "Source operand mismatch"
        );
        assert_eq!(
            self.uses_doperand(),
            self.doperand.is_some(),
            "Destination operand mismatch"
        );

        // Pack the instruction format exactly like the C++ version
        // struct OpFormat {
        //   unsigned short src_unit : 4;
        //   unsigned short si : 12;
        //   unsigned dst_unit : 4;
        //   unsigned short di : 12;
        // };
        let packed = ((self.src_unit as u32) & 0xF)
            | (((self.si as u32) & 0xFFF) << 4)
            | (((self.dst_unit as u32) & 0xF) << 16)
            | (((self.di as u32) & 0xFFF) << 20);

        let mut result = vec![packed];

        if let Some(sop) = self.soperand {
            result.push(sop);
        }

        if let Some(dop) = self.doperand {
            result.push(dop);
        }

        result
    }

    // Stack operation helpers
    pub fn push_reg(mut self, stack_id: u8, src_reg: u16) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = src_reg;
        self.dst_unit = Unit::UNIT_STACK_PUSH_POP;
        self.di = stack_id as u16;
        self
    }

    pub fn pop_to_reg(mut self, stack_id: u8, dst_reg: u16) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_STACK_PUSH_POP;
        self.si = stack_id as u16;
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = dst_reg;
        self
    }

    pub fn push_immediate(mut self, stack_id: u8, value: u32) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_ABS_OPERAND;
        self.si = 0;
        self.soperand = Some(value);
        self.dst_unit = Unit::UNIT_STACK_PUSH_POP;
        self.di = stack_id as u16;
        self
    }

    pub fn stack_peek(mut self, stack_id: u8, offset: u8, dst_reg: u16) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 64, "Stack offset must be 0-63");
        self.src_unit = Unit::UNIT_STACK_INDEX;
        // Pack stack_id in bits 2:0, offset in bits 8:3
        self.si = (stack_id as u16) | ((offset as u16) << 3);
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = dst_reg;
        self
    }

    // --- Tagged stack access helpers ---
    //
    // Pop and peek with tag mode: RAW (default), VALUE (tag bits zeroed),
    // TAG (tag bits only). Push/poke writes are always RAW.

    /// Set source to a stack pop with the given tag mode.
    ///   PUSH_POP immediate: [2:0] = stack_id, [4:3] = mode
    pub fn src_pop(mut self, stack_id: u8, mode: RegMode) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(mode != RegMode::Deref, "DEREF mode not applicable to stacks");
        self.src_unit = Unit::UNIT_STACK_PUSH_POP;
        self.si = (stack_id as u16) | ((mode as u16 & 0x3) << 3);
        self
    }

    /// Set source to a stack peek (indexed read) with the given tag mode.
    ///   INDEX immediate: [2:0] = stack_id, [8:3] = offset, [10:9] = mode
    pub fn src_peek(mut self, stack_id: u8, offset: u8, mode: RegMode) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 64, "Stack offset must be 0-63");
        assert!(mode != RegMode::Deref, "DEREF mode not applicable to stacks");
        self.src_unit = Unit::UNIT_STACK_INDEX;
        self.si = (stack_id as u16) | ((offset as u16) << 3) | ((mode as u16 & 0x3) << 9);
        self
    }

    // --- Sub-word memory access helpers ---
    //
    // Sub-word (byte/halfword) access is supported on MEMORY_OPERAND and
    // REGISTER_POINTER only. MEMORY_IMMEDIATE always performs full-word
    // access because it uses the full 12-bit immediate as a word address.

    /// Set the source to a memory-operand load (32-bit address) with the
    /// given access width and byte offset within the word.
    pub fn src_mem_op(mut self, addr: u32, width: AccessWidth, byte_offset: u8) -> Self {
        self.src_unit = Unit::UNIT_MEMORY_OPERAND;
        self.si = encode_mem_immediate(0, width, byte_offset);
        self.soperand = Some(addr);
        self
    }

    /// Set the destination to a memory-operand store (32-bit address) with
    /// the given access width and byte offset within the word.
    pub fn dst_mem_op(mut self, addr: u32, width: AccessWidth, byte_offset: u8) -> Self {
        self.dst_unit = Unit::UNIT_MEMORY_OPERAND;
        self.di = encode_mem_immediate(0, width, byte_offset);
        self.doperand = Some(addr);
        self
    }

    /// Set the source to a register-pointer load (word access).
    /// Uses REGISTER DEREF mode. For sub-word access, load the address
    /// into a register first and use MEMORY_OPERAND.
    pub fn src_reg_ptr(self, reg: u16, _width: AccessWidth, _byte_offset: u8) -> Self {
        self.src_deref(reg, 0)
    }

    /// Set the destination to a register-pointer store (word access).
    /// Uses REGISTER DEREF mode.
    pub fn dst_reg_ptr(self, reg: u16, _width: AccessWidth, _byte_offset: u8) -> Self {
        self.dst_deref(reg, 0)
    }

    // --- Tagged register access helpers ---

    /// Set source to a register read with the given access mode.
    pub fn src_reg(mut self, reg: u16, mode: RegMode) -> Self {
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = encode_reg_immediate(reg, mode, 0);
        self
    }

    /// Set destination to a register write with the given access mode.
    pub fn dst_reg(mut self, reg: u16, mode: RegMode) -> Self {
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = encode_reg_immediate(reg, mode, 0);
        self
    }

    /// Set source to a DEREF register read (strip tag, load from memory
    /// at untagged address + word offset).
    pub fn src_deref(mut self, reg: u16, offset: u8) -> Self {
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = encode_reg_immediate(reg, RegMode::Deref, offset);
        self
    }

    /// Set destination to a DEREF register write (strip tag, store to
    /// memory at untagged address + word offset).
    pub fn dst_deref(mut self, reg: u16, offset: u8) -> Self {
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = encode_reg_immediate(reg, RegMode::Deref, offset);
        self
    }

    pub fn stack_poke(mut self, stack_id: u8, offset: u8, src_reg: u16) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 64, "Stack offset must be 0-63");
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = src_reg;
        self.dst_unit = Unit::UNIT_STACK_INDEX;
        // Pack stack_id in bits 2:0, offset in bits 8:3
        self.di = (stack_id as u16) | ((offset as u16) << 3);
        self
    }
}

// Convenience function to match C++ style
pub fn instr() -> Instr {
    Instr::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_instruction_encoding() {
        // Test a simple register-to-register move
        let instr = instr()
            .src(Unit::UNIT_REGISTER)
            .si(5) // source register 5
            .dst(Unit::UNIT_REGISTER)
            .di(10); // destination register 10

        let assembled = instr.assemble();
        assert_eq!(assembled.len(), 1); // No operands needed

        // Verify bit packing
        let packed = assembled[0];
        assert_eq!(packed & 0xF, Unit::UNIT_REGISTER as u32); // src_unit
        assert_eq!((packed >> 4) & 0xFFF, 5); // si
        assert_eq!((packed >> 16) & 0xF, Unit::UNIT_REGISTER as u32); // dst_unit
        assert_eq!((packed >> 20) & 0xFFF, 10); // di
    }

    #[test]
    fn test_instruction_with_operands() {
        // Test instruction with both operands
        let instr = instr()
            .src(Unit::UNIT_MEMORY_OPERAND)
            .soperand(0x1234)
            .dst(Unit::UNIT_MEMORY_OPERAND)
            .doperand(0x5678);

        let assembled = instr.assemble();
        assert_eq!(assembled.len(), 3); // Base instruction + 2 operands
        assert_eq!(assembled[1], 0x1234); // soperand
        assert_eq!(assembled[2], 0x5678); // doperand
    }
}
