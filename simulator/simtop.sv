module simtop (
    input wire rst_i,
    input wire sysclk_i,

    input logic [31:0] sram_data_i,
    output logic [31:0] sram_data_o,
    output logic [18:0] sram_addr_o,
    output logic sram_valid_o,
    output logic [3:0] sram_wstrb_o,
    input logic sram_ready_i,

    input wire uart_rxd_i,
    output wire uart_txd_o
);

    bus_if bootmem_bus();
    blkram#(
        .INIT_FILE("bootmem.mem"),
        .RAM_DEPTH(12288)
    ) bootmem(
        .clk_i(sysclk_i),
        .rst_i(rst_i),

        .data_bus(bootmem_bus.slave)
    );

    bus_if data_bus();
    always_comb begin
        data_bus.read_data = sram_data_i;
        data_bus.ready = sram_ready_i;
        sram_data_o = data_bus.write_data;
        sram_valid_o = data_bus.valid;
        sram_wstrb_o = data_bus.wstrb;
        sram_addr_o = data_bus.addr;
    end

    tta tta(
        .rst_i(rst_i),
        .clk_i(sysclk_i),
        .instr_bus(bootmem_bus),
        .data_bus(data_bus)
    );

endmodule : simtop