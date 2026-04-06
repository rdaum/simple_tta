// Unpacks the fixed-width instruction word into source and destination routing
// information, immediates, and flags.
//
// Instruction word layout (32 bits, word-addressed):
//   [4:0]    — source unit (5-bit Unit enum)
//   [9:5]    — destination unit (5-bit Unit enum)
//   [17:10]  — source immediate (8 bits, interpretation depends on unit)
//   [25:18]  — destination immediate (8 bits)
//   [27:26]  — predicate flags (if_set, if_clear)
//   [31:28]  — reserved
//
// All outputs are combinational — results are available in the same cycle
// that op_i changes, with no clock delay.
module decoder (
    input [31:0] op_i,              // Raw instruction word to decode
    output logic [4:0] src_unit_o,  // Decoded source unit selector
    output logic [7:0] si_o,        // Source immediate field
    output logic [4:0] dst_unit_o,  // Decoded destination unit selector
    output logic [7:0] di_o,        // Destination immediate field
    output logic [5:0] flags_o      // Predicate and reserved flags
);

  assign src_unit_o = op_i[4:0];
  assign dst_unit_o = op_i[9:5];
  assign si_o       = op_i[17:10];
  assign di_o       = op_i[25:18];
  assign flags_o    = op_i[31:26];

endmodule
