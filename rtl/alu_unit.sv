`include "common.vh"

// One ALU lane. execute.sv instantiates 8 lanes and selects them by index.
//
// The ALU is *passive* — its left operand (a), right operand (b), and
// operator are written independently by prior move instructions. When
// sel_i is asserted the result is computed and latched into data_o,
// becoming readable on the following cycle via UNIT_ALU_RESULT.
module alu_unit (
    input wire rst_i,               // Synchronous reset (active high)
    input wire clk_i,               // System clock
    input wire sel_i,               // Compute enable — latch result when high
    input ALU_OPERATOR oper_i,      // Operation selector (see ALU_OPERATOR enum)
    input  logic [31:0] a_data_i,   // Left operand (A)
    input  logic [31:0] b_data_i,   // Right operand (B)
    output logic [31:0] data_o      // Result (registered — available next cycle)
);

  always @(posedge clk_i) begin
    if (rst_i) begin
      data_o <= 32'b0;
    end else if (sel_i) begin
      // Results are registered, so ALU reads are visible on a later cycle.
      case (oper_i)
        ALU_NOP: data_o <= 32'b0;
        ALU_ADD: data_o <= a_data_i + b_data_i;
        ALU_SUB: data_o <= a_data_i - b_data_i;
        ALU_DIV: data_o <= a_data_i / b_data_i;
        ALU_MUL: data_o <= a_data_i * b_data_i;
        ALU_MOD: data_o <= a_data_i % b_data_i;
        // Comparisons zero-extend a 1-bit result to 32 bits.
        ALU_EQL: data_o <= {31'b0, (a_data_i == b_data_i)};
        // Shifts use only the low 5 bits of B (shift amount 0-31).
        ALU_SL:  data_o <= a_data_i << b_data_i[4:0];
        ALU_SR:  data_o <= a_data_i >> b_data_i[4:0];
        ALU_SRA: data_o <= a_data_i >>> b_data_i[4:0];
        // NOT is unary — only A is used; B is ignored.
        ALU_NOT: data_o <= ~a_data_i;
        ALU_AND: data_o <= a_data_i & b_data_i;
        ALU_OR:  data_o <= a_data_i | b_data_i;
        ALU_XOR: data_o <= a_data_i ^ b_data_i;
        ALU_GT:  data_o <= {31'b0, (a_data_i > b_data_i)};
        ALU_LT:  data_o <= {31'b0, (a_data_i < b_data_i)};
        default: data_o <= 32'b0;
      endcase
    end
  end
endmodule : alu_unit
