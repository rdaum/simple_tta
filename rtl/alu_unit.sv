`include "common.vh"

// One ALU lane. execute.sv instantiates 8 lanes and selects them by index.
//
// The ALU is *passive* — its left operand (a), right operand (b), and
// operator are written independently by prior move instructions. The
// result is available combinationally on data_raw_o at all times,
// computed from the current operands and operator. No sel_i or clock
// delay is needed to read the result.
module alu_unit (
    input logic [3:0] oper_i,       // Operation selector (ALU_OPERATOR enum)
    input  logic [31:0] a_data_i,   // Left operand (A)
    input  logic [31:0] b_data_i,   // Right operand (B)
    output wire  [31:0] data_raw_o  // Combinational result — always valid
);

  // Result is pure combinational logic from stored operands + operator.
  reg [31:0] result;
  assign data_raw_o = result;

  always_comb begin
    case (oper_i)
      ALU_NOP: result = 32'b0;
      ALU_ADD: result = a_data_i + b_data_i;
      ALU_SUB: result = a_data_i - b_data_i;
      ALU_DIV: result = 32'b0;  // handled by muldiv_unit
      ALU_MUL: result = 32'b0;  // handled by muldiv_unit
      ALU_MOD: result = 32'b0;  // handled by muldiv_unit
      ALU_EQL: result = {31'b0, (a_data_i == b_data_i)};
      ALU_SL:  result = a_data_i << b_data_i[4:0];
      ALU_SR:  result = a_data_i >> b_data_i[4:0];
      ALU_SRA: result = a_data_i >>> b_data_i[4:0];
      ALU_NOT: result = ~a_data_i;
      ALU_AND: result = a_data_i & b_data_i;
      ALU_OR:  result = a_data_i | b_data_i;
      ALU_XOR: result = a_data_i ^ b_data_i;
      ALU_GT:  result = {31'b0, (a_data_i > b_data_i)};
      ALU_LT:  result = {31'b0, (a_data_i < b_data_i)};
      default: result = 32'b0;
    endcase
  end

endmodule
