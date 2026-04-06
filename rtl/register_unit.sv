// Single 32-bit register cell. execute.sv instantiates 32 of these to form
// the architectural register file.
module register_unit (
    input wire rst_i,
    input wire clk_i,
    input wire sel_i,
    input wire wstrb_i,
    input logic [31:0] data_i,
    output logic [31:0] data_o
);
  reg [31:0] r;

  always @(posedge clk_i) begin
    if (rst_i) r <= 32'b0;
    else if (sel_i) begin
      if (wstrb_i) begin
        r <= data_i;
      end
      data_o <= r;
    end
  end

endmodule : register_unit
