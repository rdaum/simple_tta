`include "common.vh"

// Main execution engine. Each TTA instruction is a single *move* from a
// source unit to a destination unit. This module resolves the source value
// (EXEC_START_SRC phase), then writes it to the destination (EXEC_START_DST
// phase). Memory and stack accesses require extra wait states; register and
// ALU moves typically complete in one or two cycles.
//
// Immediate field (8 bits from the instruction word):
//   REG/REG_VALUE/REG_TAG: [4:0] = register index (0..NUM_REGISTERS-1)
//   REG_DEREF:             [4:0] = register index, [7:5] = word offset (0-7)
//   ALU units:             [2:0] = lane index (0..NUM_ALUS-1)
//   STACK push/pop:        [2:0] = stack ID (0..NUM_STACKS-1)
//   STACK_INDEX:           [2:0] = stack ID, [7:3] = offset from top
//   MEM_IMM:               full 8 bits = word address (0-255)
//   IMM:                   full 8 bits = literal value (0-255)
module execute #(
    parameter NUM_REGISTERS  = 32,
    parameter NUM_ALUS       = 8,
    parameter NUM_STACKS     = 8,
    parameter STACK_DEPTH    = 32,
    parameter BARRIER_DEPTH  = 32
) (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)
    input wire [31:0] pc_i,             // Current program counter (for UNIT_PC reads)
    input logic [4:0] src_unit_i,       // Source unit selector (from decoder)
    input logic [7:0] src_immediate_i,  // Source immediate field
    input logic [31:0] src_operand_i,   // Source 32-bit operand (from sequencer)
    input logic [4:0] dst_unit_i,       // Destination unit selector (from decoder)
    input logic [7:0] dst_immediate_i,  // Destination immediate field
    input logic [31:0] dst_operand_i,   // Destination 32-bit operand (from sequencer)
    input logic [5:0] flags_i,          // Predicate and reserved flags

    // Data memory bus
    output logic [3:0]  data_wstrb_o,
    output logic [31:0] data_write_data_o,
    output logic [31:0] data_addr_o,
    output logic        data_valid_o,
    output logic        data_instr_o,
    input  logic        data_ready_i,
    input  logic [31:0] data_read_data_i,

    output logic [31:0] pc_write_o,     // PC value for jumps (to sequencer)
    output logic        pc_write_en_o,  // High to override PC (jump taken)
    output logic done_o,                // Pulses high when the move is complete

    // Valid/accept handshake with sequencer.
    input  wire         instr_valid_i,  // Sequencer has a complete instruction ready
    output wire         instr_accept_o  // Execute is consuming the instruction this cycle
);
  // Architectural register file: NUM_REGISTERS independent 32-bit cells.
  reg reg_unit_select[0:NUM_REGISTERS-1];
  reg reg_unit_write[0:NUM_REGISTERS-1];
  reg [31:0] reg_in_data[0:NUM_REGISTERS-1];
  wire [31:0] reg_raw_data[0:NUM_REGISTERS-1];
  genvar gi;
  generate
    for (gi = 0; gi < NUM_REGISTERS; gi = gi + 1) begin : gen_regs
      register_unit ru (
          .rst_i    (rst_i),
          .clk_i    (clk_i),
          .sel_i    (reg_unit_select[gi]),
          .wstrb_i  (reg_unit_write[gi]),
          .data_i   (reg_in_data[gi]),
          .data_raw_o(reg_raw_data[gi])
      );
    end
  endgenerate

  // ALU bank: NUM_ALUS independently addressable compute lanes.
  reg [31:0] alu_in_data_a[0:NUM_ALUS-1];
  reg [31:0] alu_in_data_b[0:NUM_ALUS-1];
  wire [31:0] alu_out_data[0:NUM_ALUS-1];
  reg [3:0] alu_operation[0:NUM_ALUS-1];
  generate
    for (gi = 0; gi < NUM_ALUS; gi = gi + 1) begin : gen_alus
      alu_unit au (
          .oper_i(alu_operation[gi]),
          .a_data_i(alu_in_data_a[gi]),
          .b_data_i(alu_in_data_b[gi]),
          .data_raw_o(alu_out_data[gi])
      );
    end
  endgenerate

  // Shared multi-cycle multiply/divide unit.
  logic        muldiv_start;
  logic [3:0]  muldiv_oper;
  logic [31:0] muldiv_a, muldiv_b, muldiv_result;
  logic        muldiv_done;

  muldiv_unit muldiv_inst (
      .clk_i    (clk_i),
      .rst_i    (rst_i),
      .start_i  (muldiv_start),
      .oper_i   (muldiv_oper),
      .a_i      (muldiv_a),
      .b_i      (muldiv_b),
      .result_o (muldiv_result),
      .done_o   (muldiv_done)
  );

  // Detect whether an ALU operation requires the multi-cycle unit.
  function [0:0] is_muldiv_op;
    input [3:0] op;
    begin
      is_muldiv_op = (op == ALU_MUL) || (op == ALU_DIV) || (op == ALU_MOD);
    end
  endfunction

  // Shared stack unit implementing NUM_STACKS logical stacks.
  logic [2:0] stack_select;
  logic stack_push, stack_pop;
  logic [4:0] stack_offset;
  logic stack_index_read, stack_index_write;
  logic [31:0] stack_data_in, stack_data_out;
  /* verilator lint_off UNUSEDSIGNAL */
  logic stack_ready, stack_overflow, stack_underflow;
  /* verilator lint_on UNUSEDSIGNAL */

  stack_unit #(
      .NUM_STACKS(NUM_STACKS),
      .STACK_DEPTH(STACK_DEPTH)
  ) stack_unit_inst (
      .clk_i(clk_i),
      .rst_i(rst_i),
      .stack_select_i(stack_select),
      .stack_push_i(stack_push),
      .stack_pop_i(stack_pop),
      .stack_offset_i(stack_offset),
      .stack_index_read_i(stack_index_read),
      .stack_index_write_i(stack_index_write),
      .data_i(stack_data_in),
      .data_o(stack_data_out),
      .stack_ready_o(stack_ready),
      .stack_overflow_o(stack_overflow),
      .stack_underflow_o(stack_underflow)
  );

  // Write barrier FIFO for hardware-assisted GC.
  logic barrier_push, barrier_pop;
  logic [31:0] barrier_data_in, barrier_data_out;
  logic barrier_ready;
  /* verilator lint_off UNUSEDSIGNAL */
  logic barrier_empty, barrier_full, barrier_overflow;
  /* verilator lint_on UNUSEDSIGNAL */

  barrier_unit #(
      .BARRIER_DEPTH(BARRIER_DEPTH)
  ) barrier_inst (
      .clk_i(clk_i),
      .rst_i(rst_i),
      .push_i(barrier_push),
      .data_i(barrier_data_in),
      .pop_i(barrier_pop),
      .data_o(barrier_data_out),
      .ready_o(barrier_ready),
      .barrier_empty_o(barrier_empty),
      .barrier_full_o(barrier_full),
      .barrier_overflow_o(barrier_overflow)
  );

  // Execution FSM: first resolve the source value, then write to destination.
  typedef enum {
    EXEC_START_SRC,        // Begin source resolution (fuses dst for immediate moves)
    EXEC_SRC_MEM_RETRIEVE, // Wait for data_ready_i on a memory read
    EXEC_SRC_STACK_WAIT,   // Wait for stack_ready after pop / peek
    EXEC_SRC_BARRIER_WAIT, // Wait for barrier_ready after pop
    EXEC_SRC_MULDIV_WAIT,  // Wait for muldiv_done after MUL/DIV/MOD
    EXEC_START_DST,        // Route src_value to the destination unit
    EXEC_DST_STACK_WAIT,   // Wait for stack_ready after push / poke
    EXEC_DST_BARRIER_WAIT  // Wait for barrier_ready after push
  } ExecState;
  ExecState exec_state;
  logic [31:0] src_value;

  // Execute runs autonomously once triggered via the valid/accept handshake.
  logic exec_active;

  // Accept is combinational: fires for exactly one cycle when a valid
  // instruction is presented and execute is idle.
  assign instr_accept_o = instr_valid_i && !exec_active && !done_o;
  logic stack_wait_armed;

  // 1-bit condition register for conditional branches.
  logic cond_reg;

  // Predication: check flags against condition register.
  wire pred_if_set   = flags_i[PRED_IF_SET];
  wire pred_if_clear = flags_i[PRED_IF_CLEAR];
  wire pred_skip = (pred_if_set && !cond_reg) || (pred_if_clear && cond_reg);

  integer ii;
  always @(posedge clk_i) begin
    if (rst_i) begin
      for (ii = 0; ii < NUM_REGISTERS; ii = ii + 1) begin
        reg_unit_select[ii] <= 1'b0;
        reg_unit_write[ii] <= 1'b0;
      end
      for (ii = 0; ii < NUM_ALUS; ii = ii + 1)
        alu_operation[ii] <= 4'h0;

      // Initialize stack and barrier signals
      stack_select <= 3'b000;
      stack_push <= 1'b0;
      stack_pop <= 1'b0;
      barrier_push <= 1'b0;
      barrier_pop <= 1'b0;
      barrier_data_in <= 32'b0;
      stack_offset <= 5'b0;
      stack_index_read <= 1'b0;
      stack_index_write <= 1'b0;
      stack_data_in <= 32'b0;

      // Initialize muldiv signals
      muldiv_start <= 1'b0;
      muldiv_oper <= 4'h0;
      muldiv_a <= 32'b0;
      muldiv_b <= 32'b0;

      // Initialize execution state
      exec_state <= EXEC_START_SRC;
      cond_reg <= 1'b0;
      src_value <= 32'b0;
      stack_wait_armed <= 1'b0;
      pc_write_o <= 32'b0;
      pc_write_en_o <= 1'b0;
      exec_active <= 1'b0;
      data_valid_o <= 1'b0;
      data_instr_o <= 1'b0;
      data_wstrb_o <= 4'b0000;
      data_addr_o <= 32'b0;
      data_write_data_o <= 32'b0;

      done_o <= 1'b0;
    end else begin
      reg run_execute;
      run_execute = (exec_active || instr_accept_o) && !done_o;

      // Auto-clear done_o and pc_write_en_o after one cycle.
      if (done_o) begin
        done_o <= 1'b0;
        pc_write_en_o <= 1'b0;
        exec_active <= 1'b0;
      end

      // Latch active on accept; stay active through multi-cycle operations.
      if (instr_accept_o)
        exec_active <= 1'b1;

      if (run_execute) begin
      case (exec_state)
        EXEC_START_SRC: begin
          reg src_resolved;
          reg [31:0] resolved_src;
          src_resolved = 1'b0;
          resolved_src = 32'b0;
          done_o <= 1'b0;
          pc_write_en_o <= 1'b0;
          for (ii = 0; ii < NUM_REGISTERS; ii = ii + 1) begin
            reg_unit_select[ii] <= 1'b0;
            reg_unit_write[ii] <= 1'b0;
          end
          data_valid_o <= 1'b0;
          data_wstrb_o <= 4'b0000;
          data_instr_o <= 1'b0;

          // Clear stack, barrier, and muldiv signals
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;
          barrier_push <= 1'b0;
          barrier_pop <= 1'b0;
          muldiv_start <= 1'b0;

          // Predication: skip this instruction entirely if condition doesn't match
          if (pred_skip) begin
            done_o <= 1'b1;
            exec_state <= EXEC_START_SRC;
          end else
          case (src_unit_i)
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE: begin
              case (src_unit_i)
                UNIT_MEMORY_OPERAND:   data_addr_o <= src_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {24'b0, src_immediate_i};
                default:               data_addr_o <= 32'b0;
              endcase
              data_valid_o <= 1'b1;
              exec_state <= EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_REGISTER: begin
              reg_unit_select[src_immediate_i[4:0]] <= 1'b1;
              resolved_src = reg_raw_data[src_immediate_i[4:0]];
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_VALUE: begin
              resolved_src = reg_raw_data[src_immediate_i[4:0]] & ~TAG_MASK_32;
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_TAG: begin
              resolved_src = reg_raw_data[src_immediate_i[4:0]] & TAG_MASK_32;
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_DEREF: begin
              data_addr_o <= (reg_raw_data[src_immediate_i[4:0]] & ~TAG_MASK_32)
                             + {29'b0, src_immediate_i[7:5]};
              data_valid_o <= 1'b1;
              exec_state <= EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_ALU_LEFT: begin
              resolved_src = alu_in_data_a[src_immediate_i[2:0]];
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_ALU_RIGHT: begin
              resolved_src = alu_in_data_b[src_immediate_i[2:0]];
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_ALU_RESULT: begin
              if (is_muldiv_op(alu_operation[src_immediate_i[2:0]])) begin
                muldiv_start <= 1'b1;
                muldiv_oper  <= alu_operation[src_immediate_i[2:0]];
                muldiv_a     <= alu_in_data_a[src_immediate_i[2:0]];
                muldiv_b     <= alu_in_data_b[src_immediate_i[2:0]];
                exec_state   <= EXEC_SRC_MULDIV_WAIT;
              end else begin
                resolved_src = alu_out_data[src_immediate_i[2:0]];
                src_value <= resolved_src;
                src_resolved = 1'b1;
              end
            end
            UNIT_ABS_IMMEDIATE: begin
              resolved_src = {24'b0, src_immediate_i};
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_ABS_OPERAND: begin
              resolved_src = src_operand_i;
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_PC: begin
              resolved_src = pc_i;
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_COND: begin
              resolved_src = {31'b0, cond_reg};
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_STACK_PUSH_POP: begin
              stack_select <= src_immediate_i[2:0];
              stack_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              stack_select <= src_immediate_i[2:0];
              stack_offset <= src_immediate_i[7:3];
              stack_index_read <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_WRITE_BARRIER: begin
              barrier_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_SRC_BARRIER_WAIT;
            end
            UNIT_NONE: begin
              resolved_src = 32'b0;
              src_value <= resolved_src;
              if (dst_unit_i != UNIT_NONE)
                src_resolved = 1'b1;
              else begin
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
            end
            default: begin
              resolved_src = 32'b0;
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
          endcase

          // When the source resolved immediately, evaluate the destination
          // in the same cycle — fused execute, no extra state transition.
          if (src_resolved) begin
            case (dst_unit_i)
              UNIT_REGISTER: begin
                reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
                reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
                reg_in_data[dst_immediate_i[4:0]] <= resolved_src;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_REG_VALUE: begin
                reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
                reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
                reg_in_data[dst_immediate_i[4:0]] <= (resolved_src & ~TAG_MASK_32)
                                                   | (reg_raw_data[dst_immediate_i[4:0]] & TAG_MASK_32);
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_REG_TAG: begin
                reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
                reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
                reg_in_data[dst_immediate_i[4:0]] <= (reg_raw_data[dst_immediate_i[4:0]] & ~TAG_MASK_32)
                                                   | (resolved_src & TAG_MASK_32);
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_REG_DEREF: begin
                data_addr_o <= (reg_raw_data[dst_immediate_i[4:0]] & ~TAG_MASK_32)
                               + {29'b0, dst_immediate_i[7:5]};
                data_write_data_o <= resolved_src;
                data_wstrb_o <= 4'b1111;
                data_valid_o <= 1'b1;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_ALU_LEFT: begin
                alu_in_data_a[dst_immediate_i[2:0]] <= resolved_src;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_ALU_RIGHT: begin
                alu_in_data_b[dst_immediate_i[2:0]] <= resolved_src;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_ALU_OPERATOR: begin
                alu_operation[dst_immediate_i[2:0]] <= resolved_src[3:0];
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_PC: begin
                pc_write_o <= resolved_src;
                pc_write_en_o <= 1'b1;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_COND: begin
                cond_reg <= (resolved_src != 32'b0);
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_PC_COND: begin
                if (cond_reg) begin
                  pc_write_o <= resolved_src;
                  pc_write_en_o <= 1'b1;
                end
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_NONE: begin
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              default: begin
                exec_state <= EXEC_START_DST;
              end
            endcase
          end

        end
        EXEC_SRC_MEM_RETRIEVE: begin
          if (data_ready_i) begin
            src_value <= data_read_data_i;
            data_valid_o <= 1'b0;
            exec_state <= EXEC_START_DST;
          end
        end
        EXEC_SRC_STACK_WAIT: begin
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;

          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (stack_ready) begin
            src_value <= stack_data_out;
            stack_wait_armed <= 1'b0;
            exec_state <= EXEC_START_DST;
          end
        end
        EXEC_SRC_BARRIER_WAIT: begin
          barrier_pop <= 1'b0;
          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (barrier_ready) begin
            src_value <= barrier_data_out;
            stack_wait_armed <= 1'b0;
            exec_state <= EXEC_START_DST;
          end
        end
        EXEC_SRC_MULDIV_WAIT: begin
          muldiv_start <= 1'b0;
          if (muldiv_done) begin
            src_value <= muldiv_result;
            exec_state <= EXEC_START_DST;
          end
        end
        // Destination writeback (non-fused path — used when source was multi-cycle).
        EXEC_START_DST: begin
          case (dst_unit_i)
            UNIT_REGISTER: begin
              reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
              reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
              reg_in_data[dst_immediate_i[4:0]] <= src_value;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_REG_VALUE: begin
              reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
              reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
              reg_in_data[dst_immediate_i[4:0]] <= (src_value & ~TAG_MASK_32)
                                                 | (reg_raw_data[dst_immediate_i[4:0]] & TAG_MASK_32);
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_REG_TAG: begin
              reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
              reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
              reg_in_data[dst_immediate_i[4:0]] <= (reg_raw_data[dst_immediate_i[4:0]] & ~TAG_MASK_32)
                                                 | (src_value & TAG_MASK_32);
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_REG_DEREF: begin
              data_addr_o <= (reg_raw_data[dst_immediate_i[4:0]] & ~TAG_MASK_32)
                             + {29'b0, dst_immediate_i[7:5]};
              data_write_data_o <= src_value;
              data_wstrb_o <= 4'b1111;
              data_valid_o <= 1'b1;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_ALU_LEFT: begin
              alu_in_data_a[dst_immediate_i[2:0]] <= src_value;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_ALU_RIGHT: begin
              alu_in_data_b[dst_immediate_i[2:0]] <= src_value;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_ALU_OPERATOR: begin
              alu_operation[dst_immediate_i[2:0]] <= src_value[3:0];
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE: begin
              case (dst_unit_i)
                UNIT_MEMORY_OPERAND:   data_addr_o <= dst_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {24'b0, dst_immediate_i};
                default:               data_addr_o <= 32'b0;
              endcase
              data_valid_o <= 1'b1;
              data_write_data_o <= src_value;
              data_wstrb_o <= 4'b1111;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_STACK_PUSH_POP: begin
              stack_select <= dst_immediate_i[2:0];
              stack_data_in <= src_value;
              stack_push <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              stack_select <= dst_immediate_i[2:0];
              stack_offset <= dst_immediate_i[7:3];
              stack_data_in <= src_value;
              stack_index_write <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_STACK_WAIT;
            end
            UNIT_WRITE_BARRIER: begin
              barrier_data_in <= src_value;
              barrier_push <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_BARRIER_WAIT;
            end
            UNIT_PC: begin
              pc_write_o <= src_value;
              pc_write_en_o <= 1'b1;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_COND: begin
              cond_reg <= (src_value != 32'b0);
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_PC_COND: begin
              if (cond_reg) begin
                  pc_write_o <= src_value;
                  pc_write_en_o <= 1'b1;
              end
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            default: begin
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
          endcase
        end
        EXEC_DST_STACK_WAIT: begin
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;

          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (stack_ready) begin
            stack_wait_armed <= 1'b0;
            done_o <= 1'b1;
            exec_state <= EXEC_START_SRC;
          end
        end
        EXEC_DST_BARRIER_WAIT: begin
          barrier_push <= 1'b0;
          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (barrier_ready) begin
            stack_wait_armed <= 1'b0;
            done_o <= 1'b1;
            exec_state <= EXEC_START_SRC;
          end
        end
      endcase
    end // if (exec_active)
    end // else (not reset)
  end
endmodule
