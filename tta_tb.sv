`include "rtl/common.vh"

// Simple testbench wrapper for TTA that doesn't use interfaces
module tta_tb(
    input wire rst_i,
    input wire clk_i,
    
    // Instruction bus signals
    output wire [31:0] instr_addr_o,
    output wire instr_valid_o,
    input wire instr_ready_i,
    input wire [31:0] instr_data_read_i,
    
    // Data bus signals  
    output wire [31:0] data_addr_o,
    output wire data_valid_o,
    input wire data_ready_i,
    input wire [31:0] data_data_read_i,
    output wire [31:0] data_data_write_o,
    output wire [3:0] data_wstrb_o,
    
    output wire instr_done_o
);

// Create bus interfaces
bus_if instr_bus();
bus_if data_bus();

// Connect to external signals
assign instr_addr_o = instr_bus.addr;
assign instr_valid_o = instr_bus.valid;
assign instr_bus.ready = instr_ready_i;
assign instr_bus.read_data = instr_data_read_i;

assign data_addr_o = data_bus.addr;
assign data_valid_o = data_bus.valid;
assign data_bus.ready = data_ready_i;
assign data_bus.read_data = data_data_read_i;
assign data_data_write_o = data_bus.write_data;
assign data_wstrb_o = data_bus.wstrb;

// Instantiate the TTA core
tta tta_core(
    .rst_i(rst_i),
    .clk_i(clk_i),
    .instr_done_o(instr_done_o),
    .instr_bus(instr_bus.master),
    .data_bus(data_bus.master)
);

endmodule