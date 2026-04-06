`include "common.vh"
// Top-level TTA core. Three stages cooperate:
//
//   sequencer  — prefetches opcode + operand words from instr_bus
//   decoder    — combinational: splits opcode into unit + immediates
//   execute    — reads the source, writes the destination via data_bus
//
// The sequencer and execute communicate via a valid/accept handshake:
//   - instr_valid: sequencer has a complete instruction in its buffer
//   - instr_accept: execute is consuming the instruction this cycle
// The sequencer prefetches the next instruction while execute runs,
// hiding bus latency for sequential code. All addresses are word-addressed.
module tta (
    input wire rst_i,           // Synchronous reset (active high)
    input wire clk_i,           // System clock
    output wire instr_done_o,   // Pulses high for one cycle when a move completes
    bus_if.master instr_bus,    // Instruction fetch bus (read-only in practice)
    bus_if.master data_bus      // Data load/store bus
);

  // pc is the sequencer's next-fetch address. Execute reads it as pc_i
  // for UNIT_PC, so a PC read returns the address of the word immediately
  // AFTER the current instruction (i.e., instruction_addr + word_count).
  // With prefetch, pc may have advanced further if the next instruction's
  // fetch has started, but handoff latches the correct value.
  logic [31:0] pc;
  logic [31:0] src_operand;
  logic [31:0] dst_operand;
  logic [31:0] op;
  logic done_exec;

  assign instr_done_o = done_exec;

  // Valid/accept handshake between sequencer and execute.
  wire instr_valid;
  wire instr_accept;

  // PC write interface from execute (jumps / conditional branches).
  logic [31:0] pc_write;
  logic        pc_write_en;

  sequencer sequencer (
      .clk_i(clk_i),
      .rst_i(rst_i),
      .instr_bus(instr_bus),
      .pc_o(pc),
      .op_o(op),
      .instr_valid_o(instr_valid),
      .instr_accept_i(instr_accept),
      .src_operand_o(src_operand),
      .dst_operand_o(dst_operand),
      .pc_write_i(pc_write),
      .pc_write_en_i(pc_write_en)
`ifdef SEQUENCER_DEBUG
      ,.dbg_prefetch_valid_o(),
      .dbg_fetch_state_o(),
      .dbg_prefetch_op_o()
`endif
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
      .data_bus(data_bus),
      .src_unit_i(src_unit),
      .src_immediate_i(si),
      .src_operand_i(src_operand),
      .dst_unit_i(dst_unit),
      .dst_immediate_i(di),
      .dst_operand_i(dst_operand),
      .pc_write_o(pc_write),
      .pc_write_en_o(pc_write_en),
      .done_o(done_exec),
      .instr_valid_i(instr_valid),
      .instr_accept_o(instr_accept)
  );

endmodule : tta
