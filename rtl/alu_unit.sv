`include "common.vh"

module alu_unit (
    input wire rst_i,
    input wire clk_i,
    input wire sel_i,
    input ALU_OPERATOR oper_i,

    input  logic [31:0] a_data_i,
    input  logic [31:0] b_data_i,
    output logic [31:0] data_o
);

  always @(posedge clk_i) begin
    if (rst_i) begin
      data_o <= 32'b0;
    end else if (sel_i) begin
      case (oper_i)
        ALU_NOP: data_o <= 32'b0;
        ALU_ADD: data_o <= a_data_i + b_data_i;
        ALU_SUB: data_o <= a_data_i - b_data_i;
        ALU_DIV: data_o <= a_data_i / b_data_i;
        ALU_MUL: data_o <= a_data_i * b_data_i;
        ALU_MOD: data_o <= a_data_i % b_data_i;
        ALU_EQL: data_o <= {31'b0, (a_data_i == b_data_i)};
        ALU_SL:  data_o <= a_data_i << b_data_i[4:0];
        ALU_SR:  data_o <= a_data_i >> b_data_i[4:0];
        ALU_SRA: data_o <= a_data_i >>> b_data_i[4:0];
        ALU_NOT: data_o <= ~a_data_i;  // what about not b?
        ALU_AND: data_o <= a_data_i & b_data_i;
        ALU_OR:  data_o <= a_data_i | b_data_i;
        ALU_XOR: data_o <= a_data_i ^ b_data_i;  // what about ^ b;?
        ALU_GT:  data_o <= {31'b0, (a_data_i > b_data_i)};
        ALU_LT:  data_o <= {31'b0, (a_data_i < b_data_i)};
        default: data_o <= 32'b0;
      endcase
    end
  end
endmodule : alu_unit
