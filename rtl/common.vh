`ifndef common_vh_
`define common_vh_

// Shared instruction encoding enums used across fetch, decode, execute,
// and mirrored in the Rust assembler implementation (src/assembler.rs).

// ALU operation selector. Stored internally in each ALU lane and applied
// when the lane's sel_i is asserted. Comparisons (EQL, GT, LT) produce a
// 1-bit result zero-extended to 32 bits. Shifts use only the low 5 bits
// of the B operand (i.e. shift amount 0-31).
typedef enum bit [3:0] {
    ALU_NOP = 4'h000,   // No operation — output zero
    ALU_ADD = 4'h001,   // A + B
    ALU_SUB = 4'h002,   // A - B
    ALU_MUL = 4'h003,   // A * B  (lower 32 bits)
    ALU_DIV = 4'h004,   // A / B  (unsigned)
    ALU_MOD = 4'h005,   // A % B  (unsigned)
    ALU_EQL = 4'h006,   // A == B  (0 or 1)
    ALU_SL  = 4'h007,   // A << B[4:0]  (logical left)
    ALU_SR  = 4'h008,   // A >> B[4:0]  (logical right)
    ALU_SRA = 4'h009,   // A >>> B[4:0] (arithmetic right)
    ALU_NOT = 4'h00a,   // ~A  (bitwise NOT, B ignored)
    ALU_AND = 4'h00b,   // A & B
    ALU_OR  = 4'h00c,   // A | B
    ALU_XOR = 4'h00d,   // A ^ B
    ALU_GT  = 4'h00e,   // A > B   (0 or 1)
    ALU_LT  = 4'h00f    // A < B   (0 or 1)
} ALU_OPERATOR;

// Transport unit selector. Each instruction word carries a 4-bit source unit
// and a 4-bit destination unit. The execute stage routes data from the source
// to the destination in a single move.
//
// Immediate fields are 12 bits embedded in the instruction word. Their
// interpretation depends on the unit:
//   - Register units use bits [4:0] as a register index (0-31).
//   - ALU units use bits [2:0] as a lane index (0-7).
//   - Stack push/pop uses bits [2:0] as a stack ID (0-7).
//   - Stack index uses bits [2:0] as stack ID, bits [8:3] as offset (0-63).
//   - Memory immediate zero-extends the 12-bit field to a 32-bit address.
//   - Operand variants consume a full 32-bit word from the instruction stream.
typedef enum bit[3:0] {
    UNIT_NONE             = 0,   // No-op: source yields 0, destination discards
    UNIT_STACK_PUSH_POP   = 1,   // Source: pop; Destination: push
    UNIT_STACK_INDEX      = 2,   // Source: peek at offset; Destination: poke at offset
    UNIT_REGISTER         = 3,   // Read/write register N (imm[4:0])
    UNIT_ALU_LEFT         = 4,   // ALU lane N left operand (imm[2:0])
    UNIT_ALU_RIGHT        = 5,   // ALU lane N right operand (imm[2:0])
    UNIT_ALU_OPERATOR     = 6,   // Set ALU lane N operator (imm[2:0])
    UNIT_ALU_RESULT       = 7,   // Read ALU lane N result (imm[2:0])
    UNIT_MEMORY_IMMEDIATE = 8,   // Memory at zero-extended 12-bit address
    UNIT_MEMORY_OPERAND   = 9,   // Memory at full 32-bit address (next program word)
    UNIT_PC               = 10,  // Program counter (source: read; dest: jump)
    UNIT_ABS_IMMEDIATE    = 11,  // Literal 12-bit immediate value (zero-extended)
    UNIT_ABS_OPERAND      = 12,  // Literal 32-bit value (next program word)
    UNIT_REGISTER_POINTER = 13   // Memory at address held in register N (imm[4:0])
} Unit;

`endif  // common_vh_
