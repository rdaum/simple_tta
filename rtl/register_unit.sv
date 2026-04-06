`include "common.vh"

// Single 36-bit tagged register cell. execute.sv instantiates NUM_REGISTERS
// of these to form the architectural register file.
//
// Each register stores a DATA_WIDTH-bit tagged value (32-bit value +
// 4-bit sidecar tag). The tag occupies bits [DATA_WIDTH-1:VAL_WIDTH].
module register_unit (
    input wire rst_i,
    input wire clk_i,
    input wire sel_i,
    input wire wstrb_i,
    input  logic [DATA_WIDTH-1:0] data_i,
    output wire  [DATA_WIDTH-1:0] data_raw_o
);
  reg [DATA_WIDTH-1:0] r;
  assign data_raw_o = r;

  always @(posedge clk_i) begin
    if (rst_i) r <= {DATA_WIDTH{1'b0}};
    else if (sel_i && wstrb_i) begin
      r <= data_i;
    end
  end

endmodule
