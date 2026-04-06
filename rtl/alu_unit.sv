`include "common.vh"

// One ALU lane. execute.sv instantiates 8 lanes and selects them by index.
//
// The ALU is *passive* — its left operand (a), right operand (b), and
// operator are written independently by prior move instructions. The
// result is available combinationally on data_raw_o at all times,
// computed from the current operands and operator. No sel_i or clock
// delay is needed to read the result.
module alu_unit (
    input wire rst_i,               // Synchronous reset (active high)
    input wire clk_i,               // System clock
    input ALU_OPERATOR oper_i,      // Operation selector (see ALU_OPERATOR enum)
    input  logic [31:0] a_data_i,   // Left operand (A)
    input  logic [31:0] b_data_i,   // Right operand (B)
    output wire  [31:0] data_raw_o  // Combinational result — always valid
);

  // Result is pure combinational logic from stored operands + operator.
  function automatic logic [31:0] compute(
    ALU_OPERATOR op, logic [31:0] a, logic [31:0] b
  );
    case (op)
      ALU_NOP: return 32'b0;
      ALU_ADD: return a + b;
      ALU_SUB: return a - b;
      ALU_DIV: return a / b;
      ALU_MUL: return a * b;
      ALU_MOD: return a % b;
      // Comparisons zero-extend a 1-bit result to 32 bits.
      ALU_EQL: return {31'b0, (a == b)};
      // Shifts use only the low 5 bits of B (shift amount 0-31).
      ALU_SL:  return a << b[4:0];
      ALU_SR:  return a >> b[4:0];
      ALU_SRA: return a >>> b[4:0];
      // NOT is unary — only A is used; B is ignored.
      ALU_NOT: return ~a;
      ALU_AND: return a & b;
      ALU_OR:  return a | b;
      ALU_XOR: return a ^ b;
      ALU_GT:  return {31'b0, (a > b)};
      ALU_LT:  return {31'b0, (a < b)};
      default: return 32'b0;
    endcase
  endfunction

  assign data_raw_o = compute(oper_i, a_data_i, b_data_i);

endmodule : alu_unit
