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
    UNIT_RESERVED_13      = 13,  // Reserved (was REGISTER_POINTER, now use REGISTER DEREF mode)
    UNIT_COND             = 14,  // Source: read condition (0 or 1); Dest: set condition (nonzero = true)
    UNIT_PC_COND          = 15   // Destination only: jump to src_value if condition register is set
} Unit;

// Access width selector, carried in immediate bits [11:10] of MEMORY_OPERAND
// and REGISTER_POINTER instructions. MEMORY_IMMEDIATE always performs
// full-word access (the full 12-bit immediate is used as a word address).
//
// For MEMORY_OPERAND and REGISTER_POINTER the 12-bit immediate is:
//   [11:10] — access width (AccessWidth enum below)
//   [9:8]   — byte offset within the 32-bit word (0-3 for byte, 0/2 for half)
//   [7:0]   — unused for MEMORY_OPERAND; [4:0] = register index for REG_PTR
//
// On writes, the width + offset select which byte lanes are strobed.
// On reads, the selected bytes are zero-extended to 32 bits.
typedef enum bit [1:0] {
    ACCESS_WORD     = 2'b00,  // Full 32-bit word (offset ignored, wstrb = 4'b1111)
    ACCESS_BYTE     = 2'b01,  // Single byte (offset selects lane 0-3)
    ACCESS_HALFWORD = 2'b10,  // 16-bit halfword (offset 0 = lanes 1:0, offset 2 = lanes 3:2)
    ACCESS_RESERVED = 2'b11   // Reserved for future use
} AccessWidth;

// Tag configuration for tagged-value support. The tag occupies the low
// TAG_WIDTH bits of every 32-bit value. Registers, stacks, and memory
// all store tagged values; the register access mode (below) controls
// how the tag is handled during reads and writes.
localparam int TAG_WIDTH = 2;
localparam logic [31:0] TAG_MASK_32 = (1 << TAG_WIDTH) - 1;  // 32'h0000_0003

// Register access mode, encoded in immediate bits [6:5] of UNIT_REGISTER
// instructions. Controls how the register value is interpreted.
//
// Immediate layout for UNIT_REGISTER:
//   [4:0]  — register index (0-31)
//   [6:5]  — access mode (RegAccessMode below)
//   [9:7]  — word offset for DEREF mode (0-7)
//   [11:10] — unused
typedef enum bit [1:0] {
    REG_RAW   = 2'b00,  // Full 32-bit read/write (backward compatible)
    REG_VALUE = 2'b01,  // Read: tag bits zeroed; Write: preserve tag, set payload
    REG_TAG   = 2'b10,  // Read: tag bits only; Write: preserve payload, set tag
    REG_DEREF = 2'b11   // Strip tag, add word offset, load/store via data bus
} RegAccessMode;

`endif  // common_vh_
