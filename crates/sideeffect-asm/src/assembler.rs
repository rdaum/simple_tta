//! TTA assembler — instruction encoding for the sideeffect processor.
//!
//! Instruction format (32 bits):
//!   [4:0]    src_unit   (5-bit unit selector, 32 slots)
//!   [9:5]    dst_unit   (5-bit unit selector, 32 slots)
//!   [17:10]  si         (8-bit source immediate)
//!   [25:18]  di         (8-bit destination immediate)
//!   [27:26]  predicate  (2 bits: [26]=if_set, [27]=if_clear)
//!   [31:28]  reserved   (4 bits)

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
    UNIT_REGISTER = 3,       // raw read/write
    UNIT_REG_VALUE = 4,      // read: tag bits zeroed; write: preserve tag
    UNIT_REG_TAG = 5,        // read: tag bits only; write: preserve payload
    UNIT_REG_DEREF = 6,      // strip tag, add word offset, load/store via bus
    UNIT_ALU_LEFT = 7,
    UNIT_ALU_RIGHT = 8,
    UNIT_ALU_OPERATOR = 9,
    UNIT_ALU_RESULT = 10,
    UNIT_MEMORY_IMMEDIATE = 11,
    UNIT_MEMORY_OPERAND = 12,
    UNIT_ABS_IMMEDIATE = 13,
    UNIT_ABS_OPERAND = 14,
    UNIT_PC = 15,
    UNIT_PC_COND = 16,
    UNIT_COND = 17,
    UNIT_WRITE_BARRIER = 18,
    UNIT_MEM_BYTE = 19,         // byte load/store; 32-bit addr in operand, imm[1:0] = byte offset
    UNIT_STACK_POP_VALUE = 20,  // pop with VALUE mode (tag bits zeroed)
    UNIT_STACK_POP_TAG = 21,    // pop with TAG mode (tag bits only)
    UNIT_STACK_PEEK_VALUE = 22, // peek with VALUE mode (tag bits zeroed)
    UNIT_STACK_PEEK_TAG = 23,   // peek with TAG mode (tag bits only)
    UNIT_TAG_CMP = 24,          // dest only: set cond = (src tag == imm[3:0])
    UNIT_ALLOC = 25,            // dest only: store value at heap_ptr, heap_ptr++
    UNIT_ALLOC_PTR = 26,        // src only: read {si[3:0] as tag, heap_ptr}
    UNIT_CALL = 27,             // dest only: push return addr to stack 1, jump to value
    UNIT_MAILBOX = 28,          // src: block until host writes; dst: write to host
}

impl Unit {
    fn needs_operand(self) -> bool {
        matches!(
            self,
            Unit::UNIT_MEMORY_OPERAND | Unit::UNIT_ABS_OPERAND | Unit::UNIT_MEM_BYTE
        )
    }
}

#[derive(Debug, Clone)]
pub struct Instr {
    src_unit: Unit,
    si: u8,
    dst_unit: Unit,
    di: u8,
    flags: u8, // bits [5:0] used; [0]=pred_if_set, [1]=pred_if_clear
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
            flags: 0,
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

    pub fn si(mut self, immediate: u8) -> Self {
        self.si = immediate;
        self
    }

