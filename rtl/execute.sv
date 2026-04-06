`include "common.vh"

// Main execution engine. Each TTA instruction is a single *move* from a
// source unit to a destination unit. Data values are DATA_WIDTH bits
// (32-bit value + 4-bit sidecar tag). Addresses and PC are 32-bit.
module execute #(
    parameter NUM_REGISTERS  = 32,
    parameter NUM_ALUS       = 8,
    parameter NUM_STACKS     = 8,
    parameter STACK_DEPTH    = 32,
    parameter BARRIER_DEPTH  = 32
) (
    input wire clk_i,
    input wire rst_i,
    input wire [31:0] pc_i,
    input logic [4:0] src_unit_i,
    input logic [7:0] src_immediate_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input logic [DATA_WIDTH-1:0] src_operand_i,
    /* verilator lint_on UNUSEDSIGNAL */
    input logic [4:0] dst_unit_i,
    input logic [7:0] dst_immediate_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input logic [DATA_WIDTH-1:0] dst_operand_i,
    /* verilator lint_on UNUSEDSIGNAL */
    input logic [5:0] flags_i,

    // Data memory bus (36-bit data, 32-bit address)
    output logic [3:0]  data_wstrb_o,
    output logic [DATA_WIDTH-1:0] data_write_data_o,
    output logic [31:0] data_addr_o,
    output logic        data_valid_o,
    output logic        data_instr_o,
    input  logic        data_ready_i,
    input  logic [DATA_WIDTH-1:0] data_read_data_i,

    output logic [31:0] pc_write_o,
    output logic        pc_write_en_o,
    output logic done_o,

    input  wire         instr_valid_i,
    output wire         instr_accept_o
);
  // Architectural register file.
  reg reg_unit_select[0:NUM_REGISTERS-1];
  reg reg_unit_write[0:NUM_REGISTERS-1];
  reg [DATA_WIDTH-1:0] reg_in_data[0:NUM_REGISTERS-1];
  wire [DATA_WIDTH-1:0] reg_raw_data[0:NUM_REGISTERS-1];
  genvar gi;
  generate
    for (gi = 0; gi < NUM_REGISTERS; gi = gi + 1) begin : gen_regs
      register_unit ru (
          .rst_i(rst_i), .clk_i(clk_i),
          .sel_i(reg_unit_select[gi]), .wstrb_i(reg_unit_write[gi]),
          .data_i(reg_in_data[gi]), .data_raw_o(reg_raw_data[gi])
      );
    end
  endgenerate

  // ALU bank.
  reg [DATA_WIDTH-1:0] alu_in_data_a[0:NUM_ALUS-1];
  reg [DATA_WIDTH-1:0] alu_in_data_b[0:NUM_ALUS-1];
  wire [DATA_WIDTH-1:0] alu_out_data[0:NUM_ALUS-1];
  reg [3:0] alu_operation[0:NUM_ALUS-1];
  generate
    for (gi = 0; gi < NUM_ALUS; gi = gi + 1) begin : gen_alus
      alu_unit au (
          .oper_i(alu_operation[gi]),
          .a_data_i(alu_in_data_a[gi]), .b_data_i(alu_in_data_b[gi]),
          .data_raw_o(alu_out_data[gi])
      );
    end
  endgenerate

  // Shared multi-cycle multiply/divide unit.
  logic        muldiv_start;
  logic [3:0]  muldiv_oper;
  logic [DATA_WIDTH-1:0] muldiv_a, muldiv_b, muldiv_result;
  logic        muldiv_done;

  muldiv_unit muldiv_inst (
      .clk_i(clk_i), .rst_i(rst_i),
      .start_i(muldiv_start), .oper_i(muldiv_oper),
      .a_i(muldiv_a), .b_i(muldiv_b),
      .result_o(muldiv_result), .done_o(muldiv_done)
  );

  function [0:0] is_muldiv_op;
    input [3:0] op;
    begin
      is_muldiv_op = (op == ALU_MUL) || (op == ALU_DIV) || (op == ALU_MOD);
    end
  endfunction

  // Stack unit.
  logic [2:0] stack_select;
  logic stack_push, stack_pop;
  logic [4:0] stack_offset;
  logic stack_index_read, stack_index_write;
  logic [DATA_WIDTH-1:0] stack_data_in, stack_data_out;
  /* verilator lint_off UNUSEDSIGNAL */
  logic stack_ready, stack_overflow, stack_underflow;
  /* verilator lint_on UNUSEDSIGNAL */

  stack_unit #(.NUM_STACKS(NUM_STACKS), .STACK_DEPTH(STACK_DEPTH))
  stack_unit_inst (
      .clk_i(clk_i), .rst_i(rst_i),
      .stack_select_i(stack_select),
      .stack_push_i(stack_push), .stack_pop_i(stack_pop),
      .stack_offset_i(stack_offset),
      .stack_index_read_i(stack_index_read), .stack_index_write_i(stack_index_write),
      .data_i(stack_data_in), .data_o(stack_data_out),
      .stack_ready_o(stack_ready),
      .stack_overflow_o(stack_overflow), .stack_underflow_o(stack_underflow)
  );

  // Write barrier FIFO.
  logic barrier_push, barrier_pop;
  logic [DATA_WIDTH-1:0] barrier_data_in, barrier_data_out;
  logic barrier_ready;
  /* verilator lint_off UNUSEDSIGNAL */
  logic barrier_empty, barrier_full, barrier_overflow;
  /* verilator lint_on UNUSEDSIGNAL */

  barrier_unit #(.BARRIER_DEPTH(BARRIER_DEPTH))
  barrier_inst (
      .clk_i(clk_i), .rst_i(rst_i),
      .push_i(barrier_push), .data_i(barrier_data_in),
      .pop_i(barrier_pop), .data_o(barrier_data_out),
      .ready_o(barrier_ready),
      .barrier_empty_o(barrier_empty), .barrier_full_o(barrier_full),
      .barrier_overflow_o(barrier_overflow)
  );

  // Heap pointer for ALLOC unit (32-bit word address).
  logic [VAL_WIDTH-1:0] heap_ptr;

  // Execution FSM.
  typedef enum {
    EXEC_START_SRC,
    EXEC_SRC_MEM_RETRIEVE,
    EXEC_SRC_STACK_WAIT,
    EXEC_SRC_BARRIER_WAIT,
    EXEC_SRC_MULDIV_WAIT,
    EXEC_START_DST,
    EXEC_DST_STACK_WAIT,
    EXEC_DST_BARRIER_WAIT,
    EXEC_DST_CALL_WAIT
  } ExecState;
  ExecState exec_state;
  logic [DATA_WIDTH-1:0] src_value;

  logic exec_active;
  assign instr_accept_o = instr_valid_i && !exec_active && !done_o;
  logic stack_wait_armed;
  logic cond_reg;

  // Byte access state.
  logic [1:0] src_byte_offset;
  logic       src_is_byte;

  // Stack tag mode: 0=RAW, 1=VALUE, 2=TAG
  logic [1:0] src_stack_tag_mode;

  // Predication.
  wire pred_if_set   = flags_i[PRED_IF_SET];
  wire pred_if_clear = flags_i[PRED_IF_CLEAR];
  wire pred_skip = (pred_if_set && !cond_reg) || (pred_if_clear && cond_reg);

  // Zero-tagged constant helper: 32-bit value with tag=0
  `define TAGGED_ZERO(val) {{TAG_WIDTH{1'b0}}, val}

  integer ii;
  always @(posedge clk_i) begin
    if (rst_i) begin
      for (ii = 0; ii < NUM_REGISTERS; ii = ii + 1) begin
        reg_unit_select[ii] <= 1'b0;
        reg_unit_write[ii] <= 1'b0;
      end
      for (ii = 0; ii < NUM_ALUS; ii = ii + 1)
        alu_operation[ii] <= 4'h0;

      stack_select <= 3'b000;
      stack_push <= 1'b0;
      stack_pop <= 1'b0;
      barrier_push <= 1'b0;
      barrier_pop <= 1'b0;
      barrier_data_in <= {DATA_WIDTH{1'b0}};
      stack_offset <= 5'b0;
      stack_index_read <= 1'b0;
      stack_index_write <= 1'b0;
      stack_data_in <= {DATA_WIDTH{1'b0}};

      muldiv_start <= 1'b0;
      muldiv_oper <= 4'h0;
      muldiv_a <= {DATA_WIDTH{1'b0}};
      muldiv_b <= {DATA_WIDTH{1'b0}};

      heap_ptr <= {VAL_WIDTH{1'b0}};

      src_byte_offset <= 2'b0;
      src_is_byte <= 1'b0;
      src_stack_tag_mode <= 2'b0;

      exec_state <= EXEC_START_SRC;
      cond_reg <= 1'b0;
      src_value <= {DATA_WIDTH{1'b0}};
      stack_wait_armed <= 1'b0;
      pc_write_o <= 32'b0;
      pc_write_en_o <= 1'b0;
      exec_active <= 1'b0;
      data_valid_o <= 1'b0;
      data_instr_o <= 1'b0;
      data_wstrb_o <= 4'b0000;
      data_addr_o <= 32'b0;
      data_write_data_o <= {DATA_WIDTH{1'b0}};

      done_o <= 1'b0;
    end else begin
      reg run_execute;
      run_execute = (exec_active || instr_accept_o) && !done_o;

      if (done_o) begin
        done_o <= 1'b0;
        pc_write_en_o <= 1'b0;
        exec_active <= 1'b0;
      end

      if (instr_accept_o)
        exec_active <= 1'b1;

      if (run_execute) begin
      case (exec_state)
        EXEC_START_SRC: begin
          reg src_resolved;
          reg [DATA_WIDTH-1:0] resolved_src;
          src_resolved = 1'b0;
          resolved_src = {DATA_WIDTH{1'b0}};
          done_o <= 1'b0;
          pc_write_en_o <= 1'b0;
          for (ii = 0; ii < NUM_REGISTERS; ii = ii + 1) begin
            reg_unit_select[ii] <= 1'b0;
            reg_unit_write[ii] <= 1'b0;
          end
          data_valid_o <= 1'b0;
          data_wstrb_o <= 4'b0000;
          data_instr_o <= 1'b0;

          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;
          barrier_push <= 1'b0;
          barrier_pop <= 1'b0;
          muldiv_start <= 1'b0;

          if (pred_skip) begin
            done_o <= 1'b1;
            exec_state <= EXEC_START_SRC;
          end else
          case (src_unit_i)
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE: begin
              case (src_unit_i)
                UNIT_MEMORY_OPERAND:   data_addr_o <= src_operand_i[VAL_WIDTH-1:0];
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {24'b0, src_immediate_i};
                default:               data_addr_o <= 32'b0;
              endcase
              data_valid_o <= 1'b1;
              src_is_byte <= 1'b0;
              exec_state <= EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_REGISTER: begin
              reg_unit_select[src_immediate_i[4:0]] <= 1'b1;
              resolved_src = reg_raw_data[src_immediate_i[4:0]];
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_VALUE: begin
              // Value only: zero the tag, keep 32-bit value
              resolved_src = `TAGGED_ZERO(reg_raw_data[src_immediate_i[4:0]][VAL_WIDTH-1:0]);
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_TAG: begin
              // Tag only: return tag bits in low value bits, tag=0
              resolved_src = `TAGGED_ZERO({{(VAL_WIDTH-TAG_WIDTH){1'b0}},
                              reg_raw_data[src_immediate_i[4:0]][DATA_WIDTH-1:VAL_WIDTH]});
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_REG_DEREF: begin
              // Address is the full 32-bit value (no tag stripping needed!)
              data_addr_o <= reg_raw_data[src_immediate_i[4:0]][VAL_WIDTH-1:0]
                             + {29'b0, src_immediate_i[7:5]};
              data_valid_o <= 1'b1;
              src_is_byte <= 1'b0;
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
              // Literal with tag=0
              resolved_src = `TAGGED_ZERO({{(VAL_WIDTH-8){1'b0}}, src_immediate_i});
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_ABS_OPERAND: begin
              // 32-bit operand with tag=0
              resolved_src = `TAGGED_ZERO(src_operand_i[VAL_WIDTH-1:0]);
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_PC: begin
              resolved_src = `TAGGED_ZERO(pc_i);
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_COND: begin
              resolved_src = `TAGGED_ZERO({{(VAL_WIDTH-1){1'b0}}, cond_reg});
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_STACK_PUSH_POP: begin
              stack_select <= src_immediate_i[2:0];
              stack_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b00;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              stack_select <= src_immediate_i[2:0];
              stack_offset <= src_immediate_i[7:3];
              stack_index_read <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b00;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_WRITE_BARRIER: begin
              barrier_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_SRC_BARRIER_WAIT;
            end
            UNIT_MEM_BYTE: begin
              data_addr_o <= src_operand_i[VAL_WIDTH-1:0];
              data_valid_o <= 1'b1;
              src_byte_offset <= src_immediate_i[1:0];
              src_is_byte <= 1'b1;
              exec_state <= EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_STACK_POP_VALUE: begin
              stack_select <= src_immediate_i[2:0];
              stack_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b01;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_POP_TAG: begin
              stack_select <= src_immediate_i[2:0];
              stack_pop <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b10;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_PEEK_VALUE: begin
              stack_select <= src_immediate_i[2:0];
              stack_offset <= src_immediate_i[7:3];
              stack_index_read <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b01;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_PEEK_TAG: begin
              stack_select <= src_immediate_i[2:0];
              stack_offset <= src_immediate_i[7:3];
              stack_index_read <= 1'b1;
              stack_wait_armed <= 1'b0;
              src_stack_tag_mode <= 2'b10;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_ALLOC_PTR: begin
              // Return {si[3:0] as tag, heap_ptr} — tagged pointer to next alloc
              resolved_src = {src_immediate_i[TAG_WIDTH-1:0], heap_ptr};
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_NONE: begin
              resolved_src = {DATA_WIDTH{1'b0}};
              src_value <= resolved_src;
              if (dst_unit_i != UNIT_NONE)
                src_resolved = 1'b1;
              else begin
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
            end
            default: begin
              resolved_src = {DATA_WIDTH{1'b0}};
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
          endcase

          // Fused destination — same cycle when source resolved immediately.
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
                // Preserve existing tag, replace value
                reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
                reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
                reg_in_data[dst_immediate_i[4:0]] <=
                    {reg_raw_data[dst_immediate_i[4:0]][DATA_WIDTH-1:VAL_WIDTH],
                     resolved_src[VAL_WIDTH-1:0]};
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_REG_TAG: begin
                // Preserve existing value, replace tag
                reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
                reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
                reg_in_data[dst_immediate_i[4:0]] <=
                    {resolved_src[TAG_WIDTH-1:0],
                     reg_raw_data[dst_immediate_i[4:0]][VAL_WIDTH-1:0]};
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_REG_DEREF: begin
                data_addr_o <= reg_raw_data[dst_immediate_i[4:0]][VAL_WIDTH-1:0]
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
                pc_write_o <= resolved_src[VAL_WIDTH-1:0];
                pc_write_en_o <= 1'b1;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_COND: begin
                cond_reg <= (resolved_src[VAL_WIDTH-1:0] != {VAL_WIDTH{1'b0}});
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_PC_COND: begin
                if (cond_reg) begin
                  pc_write_o <= resolved_src[VAL_WIDTH-1:0];
                  pc_write_en_o <= 1'b1;
                end
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_TAG_CMP: begin
                cond_reg <= (resolved_src[DATA_WIDTH-1:VAL_WIDTH] == dst_immediate_i[TAG_WIDTH-1:0]);
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_ALLOC: begin
                // Store value at heap_ptr, fire-and-forget write, bump HP
                data_addr_o <= heap_ptr;
                data_write_data_o <= resolved_src;
                data_wstrb_o <= 4'b1111;
                data_valid_o <= 1'b1;
                heap_ptr <= heap_ptr + 1;
                done_o <= 1'b1;
                exec_state <= EXEC_START_SRC;
              end
              UNIT_CALL: begin
                // Push return address (pc_i = next sequential PC) to stack 1, then jump
                stack_select <= 3'd1;
                stack_data_in <= `TAGGED_ZERO(pc_i);
                stack_push <= 1'b1;
                stack_wait_armed <= 1'b0;
                exec_state <= EXEC_DST_CALL_WAIT;
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
            if (src_is_byte) begin
              /* verilator lint_off UNUSEDSIGNAL */
              reg [VAL_WIDTH-1:0] shifted;
              /* verilator lint_on UNUSEDSIGNAL */
              shifted = data_read_data_i[VAL_WIDTH-1:0] >> (src_byte_offset * 8);
              src_value <= `TAGGED_ZERO({{(VAL_WIDTH-8){1'b0}}, shifted[7:0]});
            end else
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
            case (src_stack_tag_mode)
              2'b01:   // VALUE: zero tag, keep value
                src_value <= `TAGGED_ZERO(stack_data_out[VAL_WIDTH-1:0]);
              2'b10:   // TAG: tag bits in low value bits, tag=0
                src_value <= `TAGGED_ZERO({{(VAL_WIDTH-TAG_WIDTH){1'b0}},
                             stack_data_out[DATA_WIDTH-1:VAL_WIDTH]});
              default: // RAW: pass through full tagged value
                src_value <= stack_data_out;
            endcase
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
        // Non-fused destination writeback.
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
              reg_in_data[dst_immediate_i[4:0]] <=
                  {reg_raw_data[dst_immediate_i[4:0]][DATA_WIDTH-1:VAL_WIDTH],
                   src_value[VAL_WIDTH-1:0]};
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_REG_TAG: begin
              reg_unit_select[dst_immediate_i[4:0]] <= 1'b1;
              reg_unit_write[dst_immediate_i[4:0]] <= 1'b1;
              reg_in_data[dst_immediate_i[4:0]] <=
                  {src_value[TAG_WIDTH-1:0],
                   reg_raw_data[dst_immediate_i[4:0]][VAL_WIDTH-1:0]};
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_REG_DEREF: begin
              data_addr_o <= reg_raw_data[dst_immediate_i[4:0]][VAL_WIDTH-1:0]
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
                UNIT_MEMORY_OPERAND:   data_addr_o <= dst_operand_i[VAL_WIDTH-1:0];
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {24'b0, dst_immediate_i};
                default:               data_addr_o <= 32'b0;
              endcase
              data_valid_o <= 1'b1;
              data_write_data_o <= src_value;
              data_wstrb_o <= 4'b1111;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_MEM_BYTE: begin
              data_addr_o <= dst_operand_i[VAL_WIDTH-1:0];
              data_valid_o <= 1'b1;
              data_write_data_o <= `TAGGED_ZERO(src_value[VAL_WIDTH-1:0] << (dst_immediate_i[1:0] * 8));
              data_wstrb_o <= 4'b0001 << dst_immediate_i[1:0];
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
              pc_write_o <= src_value[VAL_WIDTH-1:0];
              pc_write_en_o <= 1'b1;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_COND: begin
              cond_reg <= (src_value[VAL_WIDTH-1:0] != {VAL_WIDTH{1'b0}});
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_PC_COND: begin
              if (cond_reg) begin
                  pc_write_o <= src_value[VAL_WIDTH-1:0];
                  pc_write_en_o <= 1'b1;
              end
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_TAG_CMP: begin
              cond_reg <= (src_value[DATA_WIDTH-1:VAL_WIDTH] == dst_immediate_i[TAG_WIDTH-1:0]);
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_ALLOC: begin
              data_addr_o <= heap_ptr;
              data_write_data_o <= src_value;
              data_wstrb_o <= 4'b1111;
              data_valid_o <= 1'b1;
              heap_ptr <= heap_ptr + 1;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_CALL: begin
              stack_select <= 3'd1;
              stack_data_in <= `TAGGED_ZERO(pc_i);
              stack_push <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_CALL_WAIT;
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
        EXEC_DST_CALL_WAIT: begin
          // Wait for stack push (return address) to complete, then jump.
          stack_push <= 1'b0;
          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (stack_ready) begin
            stack_wait_armed <= 1'b0;
            pc_write_o <= src_value[VAL_WIDTH-1:0];
            pc_write_en_o <= 1'b1;
            done_o <= 1'b1;
            exec_state <= EXEC_START_SRC;
          end
        end
      endcase
    end
    end
  end

  `undef TAGGED_ZERO
endmodule
