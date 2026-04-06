// Unpacks the fixed-width instruction word into source and destination routing
// information plus flags indicating whether extra operand words are required.
//
// Instruction word layout (32 bits, word-addressed):
//   [3:0]   — source unit (Unit enum)
//   [15:4]  — source immediate (12 bits, interpretation depends on unit)
//   [19:16] — destination unit (Unit enum)
//   [31:20] — destination immediate (12 bits)
//
// When a unit is UNIT_MEMORY_OPERAND or UNIT_ABS_OPERAND, a full 32-bit
// operand follows in the next program word; the corresponding need_*_operand
// flag tells the sequencer to fetch it.
module decoder (
    input wire clk_i,               // System clock
    input wire rst_i,               // Synchronous reset (active high)
    input wire sel_i,               // Decode enable strobe from sequencer
    input [31:0] op_i,              // Raw instruction word to decode
    output Unit src_unit_o,         // Decoded source unit selector
    output logic [11:0] si_o,       // Source immediate field
    output logic need_src_operand_o,// High when source needs a 32-bit operand word

    output Unit dst_unit_o,         // Decoded destination unit selector
    output logic [11:0] di_o,       // Destination immediate field
    output logic need_dst_operand_o // High when destination needs a 32-bit operand word
);
  logic [31:0] src_value;
  always @(posedge clk_i) begin
    if (rst_i) begin
      src_unit_o = UNIT_NONE;
      dst_unit_o = UNIT_NONE;
      si_o = 12'b0;
      di_o = 12'b0;
      need_src_operand_o = 1'b0;
      need_dst_operand_o = 1'b0;
    end else if (sel_i) begin
      // Instruction layout:
      // [3:0]   src unit
      // [15:4]  src immediate
      // [19:16] dst unit
      // [31:20] dst immediate
      src_unit_o = Unit'(op_i[3:0]);
      si_o = op_i[15:4];
      dst_unit_o = Unit'(op_i[19:16]);
      di_o = op_i[31:20];

      need_src_operand_o = src_unit_o == UNIT_MEMORY_OPERAND || src_unit_o == UNIT_ABS_OPERAND;
      need_dst_operand_o = dst_unit_o == UNIT_MEMORY_OPERAND || dst_unit_o == UNIT_ABS_OPERAND;
    end
  end

endmodule : decoder