    pub fn di(mut self, immediate: u8) -> Self {
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

    /// Execute only if condition register is set.
    pub fn predicate_if_set(mut self) -> Self {
        self.flags |= 1 << 0;
        self
    }

    /// Execute only if condition register is clear.
    pub fn predicate_if_clear(mut self) -> Self {
        self.flags |= 1 << 1;
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

        // Pack instruction:
        //   [4:0]   src_unit
        //   [9:5]   dst_unit
        //   [17:10] si
        //   [25:18] di
        //   [31:26] flags (predicate in [27:26], reserved [31:28])
        let packed = ((self.src_unit as u32) & 0x1F)
            | (((self.dst_unit as u32) & 0x1F) << 5)
            | (((self.si as u32) & 0xFF) << 10)
            | (((self.di as u32) & 0xFF) << 18)
            | (((self.flags as u32) & 0x3F) << 26);

        let mut result = vec![packed];

        if let Some(sop) = self.soperand {
            result.push(sop);
        }

        if let Some(dop) = self.doperand {
            result.push(dop);
        }

        result
    }

    // --- Register access helpers ---

    /// Set source to a register read (raw — full 32-bit value).
    pub fn src_reg(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = reg;
        self
    }

    /// Set destination to a register write (raw — full 32-bit value).
    pub fn dst_reg(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = reg;
        self
    }

    /// Set source to a register VALUE read (tag bits zeroed).
    pub fn src_reg_value(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_REG_VALUE;
        self.si = reg;
        self
    }

    /// Set destination to a register VALUE write (preserve tag, set payload).
    pub fn dst_reg_value(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.dst_unit = Unit::UNIT_REG_VALUE;
        self.di = reg;
        self
    }

    /// Set source to a register TAG read (tag bits only).
    pub fn src_reg_tag(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_REG_TAG;
        self.si = reg;
        self
    }

    /// Set destination to a register TAG write (preserve payload, set tag).
    pub fn dst_reg_tag(mut self, reg: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        self.dst_unit = Unit::UNIT_REG_TAG;
        self.di = reg;
        self
    }

    /// Set source to a DEREF register read (strip tag, load from memory
    /// at untagged address + word offset).
    pub fn src_deref(mut self, reg: u8, offset: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        assert!(offset < 8, "DEREF word offset must be 0-7");
        self.src_unit = Unit::UNIT_REG_DEREF;
        self.si = (reg & 0x1F) | ((offset & 0x7) << 5);
        self
    }

    /// Set destination to a DEREF register write (strip tag, store to
    /// memory at untagged address + word offset).
    pub fn dst_deref(mut self, reg: u8, offset: u8) -> Self {
        assert!(reg < 32, "Register index must be 0-31");
        assert!(offset < 8, "DEREF word offset must be 0-7");
        self.dst_unit = Unit::UNIT_REG_DEREF;
        self.di = (reg & 0x1F) | ((offset & 0x7) << 5);
        self
    }

    // --- Stack operation helpers ---

    pub fn push_reg(mut self, stack_id: u8, src_reg: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(src_reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = src_reg;
        self.dst_unit = Unit::UNIT_STACK_PUSH_POP;
        self.di = stack_id;
        self
    }

    pub fn pop_to_reg(mut self, stack_id: u8, dst_reg: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(dst_reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_STACK_PUSH_POP;
        self.si = stack_id;
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
        self.di = stack_id;
        self
    }

    pub fn stack_peek(mut self, stack_id: u8, offset: u8, dst_reg: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 32, "Stack offset must be 0-31");
        assert!(dst_reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_STACK_INDEX;
        // Pack stack_id in bits [2:0], offset in bits [7:3]
        self.si = (stack_id) | (offset << 3);
        self.dst_unit = Unit::UNIT_REGISTER;
        self.di = dst_reg;
        self
    }

    pub fn stack_poke(mut self, stack_id: u8, offset: u8, src_reg: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 32, "Stack offset must be 0-31");
        assert!(src_reg < 32, "Register index must be 0-31");
        self.src_unit = Unit::UNIT_REGISTER;
        self.si = src_reg;
        self.dst_unit = Unit::UNIT_STACK_INDEX;
        // Pack stack_id in bits [2:0], offset in bits [7:3]
        self.di = (stack_id) | (offset << 3);
        self
    }

    /// Set source to a stack pop (raw value).
    pub fn src_pop(mut self, stack_id: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_STACK_PUSH_POP;
        self.si = stack_id;
        self
    }

    /// Set source to a stack peek at offset (raw value).
    pub fn src_peek(mut self, stack_id: u8, offset: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 32, "Stack offset must be 0-31");
        self.src_unit = Unit::UNIT_STACK_INDEX;
        self.si = (stack_id) | (offset << 3);
        self
    }

    // --- Memory access helpers ---

    /// Set source to a memory operand load (32-bit address, word access).
    pub fn src_mem_op(mut self, addr: u32) -> Self {
        self.src_unit = Unit::UNIT_MEMORY_OPERAND;
        self.si = 0;
        self.soperand = Some(addr);
        self
    }

    /// Set destination to a memory operand store (32-bit address, word access).
    pub fn dst_mem_op(mut self, addr: u32) -> Self {
        self.dst_unit = Unit::UNIT_MEMORY_OPERAND;
        self.di = 0;
        self.doperand = Some(addr);
        self
    }

    /// Set source to a byte load (32-bit address, imm[1:0] = byte offset).
    pub fn src_mem_byte(mut self, addr: u32, byte_offset: u8) -> Self {
        assert!(byte_offset < 4, "Byte offset must be 0-3");
        self.src_unit = Unit::UNIT_MEM_BYTE;
        self.si = byte_offset & 0x3;
        self.soperand = Some(addr);
        self
    }

    /// Set destination to a byte store (32-bit address, imm[1:0] = byte offset).
    pub fn dst_mem_byte(mut self, addr: u32, byte_offset: u8) -> Self {
        assert!(byte_offset < 4, "Byte offset must be 0-3");
        self.dst_unit = Unit::UNIT_MEM_BYTE;
        self.di = byte_offset & 0x3;
        self.doperand = Some(addr);
        self
    }

    // --- Tagged stack access helpers ---

    /// Pop with VALUE mode (tag bits zeroed).
    pub fn src_pop_value(mut self, stack_id: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_STACK_POP_VALUE;
        self.si = stack_id;
        self
    }

    /// Pop with TAG mode (tag bits only).
    pub fn src_pop_tag(mut self, stack_id: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        self.src_unit = Unit::UNIT_STACK_POP_TAG;
        self.si = stack_id;
        self
    }

    /// Peek with VALUE mode (tag bits zeroed).
    pub fn src_peek_value(mut self, stack_id: u8, offset: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 32, "Stack offset must be 0-31");
        self.src_unit = Unit::UNIT_STACK_PEEK_VALUE;
        self.si = stack_id | (offset << 3);
        self
    }

