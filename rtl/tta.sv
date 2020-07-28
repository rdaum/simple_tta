`include "common.vh"
module tta(
    input wire rst_i,
    input wire clk_i,

    output wire instr_done_o,

    bus_if.master instr_bus,
    bus_if.master data_bus
);

    logic [31:0] pc;
    logic [31:0] src_operand;
    logic [31:0] dst_operand;
    logic [31:0] op;
    logic done_exec;

    assign instr_done_o = done_exec;

    logic need_src_operand;
    logic need_dst_operand;
    logic decoder_enable;
    logic sequencer_done;
    wire pause_sequencer = sequencer_done && ~done_exec;
    sequencer sequencer(
        .clk_i(clk_i),
        .rst_i(rst_i),
        .instr_bus(instr_bus),
        .pc_o(pc),
        .op_o(op),
        .sel_i(~pause_sequencer),
        .src_operand_o(src_operand),
        .need_src_operand_i(need_src_operand),
        .dst_operand_o(dst_operand),
        .decoder_enable_o(decoder_enable),
        .need_dst_operand_i(need_dst_operand),
        .done_o(sequencer_done)
    );
    Unit src_unit;
    Unit dst_unit;
    logic [11:0] si;
    logic [11:0] di;

    decoder decoder(
        .rst_i(rst_i),
        .clk_i(clk_i),
        .sel_i(decoder_enable),
        .op_i(op),
        .src_unit_o(src_unit),
        .need_src_operand_o(need_src_operand),
        .si_o(si),
        .dst_unit_o(dst_unit),
        .need_dst_operand_o(need_dst_operand),
        .di_o(di)
    );

    execute execute(
        .rst_i(rst_i),
        .clk_i(clk_i),
        .pc_i(pc),
        .sel_i(sequencer_done),
        .data_bus(data_bus),
        .src_unit_i(src_unit),
        .src_immediate_i(si),
        .src_operand_i(src_operand),
        .dst_unit_i(dst_unit),
        .dst_immediate_i(di),
        .dst_operand_i(dst_operand),
        .done_o(done_exec)
    );

endmodule : tta