`include "common.vh"
// Top-level TTA core. Three pipeline-ish stages cooperate in lock-step:
//
//   sequencer  — fetches opcode + optional operand words from instr_bus
//   decoder    — splits the opcode into source/destination unit + immediates
//   execute    — reads the source, writes the destination via data_bus
//
// All addresses are *word*-addressed (each address increment = one 32-bit
// word). The sequencer and execute stages handshake through done signals
// so that the next fetch does not begin until the current move completes.
module tta (
    input wire rst_i,           // Synchronous reset (active high)
    input wire clk_i,           // System clock
    output wire instr_done_o,   // Pulses high for one cycle when a move completes
    bus_if.master instr_bus,    // Instruction fetch bus (read-only in practice)
    bus_if.master data_bus      // Data load/store bus
);

  logic [31:0] pc;
  logic [31:0] src_operand;
  logic [31:0] dst_operand;
  logic [31:0] op;
  logic done_exec;

  assign instr_done_o = done_exec;

  logic sequencer_done;

  // PC write interface from execute (jumps / conditional branches).
  logic [31:0] pc_write;
  logic        pc_write_en;

  // Hold off the next fetch until the current execute phase has completed.
  wire  pause_sequencer = sequencer_done && ~done_exec;
  sequencer sequencer (
      .clk_i(clk_i),
      .rst_i(rst_i),
      .instr_bus(instr_bus),
      .pc_o(pc),
      .op_o(op),
      .sel_i(~pause_sequencer),
      .src_operand_o(src_operand),
      .dst_operand_o(dst_operand),
      .pc_write_i(pc_write),
      .pc_write_en_i(pc_write_en),
      .done_o(sequencer_done)
  );
  Unit src_unit;
  Unit dst_unit;
  logic [11:0] si;
  logic [11:0] di;

  decoder decoder (
      .op_i(op),
      .src_unit_o(src_unit),
      .si_o(si),
      .dst_unit_o(dst_unit),
      .di_o(di)
  );

  execute execute (
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
      .pc_write_o(pc_write),
      .pc_write_en_o(pc_write_en),
      .done_o(done_exec)
  );

endmodule : tta
