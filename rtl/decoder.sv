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
//
// All outputs are combinational — results are available in the same cycle
// that op_i changes, with no clock delay.
module decoder (
    input [31:0] op_i,              // Raw instruction word to decode
    output logic [3:0] src_unit_o,  // Decoded source unit selector
    output logic [11:0] si_o,       // Source immediate field
    output logic [3:0] dst_unit_o,  // Decoded destination unit selector
    output logic [11:0] di_o        // Destination immediate field
);

  assign src_unit_o = op_i[3:0];
  assign si_o = op_i[15:4];
  assign dst_unit_o = op_i[19:16];
  assign di_o = op_i[31:20];

endmodule
