`ifndef common_vh_
`define common_vh_

typedef enum bit [3:0] {
    ALU_NOP = 4'h000,
    ALU_ADD = 4'h001,
    ALU_SUB = 4'h002,
    ALU_MUL = 4'h003,
    ALU_DIV = 4'h004,
    ALU_MOD = 4'h005,
    ALU_EQL = 4'h006,
    ALU_SL = 4'h007,
    ALU_SR = 4'h008,
    ALU_SRA = 4'h009,
    ALU_NOT = 4'h00a,
    ALU_AND = 4'h00b,
    ALU_OR = 4'h00c,
    ALU_XOR = 4'h00d,
    ALU_GT = 4'h00e,
    ALU_LT = 4'h00f
} ALU_OPERATOR;

typedef enum bit[3:0] {
    UNIT_NONE = 0,
    UNIT_STACK_PUSH_POP = 1,  // TODO: Not implemented yet
    UNIT_STACK_INDEX = 2,     // TODO: Not implemented yet
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
    UNIT_REGISTER_POINTER = 13  // Value of memory address in register N
} Unit;

`endif  // common_vh_
