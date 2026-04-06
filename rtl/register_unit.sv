// Single 32-bit register cell. execute.sv instantiates 32 of these to form
// the architectural register file.
//
// When sel_i is asserted the register latches data_o from the current stored
// value. If wstrb_i is also high, data_i is written into the register on
// the same edge, but the new value only appears on data_o on the *next*
// selected cycle (registered read-after-write).
module register_unit (
    input wire rst_i,               // Synchronous reset (active high)
    input wire clk_i,               // System clock
    input wire sel_i,               // Select — enables read and/or write
    input wire wstrb_i,             // Write strobe — high to store data_i
    input logic [31:0] data_i,      // Write data
    output wire  [31:0] data_raw_o  // Combinational read — always reflects current r
);
  reg [31:0] r;
  assign data_raw_o = r;

  always @(posedge clk_i) begin
    if (rst_i) r <= 32'b0;
    else if (sel_i && wstrb_i) begin
      r <= data_i;
    end
  end

endmodule
