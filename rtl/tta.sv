`include "common.vh"
// Top-level TTA core. Three stages cooperate:
//
//   sequencer  — prefetches opcode + operand words from instr_bus
//   decoder    — combinational: splits opcode into unit + immediates + flags
//   execute    — reads the source, writes the destination via data_bus
//
// The sequencer and execute communicate via a valid/accept handshake:
//   - instr_valid: sequencer has a complete instruction in its buffer
//   - instr_accept: execute is consuming the instruction this cycle
// The sequencer prefetches the next instruction while execute runs,
// hiding bus latency for sequential code. All addresses are word-addressed.
module tta #(
    parameter NUM_REGISTERS  = 32,
    parameter NUM_ALUS       = 8,
    parameter NUM_STACKS     = 8,
    parameter STACK_DEPTH    = 32,
    parameter BARRIER_DEPTH  = 32
) (
    input wire rst_i,           // Synchronous reset (active high)
    input wire clk_i,           // System clock
    output wire instr_done_o,   // Pulses high for one cycle when a move completes

    // Instruction fetch bus
    output logic [3:0]  instr_wstrb_o,
    output logic [31:0] instr_write_data_o,
    output logic [31:0] instr_addr_o,
    output logic        instr_valid_o,
    output logic        instr_instr_o,
    input  logic        instr_ready_i,
    input  logic [31:0] instr_read_data_i,

    // Data bus
    output logic [3:0]  data_wstrb_o,
    output logic [31:0] data_write_data_o,
    output logic [31:0] data_addr_o,
    output logic        data_valid_o,
    output logic        data_instr_o,
    input  logic        data_ready_i,
    input  logic [31:0] data_read_data_i
);

  // pc is the instruction PC promoted from the queue on accept. Execute
  // reads it as pc_i for UNIT_PC, getting (instruction_addr + word_count).
  // The fetch address is tracked separately inside the sequencer.
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
      .instr_wstrb_o(instr_wstrb_o),
      .instr_write_data_o(instr_write_data_o),
      .instr_addr_o(instr_addr_o),
      .instr_valid_o_bus(instr_valid_o),
      .instr_instr_o(instr_instr_o),
      .instr_ready_i(instr_ready_i),
      .instr_read_data_i(instr_read_data_i),
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

  logic [4:0] src_unit;
  logic [4:0] dst_unit;
  logic [7:0] si;
  logic [7:0] di;
  logic [5:0] flags;

  decoder decoder (
      .op_i(op),
      .src_unit_o(src_unit),
      .si_o(si),
      .dst_unit_o(dst_unit),
      .di_o(di),
      .flags_o(flags)
  );

  execute #(
      .NUM_REGISTERS(NUM_REGISTERS),
      .NUM_ALUS(NUM_ALUS),
      .NUM_STACKS(NUM_STACKS),
      .STACK_DEPTH(STACK_DEPTH),
      .BARRIER_DEPTH(BARRIER_DEPTH)
  ) execute (
      .rst_i(rst_i),
      .clk_i(clk_i),
      .pc_i(pc),
      .data_wstrb_o(data_wstrb_o),
      .data_write_data_o(data_write_data_o),
      .data_addr_o(data_addr_o),
      .data_valid_o(data_valid_o),
      .data_instr_o(data_instr_o),
      .data_ready_i(data_ready_i),
      .data_read_data_i(data_read_data_i),
      .src_unit_i(src_unit),
      .src_immediate_i(si),
      .src_operand_i(src_operand),
      .dst_unit_i(dst_unit),
      .dst_immediate_i(di),
      .dst_operand_i(dst_operand),
      .flags_i(flags),
      .pc_write_o(pc_write),
      .pc_write_en_o(pc_write_en),
      .done_o(done_exec),
      .instr_valid_i(instr_valid),
      .instr_accept_o(instr_accept)
  );

endmodule
