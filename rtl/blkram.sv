// Simple synchronous block RAM behind the shared bus interface. Used for
// boot/program memory in the FPGA top and simulation top.
//
// Timing: requests take two cycles. On cycle 0 the address is latched and
// the read/write is performed; on cycle 1 ready pulses high and read_data
// is valid. Per-byte write strobes are supported (wstrb bits correspond to
// bytes [7:0], [15:8], [23:16], [31:24]).
module blkram #(
    parameter RAM_WIDTH = 32,       // Data width in bits
    parameter RAM_DEPTH = 1024,     // Number of words
    parameter INIT_FILE = ""        // Optional hex init file (blank = zero-fill)
) (
    input wire clk_i,               // System clock
    input wire rst_i,               // Synchronous reset (active high)

    // Bus slave port (explicit ports)
    input  logic [3:0]  bus_wstrb_i,
    input  logic [31:0] bus_write_data_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input  logic [31:0] bus_addr_i,
    /* verilator lint_on UNUSEDSIGNAL */
    input  logic        bus_valid_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input  logic        bus_instr_i,
    /* verilator lint_on UNUSEDSIGNAL */
    output logic        bus_ready_o,
    output logic [31:0] bus_read_data_o
);
  (* ram_style = "block" *) reg [RAM_WIDTH-1:0] bram_reg[RAM_DEPTH-1:0];
  reg [31:0] reg_data;

  generate
    /* verilator lint_off WIDTH */
    if (INIT_FILE != "") begin : use_init_file
      /* verilator lint_on WIDTH */
      initial begin
        $display("Preloading %m from %s", INIT_FILE);
        $readmemh(INIT_FILE, bram_reg);
      end
    end else begin : init_bram_to_zero
      integer ram_index;
      initial
        for (ram_index = 0; ram_index < RAM_DEPTH; ram_index = ram_index + 1)
          bram_reg[ram_index] = {RAM_WIDTH{1'b0}};
    end
  endgenerate

  // Two-state handshake FSM:
  //   State 0 (IDLE): wait for valid, latch address, perform read/write,
  //                    assert ready.
  //   State 1 (ACK):  deassert ready, return to idle.
  //
  // Read data appears on reg_data one cycle after the request, at the same
  // time ready goes high.
  logic state;
  reg   ready_reg;
  always @(posedge clk_i) begin
    if (rst_i) begin
      ready_reg <= 1'b0;
      state <= 1'b0;
      reg_data <= 32'b0;
    end else
      case (state)
        0: begin
          if (bus_valid_i) begin
            ready_reg <= 1'b1;
            // Per-byte write strobes: each bit controls one byte lane.
            if (bus_wstrb_i != 4'b0) begin
              if (bus_wstrb_i[3]) bram_reg[bus_addr_i][31:24] <= bus_write_data_i[31:24];
              if (bus_wstrb_i[2]) bram_reg[bus_addr_i][23:16] <= bus_write_data_i[23:16];
              if (bus_wstrb_i[1]) bram_reg[bus_addr_i][15:8] <= bus_write_data_i[15:8];
              if (bus_wstrb_i[0]) bram_reg[bus_addr_i][7:0] <= bus_write_data_i[7:0];
            end
            // Simultaneous read: returns the value *before* the write on this cycle.
            reg_data <= bram_reg[bus_addr_i];
            state <= 1;
          end
        end
        1: begin
          // Deassert ready to complete the one-cycle handshake pulse.
          ready_reg <= 1'b0;
          state <= 1'b0;
        end
      endcase

  end

  assign bus_read_data_o = reg_data;
  assign bus_ready_o = ready_reg;

  //  The following function calculates the address width based on specified RAM depth
  function integer clogb2;
    input integer depth;
    for (clogb2 = 0; depth > 0; clogb2 = clogb2 + 1) depth = depth >> 1;
  endfunction

endmodule
