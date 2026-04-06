`include "common.vh"

// One ALU lane. execute.sv instantiates NUM_ALUS lanes and selects by index.
//
// The ALU is *passive* — its left operand (a), right operand (b), and
// operator are written independently by prior move instructions. The
// result is available combinationally on data_raw_o at all times.
//
// Arithmetic operates on the VAL_WIDTH-bit value portion only.
// The TAG_WIDTH-bit tag from the left (A) operand is preserved in
// the result — the right operand's tag is ignored.
module alu_unit (
    input logic [3:0] oper_i,
    input  logic [DATA_WIDTH-1:0] a_data_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input  logic [DATA_WIDTH-1:0] b_data_i,
    /* verilator lint_on UNUSEDSIGNAL */
    output wire  [DATA_WIDTH-1:0] data_raw_o
);

  wire [TAG_WIDTH-1:0]  a_tag = a_data_i[DATA_WIDTH-1:VAL_WIDTH];
  wire [VAL_WIDTH-1:0]  a_val = a_data_i[VAL_WIDTH-1:0];
  wire [VAL_WIDTH-1:0]  b_val = b_data_i[VAL_WIDTH-1:0];

  reg [VAL_WIDTH-1:0] val_result;
  assign data_raw_o = {a_tag, val_result};

  always_comb begin
    case (oper_i)
      ALU_NOP: val_result = {VAL_WIDTH{1'b0}};
      ALU_ADD: val_result = a_val + b_val;
      ALU_SUB: val_result = a_val - b_val;
      ALU_DIV: val_result = {VAL_WIDTH{1'b0}};  // handled by muldiv_unit
      ALU_MUL: val_result = {VAL_WIDTH{1'b0}};  // handled by muldiv_unit
      ALU_MOD: val_result = {VAL_WIDTH{1'b0}};  // handled by muldiv_unit
      ALU_EQL: val_result = {{(VAL_WIDTH-1){1'b0}}, (a_val == b_val)};
      ALU_SL:  val_result = a_val << b_val[4:0];
      ALU_SR:  val_result = a_val >> b_val[4:0];
      ALU_SRA: val_result = a_val >>> b_val[4:0];
      ALU_NOT: val_result = ~a_val;
      ALU_AND: val_result = a_val & b_val;
      ALU_OR:  val_result = a_val | b_val;
      ALU_XOR: val_result = a_val ^ b_val;
      ALU_GT:  val_result = {{(VAL_WIDTH-1){1'b0}}, (a_val > b_val)};
      ALU_LT:  val_result = {{(VAL_WIDTH-1){1'b0}}, (a_val < b_val)};
      default: val_result = {VAL_WIDTH{1'b0}};
    endcase
  end

endmodule
