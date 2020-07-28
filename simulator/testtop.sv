module testtop(
    input wire rst_i,
    input wire sysclk_i,

    input logic [31:0] instr_data_read_i,
    output logic [31:0] instr_data_write_o,
    output logic [18:0] instr_addr_o,
    output logic instr_valid_o,
    output logic instr_instr_o,
    input logic instr_ready_i,
    
    input logic [31:0] data_data_read_i,
    output logic [31:0] data_data_write_o,
    output logic [18:0] data_addr_o,
    output logic data_valid_o,
    output logic [3:0] data_wstrb_o,
    input logic data_ready_i,

    output logic [31:0] cycles_executed_o,
    output wire instr_done_o
);

    always @(posedge sysclk_i) begin
        if (rst_i) begin
            cycles_executed_o <= 32'b0;
        end
        cycles_executed_o <= cycles_executed_o + 1;
    end

    bus_if data_bus;
    bus_if instr_bus;
    always_comb begin
        data_bus.read_data = data_data_read_i;
        data_bus.ready = data_ready_i;
        data_data_write_o = data_bus.write_data;
        data_valid_o = data_bus.valid;
        data_wstrb_o = data_bus.wstrb;
        data_addr_o = data_bus.addr;

        instr_bus.read_data = instr_data_read_i;
        instr_bus.ready = instr_ready_i;
        instr_data_write_o = instr_bus.write_data;
        instr_valid_o = instr_bus.valid;
        instr_addr_o = instr_bus.addr;
        instr_instr_o = instr_bus.instr;

    end

    tta tta(
        .rst_i(rst_i),
        .clk_i(sysclk_i),
        .instr_bus(instr_bus),
        .data_bus(data_bus),
        .instr_done_o(instr_done_o)
    );

endmodule : testtop