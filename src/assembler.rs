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
    UNIT_REGISTER_POINTER = 13,
}

impl Unit {
    fn needs_operand(self) -> bool {
        matches!(self, Unit::UNIT_MEMORY_OPERAND | Unit::UNIT_ABS_OPERAND)
    }
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
