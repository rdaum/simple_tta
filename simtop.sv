`include "rtl/common.vh"

// SoC-style simulation top with boot ROM and external SRAM modeling.
// Port widths use literals (not parameters) for Verilator/Marlin macro compat.
module simtop (
    input wire rst_i,
    input wire sysclk_i,

    input logic [35:0] sram_data_i,
    output logic [35:0] sram_data_o,
    output logic [18:0] sram_addr_o,
    output logic sram_valid_o,
    output logic [3:0] sram_wstrb_o,
    input logic sram_ready_i,

    input wire uart_rxd_i,
    output wire uart_txd_o
);

    // Instruction bus wires (sequencer -> blkram, 32-bit)
    wire [3:0]  ibs_wstrb;
    wire [31:0] ibs_write_data;
    wire [31:0] ibs_addr;
    wire        ibs_valid;
    wire        ibs_instr;
    wire        ibs_ready;
    wire [31:0] ibs_read_data;

    blkram#(
        .INIT_FILE("bootmem.mem"),
        .RAM_DEPTH(12288)
    ) bootmem(
        .clk_i(sysclk_i),
        .rst_i(rst_i),
        .bus_wstrb_i(ibs_wstrb),
        .bus_write_data_i(ibs_write_data),
        .bus_addr_i(ibs_addr),
        .bus_valid_i(ibs_valid),
        .bus_instr_i(ibs_instr),
        .bus_ready_o(ibs_ready),
        .bus_read_data_o(ibs_read_data)
    );

    // Data bus wires (execute -> SRAM, 36-bit data)
    wire [3:0]  dbs_wstrb;
    wire [35:0] dbs_write_data;
    wire [31:0] dbs_addr;
    wire        dbs_valid;

    assign sram_data_o  = dbs_write_data;
    assign sram_valid_o = dbs_valid;
    assign sram_wstrb_o = dbs_wstrb;
    assign sram_addr_o  = dbs_addr[18:0];

    assign uart_txd_o = uart_rxd_i;

    tta tta(
        .rst_i(rst_i),
        .clk_i(sysclk_i),
        .instr_done_o(),
        .instr_wstrb_o(ibs_wstrb),
        .instr_write_data_o(ibs_write_data),
        .instr_addr_o(ibs_addr),
        .instr_valid_o(ibs_valid),
        .instr_instr_o(ibs_instr),
        .instr_ready_i(ibs_ready),
        .instr_read_data_i(ibs_read_data),
        .data_wstrb_o(dbs_wstrb),
        .data_write_data_o(dbs_write_data),
        .data_addr_o(dbs_addr),
        .data_valid_o(dbs_valid),
        .data_instr_o(),
        .data_ready_i(sram_ready_i),
        .data_read_data_i(sram_data_i),
        .mailbox_data_i({DATA_WIDTH{1'b0}}),
        .mailbox_valid_i(1'b0),
        .mailbox_ack_o(),
        .mailbox_out_o(),
        .mailbox_out_valid_o()
    );

endmodule
