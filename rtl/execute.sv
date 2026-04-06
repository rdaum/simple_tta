`define NUM_REGISTERS 32  // Architectural register count (addressed by imm[4:0])
`define NUM_ALUS 8         // Independent ALU lanes (addressed by imm[2:0])

// Main execution engine. Each TTA instruction is a single *move* from a
// source unit to a destination unit. This module resolves the source value
// (EXEC_START_SRC phase), then writes it to the destination (EXEC_START_DST
// phase). Memory and stack accesses require extra wait states; register and
// ALU moves typically complete in one or two cycles.
//
// Immediate field bit layout (12 bits from the instruction word):
//   Registers:       [4:0] = register index (0-31)
//   ALU lanes:       [2:0] = lane index (0-7)
//   Stack push/pop:  [2:0] = stack ID (0-7)
//   Stack index:     [2:0] = stack ID, [8:3] = offset from top (0-63)
//   Memory imm:      full 12 bits zero-extended to a 32-bit word address
module execute (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)
    input wire [31:0] pc_i,             // Current program counter (for UNIT_PC reads)
    input logic [3:0] src_unit_i,       // Source unit selector (from decoder)
    input logic [11:0] src_immediate_i, // Source immediate field
    input logic [31:0] src_operand_i,   // Source 32-bit operand (from sequencer)
    input logic [3:0] dst_unit_i,       // Destination unit selector (from decoder)
    input logic [11:0] dst_immediate_i, // Destination immediate field
    input logic [31:0] dst_operand_i,   // Destination 32-bit operand (from sequencer)

    // Data memory bus (explicit ports replacing bus_if.master)
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
  // Architectural register file: 32 independent 32-bit cells.
  reg reg_unit_select[0:`NUM_REGISTERS-1];
  reg reg_unit_write[0:`NUM_REGISTERS-1];
  reg [31:0] reg_in_data[0:`NUM_REGISTERS-1];
  wire [31:0] reg_raw_data[0:`NUM_REGISTERS-1];
  genvar gi;
  generate
    for (gi = 0; gi < `NUM_REGISTERS; gi = gi + 1) begin : gen_regs
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

  // ALU bank: 8 independently addressable compute lanes.
  // Results are combinational — available immediately from stored operands.
  reg [31:0] alu_in_data_a[0:`NUM_ALUS-1];
  reg [31:0] alu_in_data_b[0:`NUM_ALUS-1];
  wire [31:0] alu_out_data[0:`NUM_ALUS-1];
  reg [3:0] alu_operation[0:`NUM_ALUS-1];
  generate
    for (gi = 0; gi < `NUM_ALUS; gi = gi + 1) begin : gen_alus
      alu_unit au (
          .oper_i(alu_operation[gi]),
          .a_data_i(alu_in_data_a[gi]),
          .b_data_i(alu_in_data_b[gi]),
          .data_raw_o(alu_out_data[gi])
      );
    end
  endgenerate

  // Shared stack unit implementing 8 logical stacks.
  logic [2:0] stack_select;
  logic stack_push, stack_pop;
  logic [5:0] stack_offset;
  logic stack_index_read, stack_index_write;
  logic [31:0] stack_data_in, stack_data_out;
  /* verilator lint_off UNUSEDSIGNAL */
  logic stack_ready, stack_overflow, stack_underflow;
  /* verilator lint_on UNUSEDSIGNAL */

  stack_unit stack_unit_inst (
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

  barrier_unit barrier_inst (
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
  //
  //  EXEC_START_SRC ──→ (immediate units) ──→ EXEC_START_DST ──→ done
  //       │                                         │
  //       ├─→ EXEC_SRC_MEM_RETRIEVE ──→ EXEC_START_DST
  //       └─→ EXEC_SRC_STACK_WAIT   ──→ EXEC_START_DST
  //                                         │
  //                                         └─→ EXEC_DST_STACK_WAIT ──→ done
  typedef enum {
    EXEC_START_SRC,        // Begin source resolution (fuses dst for immediate moves)
    EXEC_SRC_MEM_RETRIEVE, // Wait for data_ready_i on a memory read
    EXEC_SRC_STACK_WAIT,   // Wait for stack_ready after pop / peek
    EXEC_SRC_BARRIER_WAIT, // Wait for barrier_ready after pop
    EXEC_START_DST,        // Route src_value to the destination unit
    EXEC_DST_STACK_WAIT,   // Wait for stack_ready after push / poke
    EXEC_DST_BARRIER_WAIT  // Wait for barrier_ready after push
  } ExecState;
  ExecState exec_state;
  logic [31:0] src_value;

  // Execute runs autonomously once triggered via the valid/accept handshake.
  // exec_active stays high through multi-cycle operations until done_o fires.
  logic exec_active;

  // Accept is combinational: fires for exactly one cycle when a valid
  // instruction is presented and execute is idle.
  assign instr_accept_o = instr_valid_i && !exec_active && !done_o;
  logic stack_wait_armed;

  // 1-bit condition register for conditional branches.
  // Written via UNIT_COND (nonzero → 1, zero → 0).
  // Read via UNIT_COND (returns 0 or 1).
  // Tested by UNIT_PC_COND (jump only if set).
  logic cond_reg;

  // Sub-word access helpers: extract width and byte offset from immediate fields.
  // Only applicable for MEMORY_OPERAND and REGISTER_POINTER where the
  // immediate field is not used as the primary address. MEMORY_IMMEDIATE
  // and UNIT_REGISTER always use word access (their immediate bits have
  // different meanings).
  logic [1:0] src_width, dst_width;
  logic [1:0] src_byte_offset, dst_byte_offset;
  always_comb begin
    if (src_unit_i == UNIT_MEMORY_IMMEDIATE || src_unit_i == UNIT_REGISTER) begin
      src_width       = ACCESS_WORD;
      src_byte_offset = 2'b00;
    end else begin
      src_width       = src_immediate_i[11:10];
      src_byte_offset = src_immediate_i[9:8];
    end
    if (dst_unit_i == UNIT_MEMORY_IMMEDIATE || dst_unit_i == UNIT_REGISTER) begin
      dst_width       = ACCESS_WORD;
      dst_byte_offset = 2'b00;
    end else begin
      dst_width       = dst_immediate_i[11:10];
      dst_byte_offset = dst_immediate_i[9:8];
    end
  end

  // Register access mode helpers: decode mode, index, and DEREF offset
  // from the immediate field of UNIT_REGISTER instructions.
  logic [1:0] src_reg_mode, dst_reg_mode;
  logic [4:0]   src_reg_idx,  dst_reg_idx;
  logic [2:0]   src_deref_offset, dst_deref_offset;
  always_comb begin
    src_reg_mode     = src_immediate_i[6:5];
    src_reg_idx      = src_immediate_i[4:0];
    src_deref_offset = src_immediate_i[9:7];
    dst_reg_mode     = dst_immediate_i[6:5];
    dst_reg_idx      = dst_immediate_i[4:0];
    dst_deref_offset = dst_immediate_i[9:7];
  end

  // Stack access mode helpers: decode tag mode from the immediate field.
  //   STACK_PUSH_POP: imm[4:3] = mode (RAW/VALUE/TAG)
  //   STACK_INDEX:    imm[10:9] = mode (RAW/VALUE/TAG)
  // Only RAW/VALUE/TAG are meaningful (DEREF is not applicable to stacks).
  logic [1:0] src_stack_mode;
  always_comb begin
    case (src_unit_i)
      UNIT_STACK_PUSH_POP: src_stack_mode = src_immediate_i[4:3];
      UNIT_STACK_INDEX:    src_stack_mode = src_immediate_i[10:9];
      default:             src_stack_mode = 2'b00; // RAW
    endcase
  end

  // Compute write strobes from access width and byte offset.
  function automatic [3:0] width_to_wstrb;
    input [1:0] w;
    input [1:0] off;
    case (w)
      ACCESS_BYTE:     width_to_wstrb = 4'b0001 << off;
      ACCESS_HALFWORD: width_to_wstrb = 4'b0011 << off;
      default:         width_to_wstrb = 4'b1111;
    endcase
  endfunction

  // Extract and zero-extend sub-word data from a 32-bit bus read.
  /* verilator lint_off UNUSEDSIGNAL */
  function automatic [31:0] extract_read;
    input [31:0] data;
    input [1:0] w;
    input [1:0] off;
    reg [31:0] shifted;
    begin
      shifted = data >> (off * 8);
      case (w)
        ACCESS_BYTE:     extract_read = {24'b0, shifted[7:0]};
        ACCESS_HALFWORD: extract_read = {16'b0, shifted[15:0]};
        default:         extract_read = data;
      endcase
    end
  endfunction
  /* verilator lint_on UNUSEDSIGNAL */

  integer ii;
  always @(posedge clk_i) begin
    if (rst_i) begin
      for (ii = 0; ii < `NUM_REGISTERS; ii = ii + 1) begin
        reg_unit_select[ii] <= 1'b0;
        reg_unit_write[ii] <= 1'b0;
      end
      for (ii = 0; ii < `NUM_ALUS; ii = ii + 1)
        alu_operation[ii] <= 4'h0;

      // Initialize stack and barrier signals
      stack_select <= 3'b000;
      stack_push <= 1'b0;
      stack_pop <= 1'b0;
      barrier_push <= 1'b0;
      barrier_pop <= 1'b0;
      barrier_data_in <= 32'b0;
      stack_offset <= 6'b0;
      stack_index_read <= 1'b0;
      stack_index_write <= 1'b0;
      stack_data_in <= 32'b0;

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
      // Run the FSM on the accept cycle (decoder outputs are now
      // combinational from the queue head) AND while exec_active for
      // multi-cycle operations. This eliminates the 1-cycle accept
      // overhead — fused moves complete on the accept cycle itself.
      // run_execute is combinational within this block, equivalent to a wire.
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
          // src_resolved: set by immediate sources so the destination can
          // be evaluated in the same cycle (fused src+dst, no extra state).
          reg src_resolved;
          reg [31:0] resolved_src;
          src_resolved = 1'b0;
          resolved_src = 32'b0;
          done_o <= 1'b0;
          pc_write_en_o <= 1'b0;
          for (ii = 0; ii < `NUM_REGISTERS; ii = ii + 1) begin
            reg_unit_select[ii] <= 1'b0;
            reg_unit_write[ii] <= 1'b0;
          end
          data_valid_o <= 1'b0;
          data_wstrb_o <= 4'b0000;
          data_instr_o <= 1'b0;

          // Clear stack and barrier signals
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;
          barrier_push <= 1'b0;
          barrier_pop <= 1'b0;
          case (src_unit_i)
            // Source is memory-backed, so begin a bus read.
            // Width (imm[11:10]) and byte offset (imm[9:8]) are applied
            // when the data returns in EXEC_SRC_MEM_RETRIEVE. These fields
            // are only active for MEMORY_OPERAND; MEMORY_IMMEDIATE always
            // does a full-word read. For register-indirect, use DEREF mode.
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE: begin
              case (src_unit_i)
                UNIT_MEMORY_OPERAND: data_addr_o <= src_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {20'b0, src_immediate_i};
                default: data_addr_o <= 32'b0;
              endcase
              data_valid_o <= 1'b1;
              exec_state <= EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_REGISTER: begin
              case (src_reg_mode)
                REG_RAW: begin
                  reg_unit_select[src_reg_idx] <= 1'b1;
                  resolved_src = reg_raw_data[src_reg_idx];
                  src_value <= resolved_src;
                  src_resolved = 1'b1;
                end
                REG_VALUE: begin
                  resolved_src = reg_raw_data[src_reg_idx] & ~TAG_MASK_32;
                  src_value <= resolved_src;
                  src_resolved = 1'b1;
                end
                REG_TAG: begin
                  resolved_src = reg_raw_data[src_reg_idx] & TAG_MASK_32;
                  src_value <= resolved_src;
                  src_resolved = 1'b1;
                end
                REG_DEREF: begin
                  data_addr_o <= (reg_raw_data[src_reg_idx] & ~TAG_MASK_32)
                                 + {29'b0, src_deref_offset};
                  data_valid_o <= 1'b1;
                  exec_state <= EXEC_SRC_MEM_RETRIEVE;
                end
              endcase
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
              // ALU results are combinational — read directly, no wait.
              resolved_src = alu_out_data[src_immediate_i[2:0]];
              src_value <= resolved_src;
              src_resolved = 1'b1;
            end
            UNIT_ABS_IMMEDIATE: begin
              resolved_src = {20'b0, src_immediate_i};
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
              stack_offset <= src_immediate_i[8:3];
              stack_index_read <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_SRC_STACK_WAIT;
            end
            UNIT_WRITE_BARRIER: begin
              // Pop next entry from the barrier FIFO (GC drains it).
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
            // Inline the EXEC_START_DST logic. For destinations that need
            // extra cycles (stack), fall back to the EXEC_START_DST state.
            case (dst_unit_i)
              UNIT_REGISTER: begin
                case (dst_reg_mode)
                  REG_RAW: begin
                    reg_unit_select[dst_reg_idx] <= 1'b1;
                    reg_unit_write[dst_reg_idx] <= 1'b1;
                    reg_in_data[dst_reg_idx] <= resolved_src;
                  end
                  REG_VALUE: begin
                    reg_unit_select[dst_reg_idx] <= 1'b1;
                    reg_unit_write[dst_reg_idx] <= 1'b1;
                    reg_in_data[dst_reg_idx] <= (resolved_src & ~TAG_MASK_32)
                                              | (reg_raw_data[dst_reg_idx] & TAG_MASK_32);
                  end
                  REG_TAG: begin
                    reg_unit_select[dst_reg_idx] <= 1'b1;
                    reg_unit_write[dst_reg_idx] <= 1'b1;
                    reg_in_data[dst_reg_idx] <= (reg_raw_data[dst_reg_idx] & ~TAG_MASK_32)
                                              | (resolved_src & TAG_MASK_32);
                  end
                  REG_DEREF: begin
                    data_addr_o <= (reg_raw_data[dst_reg_idx] & ~TAG_MASK_32)
                                   + {29'b0, dst_deref_offset};
                    data_write_data_o <= resolved_src;
                    data_wstrb_o <= 4'b1111;
                    data_valid_o <= 1'b1;
                  end
                endcase
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
                // Destinations needing extra cycles (memory, stack) go
                // through the EXEC_START_DST state on the next cycle.
                exec_state <= EXEC_START_DST;
              end
            endcase
          end

        end
        EXEC_SRC_MEM_RETRIEVE: begin
          if (data_ready_i) begin
            // Extract the requested byte/halfword/word and zero-extend.
            src_value <= extract_read(data_read_data_i, src_width, src_byte_offset);
            data_valid_o <= 1'b0;
            exec_state <= EXEC_START_DST;
          end
        end
        EXEC_SRC_STACK_WAIT: begin
          // Clear stack control signals after first cycle
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;

          // Allow one cycle for stack_unit to consume the command before
          // sampling ready/data on the following cycle.
          if (!stack_wait_armed) begin
            stack_wait_armed <= 1'b1;
          end else if (stack_ready) begin
            // Apply tag mode mask to the stack read value.
            case (src_stack_mode)
              2'b01:   src_value <= stack_data_out & ~TAG_MASK_32; // VALUE
              2'b10:   src_value <= stack_data_out & TAG_MASK_32;  // TAG
              default: src_value <= stack_data_out;                // RAW
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
        // Destination writeback consumes the resolved source value and applies
        // the instruction side effect.
        EXEC_START_DST: begin
          case (dst_unit_i)
            UNIT_REGISTER: begin
              case (dst_reg_mode)
                REG_RAW: begin
                  reg_unit_select[dst_reg_idx] <= 1'b1;
                  reg_unit_write[dst_reg_idx] <= 1'b1;
                  reg_in_data[dst_reg_idx] <= src_value;
                end
                REG_VALUE: begin
                  // Preserve tag bits, replace payload.
                  reg_unit_select[dst_reg_idx] <= 1'b1;
                  reg_unit_write[dst_reg_idx] <= 1'b1;
                  reg_in_data[dst_reg_idx] <= (src_value & ~TAG_MASK_32)
                                            | (reg_raw_data[dst_reg_idx] & TAG_MASK_32);
                end
                REG_TAG: begin
                  // Preserve payload, replace tag bits.
                  reg_unit_select[dst_reg_idx] <= 1'b1;
                  reg_unit_write[dst_reg_idx] <= 1'b1;
                  reg_in_data[dst_reg_idx] <= (reg_raw_data[dst_reg_idx] & ~TAG_MASK_32)
                                            | (src_value & TAG_MASK_32);
                end
                REG_DEREF: begin
                  // Store src_value to memory at (reg & ~TAG_MASK) + offset.
                  data_addr_o <= (reg_raw_data[dst_reg_idx] & ~TAG_MASK_32)
                                 + {29'b0, dst_deref_offset};
                  data_write_data_o <= src_value;
                  data_wstrb_o <= 4'b1111;
                  data_valid_o <= 1'b1;
                end
              endcase
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_ALU_LEFT: begin
              // Writing ALU_LEFT/RIGHT/OPERATOR configures an ALU lane.
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
                UNIT_MEMORY_OPERAND: data_addr_o <= dst_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_addr_o <= {20'b0, dst_immediate_i};
                default: data_addr_o <= 32'b0;
              endcase

              data_valid_o <= 1'b1;
              // For sub-word writes, shift data into the correct byte lane(s).
              // Word writes pass data and strobes through unchanged.
              if (dst_width == ACCESS_WORD) begin
                data_write_data_o <= src_value;
                data_wstrb_o <= 4'b1111;
              end else begin
                data_write_data_o <= src_value << (dst_byte_offset * 8);
                data_wstrb_o <= width_to_wstrb(dst_width, dst_byte_offset);
              end
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_STACK_PUSH_POP: begin
              // Push writes src_value to the selected stack.
              stack_select <= dst_immediate_i[2:0];  // Stack ID from bits 2:0
              stack_data_in <= src_value;
              stack_push <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              // Indexed writes implement poke-like behavior.
              stack_select <= dst_immediate_i[2:0];     // Stack ID from bits 2:0
              stack_offset <= dst_immediate_i[8:3];     // Offset from bits 8:3 (6 bits)
              stack_data_in <= src_value;
              stack_index_write <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_STACK_WAIT;
            end
            UNIT_WRITE_BARRIER: begin
              // Push address to the write barrier FIFO for GC.
              barrier_data_in <= src_value;
              barrier_push <= 1'b1;
              stack_wait_armed <= 1'b0;
              exec_state <= EXEC_DST_BARRIER_WAIT;
            end
            UNIT_PC: begin
              // Unconditional jump: set PC to src_value.
              pc_write_o <= src_value;
              pc_write_en_o <= 1'b1;
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_COND: begin
              // Write condition register: nonzero → 1, zero → 0.
              cond_reg <= (src_value != 32'b0);
              done_o <= 1'b1;
              exec_state <= EXEC_START_SRC;
            end
            UNIT_PC_COND: begin
              // Conditional jump: set PC to src_value only if cond_reg is set.
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
          // Clear the one-cycle stack command strobes while waiting for ready.
          stack_push <= 1'b0;
          stack_pop <= 1'b0;
          stack_index_read <= 1'b0;
          stack_index_write <= 1'b0;

          // Allow one cycle for stack_unit to consume the command before
          // checking for completion on the following cycle.
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
