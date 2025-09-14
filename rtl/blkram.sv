module blkram #(
    parameter RAM_WIDTH = 32,  // Specify RAM data width
    parameter RAM_DEPTH = 1024,  // Specify RAM depth (number of entries)
    parameter INIT_FILE = ""                     // Specify name/location of RAM initialization file if using one (leave blank if not)
) (
    input wire clk_i,
    input wire rst_i,

    bus_if.slave data_bus
);
  (* ram_style = "block" *) reg [RAM_WIDTH-1:0] bram_reg[RAM_DEPTH-1:0];
  reg [31:0] reg_data;

  wire [31:0] bus_addr;
  assign bus_addr = data_bus.addr;

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
          if (data_bus.valid) begin
            ready_reg <= 1'b1;
            if (data_bus.wstrb != 4'b0) begin
              if (data_bus.wstrb[3]) bram_reg[data_bus.addr][31:24] <= data_bus.write_data[31:24];
              if (data_bus.wstrb[2]) bram_reg[data_bus.addr][23:16] <= data_bus.write_data[23:16];
              if (data_bus.wstrb[1]) bram_reg[data_bus.addr][15:8] <= data_bus.write_data[15:8];
              if (data_bus.wstrb[0]) bram_reg[data_bus.addr][7:0] <= data_bus.write_data[7:0];
            end
            reg_data <= bram_reg[data_bus.addr];
            state <= 1;
          end
        end
        1: begin
          ready_reg <= 1'b0;
          state <= 1'b0;
        end
      endcase

  end

  assign data_bus.read_data = reg_data;
  assign data_bus.ready = ready_reg;

  //  The following function calculates the address width based on specified RAM depth
  function integer clogb2;
    input integer depth;
    for (clogb2 = 0; depth > 0; clogb2 = clogb2 + 1) depth = depth >> 1;
  endfunction : clogb2

endmodule : blkram