    /// Peek with TAG mode (tag bits only).
    pub fn src_peek_tag(mut self, stack_id: u8, offset: u8) -> Self {
        assert!(stack_id < 8, "Stack ID must be 0-7");
        assert!(offset < 32, "Stack offset must be 0-31");
        self.src_unit = Unit::UNIT_STACK_PEEK_TAG;
        self.si = stack_id | (offset << 3);
        self
    }

    /// Set destination to tag compare: sets cond = (src_value tag == expected_tag).
    pub fn dst_tag_cmp(mut self, expected_tag: u8) -> Self {
        assert!(expected_tag < 16, "Tag must be 0-15");
        self.dst_unit = Unit::UNIT_TAG_CMP;
        self.di = expected_tag;
        self
    }

    // --- Allocation helpers ---

    /// Set destination to ALLOC: store value at heap_ptr, then heap_ptr++.
    pub fn dst_alloc(mut self) -> Self {
        self.dst_unit = Unit::UNIT_ALLOC;
        self
    }

    /// Set source to ALLOC_PTR: read current heap_ptr with the given tag.
    /// Returns a tagged pointer: {tag[3:0], heap_ptr}.
    pub fn src_alloc_ptr(mut self, tag: u8) -> Self {
        assert!(tag < 16, "Tag must be 0-15");
        self.src_unit = Unit::UNIT_ALLOC_PTR;
        self.si = tag;
        self
    }

    // --- Call/return helpers ---

    /// Set destination to CALL: push return address to stack 1, jump to src value.
    pub fn dst_call(mut self) -> Self {
        self.dst_unit = Unit::UNIT_CALL;
        self
    }

    // --- Mailbox helpers ---

    /// Set source to MAILBOX: blocks until host writes a value.
    pub fn src_mailbox(mut self) -> Self {
        self.src_unit = Unit::UNIT_MAILBOX;
        self
    }

    /// Set destination to MAILBOX: write value to host-readable output.
    pub fn dst_mailbox(mut self) -> Self {
        self.dst_unit = Unit::UNIT_MAILBOX;
        self
    }
}

/// Convenience function to create a new instruction builder.
pub fn instr() -> Instr {
    Instr::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_instruction_encoding() {
        let instr = instr()
            .src(Unit::UNIT_REGISTER)
            .si(5)
            .dst(Unit::UNIT_REGISTER)
            .di(10);

        let assembled = instr.assemble();
        assert_eq!(assembled.len(), 1);

        let packed = assembled[0];
        assert_eq!(packed & 0x1F, Unit::UNIT_REGISTER as u32); // src_unit
        assert_eq!((packed >> 5) & 0x1F, Unit::UNIT_REGISTER as u32); // dst_unit
        assert_eq!((packed >> 10) & 0xFF, 5); // si
        assert_eq!((packed >> 18) & 0xFF, 10); // di
        assert_eq!((packed >> 26) & 0x3F, 0); // flags
    }

    #[test]
    fn test_instruction_with_operands() {
        let instr = instr()
            .src(Unit::UNIT_MEMORY_OPERAND)
            .soperand(0x1234)
            .dst(Unit::UNIT_MEMORY_OPERAND)
            .doperand(0x5678);

        let assembled = instr.assemble();
        assert_eq!(assembled.len(), 3);
        assert_eq!(assembled[1], 0x1234);
        assert_eq!(assembled[2], 0x5678);
    }

    #[test]
    fn test_predication_encoding() {
        let instr = instr()
            .src(Unit::UNIT_ABS_IMMEDIATE)
            .si(42)
            .dst(Unit::UNIT_REGISTER)
            .di(0)
            .predicate_if_set();

        let assembled = instr.assemble();
        let packed = assembled[0];
        assert_eq!((packed >> 26) & 0x1, 1); // pred_if_set
        assert_eq!((packed >> 27) & 0x1, 0); // pred_if_clear
    }
}
