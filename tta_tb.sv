`include "rtl/common.vh"

// Simple testbench wrapper for TTA — passes bus signals through directly.
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

// Instantiate the TTA core — direct port connections, no bus_if.
tta tta_core(
    .rst_i(rst_i),
    .clk_i(clk_i),
    .instr_done_o(instr_done_o),
    // Instruction bus
    .instr_wstrb_o(),
    .instr_write_data_o(),
    .instr_addr_o(instr_addr_o),
    .instr_valid_o(instr_valid_o),
    .instr_instr_o(),
    .instr_ready_i(instr_ready_i),
    .instr_read_data_i(instr_data_read_i),
    // Data bus
    .data_wstrb_o(data_wstrb_o),
    .data_write_data_o(data_data_write_o),
    .data_addr_o(data_addr_o),
    .data_valid_o(data_valid_o),
    .data_instr_o(),
    .data_ready_i(data_ready_i),
    .data_read_data_i(data_data_read_i)
);

endmodule
