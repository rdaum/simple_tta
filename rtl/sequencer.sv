// Fetch state machine for variable-length instructions (word-addressed).
//
// Instructions are 1, 2, or 3 words long:
//   Word 0: opcode (always present)
//   Word 1: source operand  (present when src unit is MEMORY_OPERAND or ABS_OPERAND)
//   Word 2: destination operand (present when dst unit is MEMORY_OPERAND or ABS_OPERAND)
//
// The sequencer prefetches: fetch stages run unconditionally so the bus
// read overlaps with execute. Fetched data is staged in prefetch registers
// and only promoted to the outputs (op_o, src_operand_o, dst_operand_o)
// at handoff, keeping the decoder outputs stable while execute runs.
//
// On a taken branch, pc_write_en_i discards any in-flight prefetch and
// restarts from the new PC.
module sequencer (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)
    bus_if.master instr_bus,            // Instruction fetch bus
    output logic [31:0] pc_o,           // Current program counter (word address)
    output logic [31:0] op_o,           // Fetched opcode word for the decoder
    output logic [31:0] src_operand_o,  // 32-bit source operand (when needed)
    output logic [31:0] dst_operand_o,  // 32-bit destination operand (when needed)

    input  logic        exec_busy_i,    // High while execute is processing

    // PC override from execute (for jumps / conditional branches).
    input  logic [31:0] pc_write_i,     // New PC value
    input  logic        pc_write_en_i,  // High to override PC with pc_write_i

    output logic done_o                 // Pulses high when instruction is ready
);

  enum {
    SEQ_START,                  // Issue bus read for opcode at pc_o
    SEQ_READ_OPCODE,            // Wait for bus ready, latch opcode + decide on operands
    SEQ_READ_SRC_OPERAND,       // Wait for source operand word from bus
    SEQ_READ_DST_OPERAND_START, // Issue bus read for destination operand word
    SEQ_READ_DST_OPERAND,       // Wait for destination operand word from bus
    SEQ_READY                   // Fetch complete, waiting for execute to become idle
  } sequencer_state;

  // Prefetch buffer: fetch results are staged here and only promoted
  // to op_o / src_operand_o / dst_operand_o at handoff, so the decoder
  // outputs remain stable while execute is running.
  logic [31:0] prefetch_op;
  logic [31:0] prefetch_src_operand;
  logic [31:0] prefetch_dst_operand;

  function automatic logic needs_src_op(logic [31:0] raw_op);
    Unit su = Unit'(raw_op[3:0]);
    return su == UNIT_MEMORY_OPERAND || su == UNIT_ABS_OPERAND;
  endfunction

  function automatic logic needs_dst_op(logic [31:0] raw_op);
    Unit du = Unit'(raw_op[19:16]);
    return du == UNIT_MEMORY_OPERAND || du == UNIT_ABS_OPERAND;
  endfunction

  // Combinational next-PC values for use inside the sequential block.
  // Avoids read-after-write issues with non-blocking pc_o updates.
  wire [31:0] pc_plus_1 = pc_o + 1;
  wire [31:0] pc_plus_2 = pc_o + 2;

  always @(posedge clk_i) begin
    if (rst_i) begin
      pc_o <= 32'b0;
      op_o <= 32'b0;
      src_operand_o <= 32'b0;
      dst_operand_o <= 32'b0;
      prefetch_op <= 32'b0;
      prefetch_src_operand <= 32'b0;
      prefetch_dst_operand <= 32'b0;
      done_o <= 1'b0;
      sequencer_state <= SEQ_START;
      instr_bus.valid <= 1'b0;
      instr_bus.instr <= 1'b0;
      instr_bus.addr <= 32'b0;
    end else if (pc_write_en_i) begin
      // Branch taken: discard any in-flight prefetch, restart from new PC.
      pc_o <= pc_write_i;
      instr_bus.valid <= 1'b0;
      done_o <= 1'b0;
      sequencer_state <= SEQ_START;
    end else begin
      // Auto-clear done_o after one cycle (pulse semantics).
      // done_o is set via <= so it's visible to execute for exactly one
      // cycle, regardless of always-block evaluation order.
      if (done_o)
        done_o <= 1'b0;

      case (sequencer_state)
        // Fetch stages run unconditionally — not gated by exec_busy_i.
        // This is the prefetch: the bus read overlaps with execute.
        SEQ_START: begin
          instr_bus.valid <= 1'b1;
          instr_bus.instr <= 1'b1;
          instr_bus.addr <= pc_o;
          sequencer_state <= SEQ_READ_OPCODE;
        end

        SEQ_READ_OPCODE: begin
          if (instr_bus.ready) begin
            prefetch_op <= instr_bus.read_data;
            if (needs_src_op(instr_bus.read_data) || needs_dst_op(instr_bus.read_data)) begin
              // Multi-word instruction: fetch the next operand word.
              instr_bus.valid <= 1'b1;
              instr_bus.instr <= 1'b0;
              instr_bus.addr  <= pc_o + 1;
              if (needs_src_op(instr_bus.read_data))
                sequencer_state <= SEQ_READ_SRC_OPERAND;
              else
                sequencer_state <= SEQ_READ_DST_OPERAND;
            end else begin
              // 1-word instruction. Advance PC, try to hand off.
              pc_o <= pc_plus_1;
              instr_bus.valid <= 1'b0;
              if (!exec_busy_i) begin
                // Execute is idle: promote prefetch to outputs and fire.
                op_o <= instr_bus.read_data;
                done_o <= 1'b1;
                sequencer_state <= SEQ_START;
              end else begin
                sequencer_state <= SEQ_READY;
              end
            end
          end
        end

        SEQ_READ_SRC_OPERAND: begin
          if (instr_bus.ready) begin
            prefetch_src_operand <= instr_bus.read_data;
            if (needs_dst_op(prefetch_op)) begin
              // 3-word instruction: still need dst operand.
              instr_bus.valid <= 1'b0;
              pc_o <= pc_o + 1;
              sequencer_state <= SEQ_READ_DST_OPERAND_START;
            end else begin
              // 2-word instruction. Advance PC past both words.
              pc_o <= pc_plus_2;
              instr_bus.valid <= 1'b0;
              if (!exec_busy_i) begin
                op_o <= prefetch_op;
                src_operand_o <= instr_bus.read_data;
                done_o <= 1'b1;
                sequencer_state <= SEQ_START;
              end else begin
                sequencer_state <= SEQ_READY;
              end
            end
          end
        end

        SEQ_READ_DST_OPERAND_START: begin
          // Issue a bus read for the destination operand at pc+1.
          instr_bus.addr  <= pc_o + 1;
          instr_bus.valid <= 1'b1;
          instr_bus.instr <= 1'b0;
          sequencer_state <= SEQ_READ_DST_OPERAND;
        end

        SEQ_READ_DST_OPERAND: begin
          if (instr_bus.ready) begin
            prefetch_dst_operand <= instr_bus.read_data;
            // 3-word instruction. Advance PC past everything.
            pc_o <= pc_plus_2;
            instr_bus.valid <= 1'b0;
            if (!exec_busy_i) begin
              op_o <= prefetch_op;
              src_operand_o <= prefetch_src_operand;
              dst_operand_o <= instr_bus.read_data;
              done_o <= 1'b1;
              sequencer_state <= SEQ_START;
            end else begin
              sequencer_state <= SEQ_READY;
            end
          end
        end

        // Fetch is done but execute is still busy with the previous
        // instruction. Wait here until it finishes, then promote
        // the prefetch buffer to outputs and hand off.
        SEQ_READY: begin
          if (!exec_busy_i) begin
            op_o <= prefetch_op;
            src_operand_o <= prefetch_src_operand;
            dst_operand_o <= prefetch_dst_operand;
            done_o <= 1'b1;
            sequencer_state <= SEQ_START;
          end
        end
      endcase
    end
  end

endmodule : sequencer
