`include "rtl/common.vh"

// Simple testbench wrapper for TTA — passes bus signals through directly.
// Port widths use literals (not parameters) for Verilator/Marlin macro compat.
module tta_tb(
    input wire rst_i,
    input wire clk_i,

    // Instruction bus signals (32-bit)
    output wire [31:0] instr_addr_o,
    output wire instr_valid_o,
    input wire instr_ready_i,
    input wire [31:0] instr_data_read_i,

    // Data bus signals (36-bit data, 32-bit address)
    output wire [31:0] data_addr_o,
    output wire data_valid_o,
    input wire data_ready_i,
    input wire [35:0] data_data_read_i,
    output wire [35:0] data_data_write_o,
    output wire [3:0] data_wstrb_o,

    output wire instr_done_o,

    // Host mailbox (36-bit data)
    input wire [35:0] mailbox_data_i,
    input wire mailbox_valid_i,
    output wire mailbox_ack_o,
    output wire [35:0] mailbox_out_o,
    output wire mailbox_out_valid_o
);

tta tta_core(
    .rst_i(rst_i),
    .clk_i(clk_i),
    .instr_done_o(instr_done_o),
    .instr_wstrb_o(),
    .instr_write_data_o(),
    .instr_addr_o(instr_addr_o),
    .instr_valid_o(instr_valid_o),
    .instr_instr_o(),
    .instr_ready_i(instr_ready_i),
    .instr_read_data_i(instr_data_read_i),
    .data_wstrb_o(data_wstrb_o),
    .data_write_data_o(data_data_write_o),
    .data_addr_o(data_addr_o),
    .data_valid_o(data_valid_o),
    .data_instr_o(),
    .data_ready_i(data_ready_i),
    .data_read_data_i(data_data_read_i),
    .mailbox_data_i(mailbox_data_i),
    .mailbox_valid_i(mailbox_valid_i),
    .mailbox_ack_o(mailbox_ack_o),
    .mailbox_out_o(mailbox_out_o),
    .mailbox_out_valid_o(mailbox_out_valid_o)
);

endmodule
