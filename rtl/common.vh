`ifndef common_vh_
`define common_vh_

// Shared instruction encoding enums used across fetch, decode, execute,
// and mirrored in the Rust assembler implementation (assembler.rs).
//
// Instruction format (32 bits):
//   [4:0]    src_unit   (5-bit unit selector, 32 slots)
//   [9:5]    dst_unit   (5-bit unit selector, 32 slots)
//   [17:10]  si         (8-bit source immediate)
//   [25:18]  di         (8-bit destination immediate)
//   [27:26]  predicate  (2 bits: [26]=if_set, [27]=if_clear)
//   [31:28]  reserved   (4 bits)

// ALU operation selector. Stored internally in each ALU lane and applied
// when the lane's result is read. Comparisons (EQL, GT, LT) produce a
// 1-bit result zero-extended to 32 bits. Shifts use only the low 5 bits
// of the B operand (i.e. shift amount 0-31).
typedef enum bit [3:0] {
    ALU_NOP = 4'h000,   // No operation — output zero
    ALU_ADD = 4'h001,   // A + B
    ALU_SUB = 4'h002,   // A - B
    ALU_MUL = 4'h003,   // A * B  (multi-cycle, via muldiv_unit)
    ALU_DIV = 4'h004,   // A / B  (multi-cycle, via muldiv_unit)
    ALU_MOD = 4'h005,   // A % B  (multi-cycle, via muldiv_unit)
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

// Transport unit selector (5-bit). Each instruction carries a source and
// destination unit. The execute stage routes data from source to destination.
//
// Immediate field (8 bits) interpretation depends on the unit:
//   - REG/REG_VALUE/REG_TAG: [4:0] = register index (0-31)
//   - REG_DEREF: [4:0] = register index, [7:5] = word offset (0-7)
//   - ALU units: [2:0] = lane index (0-7)
//   - STACK push/pop: [2:0] = stack ID (0-7)
//   - STACK_INDEX: [2:0] = stack ID, [7:3] = offset (0-31)
//   - MEM_IMM: full 8 bits = word address (0-255)
//   - IMM: full 8 bits = literal value (0-255)
//   - OPERAND/MEM_OP: immediate unused; 32-bit value in next word
typedef enum bit[4:0] {
    UNIT_NONE             = 0,   // No-op: source yields 0, destination discards
    UNIT_STACK_PUSH_POP   = 1,   // Source: pop; Destination: push
    UNIT_STACK_INDEX      = 2,   // Source: peek at offset; Destination: poke at offset
    UNIT_REGISTER         = 3,   // Raw read/write register N (imm[4:0])
    UNIT_REG_VALUE        = 4,   // Read: tag bits zeroed; Write: preserve tag, set payload
    UNIT_REG_TAG          = 5,   // Read: tag bits only; Write: preserve payload, set tag
    UNIT_REG_DEREF        = 6,   // Strip tag, add word offset, load/store via data bus
    UNIT_ALU_LEFT         = 7,   // ALU lane N left operand (imm[2:0])
    UNIT_ALU_RIGHT        = 8,   // ALU lane N right operand (imm[2:0])
    UNIT_ALU_OPERATOR     = 9,   // Set ALU lane N operator (imm[2:0])
    UNIT_ALU_RESULT       = 10,  // Read ALU lane N result (imm[2:0])
    UNIT_MEMORY_IMMEDIATE = 11,  // Memory at 8-bit word address (0-255)
    UNIT_MEMORY_OPERAND   = 12,  // Memory at full 32-bit address (next program word)
    UNIT_ABS_IMMEDIATE    = 13,  // Literal 8-bit value (0-255, zero-extended)
    UNIT_ABS_OPERAND      = 14,  // Literal 32-bit value (next program word)
    UNIT_PC               = 15,  // Program counter (source: read; dest: jump)
    UNIT_PC_COND          = 16,  // Destination only: jump if condition register is set
    UNIT_COND             = 17,  // Source: read condition (0 or 1); Dest: set condition
    UNIT_WRITE_BARRIER    = 18,  // Source: pop barrier FIFO; Dest: push to barrier FIFO
    UNIT_MEM_BYTE         = 19,  // Byte load/store; 32-bit address in operand, imm[1:0] = byte offset
    UNIT_STACK_POP_VALUE  = 20,  // Pop with VALUE mode (tag bits zeroed)
    UNIT_STACK_POP_TAG    = 21,  // Pop with TAG mode (tag bits only)
    UNIT_STACK_PEEK_VALUE = 22,  // Peek with VALUE mode (tag bits zeroed)
    UNIT_STACK_PEEK_TAG   = 23,  // Peek with TAG mode (tag bits only)
    UNIT_TAG_CMP          = 24,  // Dest only: set cond = (src tag == imm[3:0])
    UNIT_ALLOC            = 25,  // Dest only: store value at heap_ptr, heap_ptr++
    UNIT_ALLOC_PTR        = 26,  // Src only: read {si[3:0] as tag, heap_ptr} (tagged pointer)
    UNIT_CALL             = 27,  // Dest only: push return addr to stack 1, jump to value
    UNIT_MAILBOX          = 28   // Src: block until host writes (handshake); Dst: write to host
} Unit;

// Predicate flag bit positions within the 6-bit flags field.
localparam PRED_IF_SET   = 0;  // flags[0]: execute only if cond_reg == 1
localparam PRED_IF_CLEAR = 1;  // flags[1]: execute only if cond_reg == 0

// Tagged data width. Every register, stack slot, and memory word is
// DATA_WIDTH bits: a VAL_WIDTH-bit value plus a TAG_WIDTH-bit sidecar tag.
// The tag is stored in the high bits [DATA_WIDTH-1:VAL_WIDTH], NOT in
// the low bits of the value — no masking is ever needed on addresses.
localparam int TAG_WIDTH  = 4;   // 4-bit tag → 16 types
localparam int VAL_WIDTH  = 32;  // 32-bit value (addresses, integers)
localparam int DATA_WIDTH = VAL_WIDTH + TAG_WIDTH;  // 36-bit tagged word

`endif  // common_vh_
